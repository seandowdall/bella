use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{net::IpAddr, time::Duration};

use crate::AppState;

const SESSION_COOKIE: &str = "bella_session";

pub async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if let Some(response) = enforce_csrf(&state, &request) {
        return response;
    }
    if let Some(response) = enforce_rate_limit(
        &state,
        request.method().clone(),
        request.uri().path().to_owned(),
        client_key(request.headers()),
    )
    .await
    {
        return response;
    }

    let mut response = next.run(request).await;
    add_security_headers(response.headers_mut());
    response
}

fn enforce_csrf(state: &AppState, request: &Request) -> Option<Response> {
    if is_safe_method(request.method()) || has_bearer_token(request.headers()) {
        return None;
    }
    if !has_session_cookie(request.headers()) {
        return None;
    }
    if trusted_request_origin(state, request.headers()) {
        return None;
    }
    Some(error_response(
        StatusCode::FORBIDDEN,
        "trusted browser origin required",
    ))
}

async fn enforce_rate_limit(
    state: &AppState,
    method: Method,
    path: String,
    client_key: Option<String>,
) -> Option<Response> {
    let policy = rate_limit_policy(&method, &path)?;
    let key = format!(
        "{}:{}",
        policy.name,
        client_key.as_deref().unwrap_or("unknown")
    );

    match check_rate_limit(state, &key, policy.limit, policy.window).await {
        Ok(true) => None,
        Ok(false) => Some(error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "too many requests",
        )),
        Err(error) => {
            tracing::error!(%error, "rate limit check failed");
            Some(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "request guard failed",
            ))
        }
    }
}

async fn check_rate_limit(
    state: &AppState,
    key: &str,
    limit: i64,
    window: Duration,
) -> Result<bool, sqlx::Error> {
    let mut transaction = state.db.begin().await?;
    sqlx::query("delete from rate_limit_hits where hit_at < now() - interval '10 minutes'")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(key)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "delete from rate_limit_hits
         where key = $1
           and hit_at < now() - ($2::bigint * interval '1 second')",
    )
    .bind(key)
    .bind(window.as_secs() as i64)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("insert into rate_limit_hits (key) values ($1)")
        .bind(key)
        .execute(&mut *transaction)
        .await?;
    let hits: i64 = sqlx::query_scalar("select count(*) from rate_limit_hits where key = $1")
        .bind(key)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(hits <= limit)
}

fn add_security_headers(headers: &mut HeaderMap) {
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; frame-ancestors 'none'"
            .parse()
            .unwrap(),
    );
}

fn trusted_request_origin(state: &AppState, headers: &HeaderMap) -> bool {
    header_origin(headers, header::ORIGIN)
        .or_else(|| referer_origin(headers))
        .is_some_and(|origin| {
            state
                .config
                .allowed_origins
                .iter()
                .any(|allowed| allowed == &origin)
        })
}

fn header_origin(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    parse_origin(value)
}

fn referer_origin(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::REFERER)?.to_str().ok()?.trim();
    parse_origin(value)
}

fn parse_origin(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    Some(url.origin().ascii_serialization())
}

fn has_bearer_token(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim().starts_with("Bearer "))
}

fn has_session_cookie(headers: &HeaderMap) -> bool {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(';').map(str::trim).any(|cookie| {
                cookie.starts_with(SESSION_COOKIE) && cookie.as_bytes().get(13) == Some(&b'=')
            })
        })
}

fn is_safe_method(method: &Method) -> bool {
    matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

struct RateLimitPolicy {
    name: &'static str,
    limit: i64,
    window: Duration,
}

fn rate_limit_policy(method: &Method, path: &str) -> Option<RateLimitPolicy> {
    if path == "/v1/auth/github/start" || path == "/v1/auth/github/callback" {
        return Some(RateLimitPolicy {
            name: "auth_oauth",
            limit: 30,
            window: Duration::from_secs(60),
        });
    }
    if path == "/v1/integrations/github/callback" {
        return Some(RateLimitPolicy {
            name: "github_install_callback",
            limit: 60,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::GET && path.ends_with("/integrations/github/start") {
        return Some(RateLimitPolicy {
            name: "github_install_start",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::GET && path.ends_with("/integrations/github/repositories") {
        return Some(RateLimitPolicy {
            name: "github_repositories_refresh",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path == "/v1/github/webhook" {
        return Some(RateLimitPolicy {
            name: "github_webhook",
            limit: 600,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path == "/v1/auth/cli/start" {
        return Some(RateLimitPolicy {
            name: "auth_cli_start",
            limit: 20,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path == "/v1/auth/cli/poll" {
        return Some(RateLimitPolicy {
            name: "auth_cli_poll",
            limit: 120,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path == "/v1/auth/logout" {
        return Some(RateLimitPolicy {
            name: "auth_logout",
            limit: 60,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path.ends_with("/provider-accounts") {
        return Some(RateLimitPolicy {
            name: "provider_account_write",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::PATCH && path.ends_with("/integrations/posthog") {
        return Some(RateLimitPolicy {
            name: "integration_write",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::DELETE && path.ends_with("/integrations/posthog") {
        return Some(RateLimitPolicy {
            name: "integration_write",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::DELETE && path.ends_with("/integrations/github") {
        return Some(RateLimitPolicy {
            name: "integration_write",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path.ends_with("/integrations/posthog/webhook-secret/rotate") {
        return Some(RateLimitPolicy {
            name: "integration_write",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path.ends_with("/integrations/slack/install-url") {
        return Some(RateLimitPolicy {
            name: "integration_write",
            limit: 20,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path.ends_with("/invitations") {
        return Some(RateLimitPolicy {
            name: "organization_invitation_write",
            limit: 10,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path == "/v1/invitations/accept" {
        return Some(RateLimitPolicy {
            name: "organization_invitation_accept",
            limit: 30,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::DELETE && path.contains("/invitations/") {
        return Some(RateLimitPolicy {
            name: "organization_invitation_revoke",
            limit: 60,
            window: Duration::from_secs(300),
        });
    }
    if (method == Method::PATCH || method == Method::DELETE) && path.contains("/members/") {
        return Some(RateLimitPolicy {
            name: "organization_member_write",
            limit: 60,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path.ends_with("/sdk/usage-events") {
        return Some(RateLimitPolicy {
            name: "sdk_ingestion",
            limit: 600,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path.ends_with("/webhooks/posthog") {
        return Some(RateLimitPolicy {
            name: "posthog_webhook",
            limit: 300,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path == "/v1/slack/events" {
        return Some(RateLimitPolicy {
            name: "slack_events",
            limit: 600,
            window: Duration::from_secs(60),
        });
    }
    if method == Method::POST && path.ends_with("/sync") {
        return Some(RateLimitPolicy {
            name: "provider_sync",
            limit: 10,
            window: Duration::from_secs(300),
        });
    }
    if method == Method::POST && path.ends_with("/agent/messages") {
        return Some(RateLimitPolicy {
            name: "agent_messages",
            limit: 60,
            window: Duration::from_secs(60),
        });
    }
    None
}

fn client_key(headers: &HeaderMap) -> Option<String> {
    let address = headers
        .get("x-real-ip")?
        .to_str()
        .ok()?
        .trim()
        .parse::<IpAddr>()
        .ok()?;
    Some(format!("ip:{address}"))
}

fn error_response(status: StatusCode, message: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::{client_key, has_session_cookie, parse_origin, rate_limit_policy};
    use axum::http::{HeaderMap, Method, header};

    #[test]
    fn parses_origin_from_url() {
        assert_eq!(
            parse_origin("https://app.bella.md/somewhere"),
            Some("https://app.bella.md".to_string())
        );
    }

    #[test]
    fn detects_session_cookie_name_exactly() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "other=1; bella_session=abc; bella_session_extra=no"
                .parse()
                .unwrap(),
        );
        assert!(has_session_cookie(&headers));
    }

    #[test]
    fn uses_railway_client_address_and_ignores_spoofable_forwarding_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.10".parse().unwrap());
        headers.insert("x-forwarded-for", "198.51.100.4".parse().unwrap());

        assert_eq!(client_key(&headers).as_deref(), Some("ip:203.0.113.10"));
    }

    #[test]
    fn rejects_invalid_railway_client_addresses() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "not-an-address".parse().unwrap());

        assert_eq!(client_key(&headers), None);
    }

    #[test]
    fn rate_limits_expensive_routes() {
        assert!(rate_limit_policy(&Method::POST, "/v1/auth/cli/start").is_some());
        assert!(
            rate_limit_policy(
                &Method::GET,
                "/v1/organizations/00000000-0000-0000-0000-000000000000/integrations/github/start"
            )
            .is_some()
        );
        assert!(
            rate_limit_policy(
                &Method::GET,
                "/v1/organizations/00000000-0000-0000-0000-000000000000/integrations/github/repositories"
            )
            .is_some()
        );
        assert!(
            rate_limit_policy(
                &Method::POST,
                "/v1/organizations/00000000-0000-0000-0000-000000000000/agent/messages"
            )
            .is_some()
        );
        assert!(
            rate_limit_policy(
                &Method::POST,
                "/v1/organizations/00000000-0000-0000-0000-000000000000/invitations"
            )
            .is_some()
        );
        assert!(rate_limit_policy(&Method::POST, "/v1/invitations/accept").is_some());
        assert!(rate_limit_policy(&Method::GET, "/v1/providers").is_none());
    }
}

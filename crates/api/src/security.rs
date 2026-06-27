use axum::{
    Json,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::AppState;

const SESSION_COOKIE: &str = "bella_session";

pub struct RateLimiter {
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    fn check(&self, key: String, limit: usize, window: Duration) -> bool {
        let now = Instant::now();
        let cutoff = now - window;
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");
        if buckets.len() > 10_000 {
            buckets.retain(|_, hits| hits.back().is_some_and(|last| *last >= cutoff));
        }
        let hits = buckets.entry(key).or_default();
        while hits.front().is_some_and(|hit| *hit < cutoff) {
            hits.pop_front();
        }
        if hits.len() >= limit {
            return false;
        }
        hits.push_back(now);
        true
    }
}

pub async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if let Some(response) = enforce_csrf(&state, &request) {
        return response;
    }
    if let Some(response) = enforce_rate_limit(&state, &request) {
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

fn enforce_rate_limit(state: &AppState, request: &Request) -> Option<Response> {
    let policy = rate_limit_policy(request.method(), request.uri().path())?;
    let peer_addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string());
    let key = format!(
        "{}:{}",
        policy.name,
        client_key(
            request.headers(),
            peer_addr.as_deref(),
            state.config.trust_proxy_headers
        )
    );
    if state.rate_limiter.check(key, policy.limit, policy.window) {
        None
    } else {
        Some(error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "too many requests",
        ))
    }
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
    limit: usize,
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
    if method == Method::POST && path.ends_with("/integrations/posthog/webhook-secret/rotate") {
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

fn client_key<'a>(
    headers: &'a HeaderMap,
    peer_addr: Option<&'a str>,
    trust_proxy_headers: bool,
) -> &'a str {
    if trust_proxy_headers
        && let Some(forwarded) = ["fly-client-ip", "x-real-ip", "x-forwarded-for"]
            .into_iter()
            .find_map(|name| {
                headers
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.split(',').next())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
    {
        return forwarded;
    }
    peer_addr.unwrap_or("unknown")
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
    fn rate_limits_expensive_routes() {
        assert!(rate_limit_policy(&Method::POST, "/v1/auth/cli/start").is_some());
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

    #[test]
    fn ignores_forwarded_client_headers_unless_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.10".parse().unwrap());

        assert_eq!(
            client_key(&headers, Some("198.51.100.3"), false),
            "198.51.100.3"
        );
        assert_eq!(
            client_key(&headers, Some("198.51.100.3"), true),
            "203.0.113.10"
        );
    }
}

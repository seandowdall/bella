use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{RngCore, rngs::OsRng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

const SESSION_COOKIE: &str = "bella_session";
const OAUTH_BROWSER_COOKIE: &str = "bella_oauth_browser";

#[derive(Debug, Serialize)]
pub struct AuthUser {
    pub(crate) id: Uuid,
    pub(crate) github_login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
pub struct StartQuery {
    return_to: Option<String>,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Deserialize)]
pub struct CliStartRequest {
    poll_secret: String,
}

#[derive(Serialize)]
pub struct CliStartResponse {
    request_id: Uuid,
    verification_url: String,
    expires_in: i64,
    interval: u64,
}

#[derive(Deserialize)]
pub struct CliPollRequest {
    request_id: Uuid,
    poll_secret: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CliPollResponse {
    Pending,
    Complete { token: String, user: AuthUser },
}

#[derive(Deserialize)]
struct GithubTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GithubUser {
    id: i64,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

struct OAuthFlow {
    flow_kind: String,
    cli_request_id: Option<Uuid>,
    browser_nonce_hash: Option<String>,
    return_to: Option<String>,
}

pub async fn web_start(
    State(state): State<AppState>,
    Query(query): Query<StartQuery>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AuthError> {
    cleanup_expired_auth_records(&state).await?;
    let return_to = query
        .return_to
        .filter(|value| is_safe_return_to(value, &state.config.web_url))
        .unwrap_or_else(|| state.config.web_url.clone());
    let browser_nonce = jar
        .get(OAUTH_BROWSER_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .unwrap_or_else(random_token);
    let url = create_oauth_flow(
        &state,
        "web",
        None,
        Some(hash_token(&browser_nonce)),
        Some(return_to),
    )
    .await?;
    let cookie = Cookie::build((OAUTH_BROWSER_COOKIE, browser_nonce))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.secure_cookies)
        .max_age(time::Duration::minutes(10))
        .build();
    Ok((jar.add(cookie), Redirect::temporary(&url)))
}

pub async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AuthError> {
    let flow = consume_oauth_flow(&state, &query.state).await?;
    if flow.flow_kind == "web" {
        let browser_nonce = jar
            .get(OAUTH_BROWSER_COOKIE)
            .ok_or(AuthError::InvalidFlow)?;
        if flow.browser_nonce_hash.as_deref() != Some(&hash_token(browser_nonce.value())) {
            return Err(AuthError::InvalidFlow);
        }
    }
    let github_user = fetch_github_user(&state, &query.code).await?;
    let user = upsert_user(&state, github_user).await?;
    crate::organizations::ensure_default_organization(&state, &user).await?;

    match flow.flow_kind.as_str() {
        "web" => {
            let session_token = random_token();
            sqlx::query(
                "insert into web_sessions (token_hash, user_id, expires_at) values ($1, $2, $3)",
            )
            .bind(hash_token(&session_token))
            .bind(user.id)
            .bind(Utc::now() + ChronoDuration::days(30))
            .execute(&state.db)
            .await?;

            let cookie = Cookie::build((SESSION_COOKIE, session_token))
                .path("/")
                .http_only(true)
                .same_site(SameSite::Lax)
                .secure(state.config.secure_cookies)
                .max_age(time::Duration::days(30))
                .build();
            Ok((
                jar.add(cookie),
                Redirect::to(
                    flow.return_to
                        .as_deref()
                        .unwrap_or(state.config.web_url.as_str()),
                ),
            ))
        }
        "cli" => {
            let request_id = flow.cli_request_id.ok_or(AuthError::InvalidFlow)?;
            let api_token = random_token();
            let mut transaction = state.db.begin().await?;
            sqlx::query(
                "insert into api_tokens (token_hash, user_id, label) values ($1, $2, 'bella cli')",
            )
            .bind(hash_token(&api_token))
            .bind(user.id)
            .execute(&mut *transaction)
            .await?;
            let result = sqlx::query(
                "update cli_login_requests
                 set user_id = $1, api_token = $2
                 where id = $3 and expires_at > now()",
            )
            .bind(user.id)
            .bind(api_token)
            .bind(request_id)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                return Err(AuthError::InvalidFlow);
            }
            transaction.commit().await?;
            Ok((
                jar,
                Redirect::to(&format!(
                    "{}/auth/cli/success",
                    state.config.web_url.trim_end_matches('/')
                )),
            ))
        }
        _ => Err(AuthError::InvalidFlow),
    }
}

pub async fn me(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<AuthUser>, AuthError> {
    Ok(Json(authenticated_user(&state, &jar, &headers).await?))
}

pub async fn authenticated_user(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
) -> Result<AuthUser, AuthError> {
    let token = bearer_token(headers)
        .or_else(|| {
            jar.get(SESSION_COOKIE)
                .map(|cookie| cookie.value().to_owned())
        })
        .ok_or(AuthError::Unauthorized)?;
    find_user_by_token(state, &token).await
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<CookieJar, AuthError> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        sqlx::query("delete from web_sessions where token_hash = $1")
            .bind(hash_token(cookie.value()))
            .execute(&state.db)
            .await?;
    }

    let removal = Cookie::build(SESSION_COOKIE)
        .path("/")
        .secure(state.config.secure_cookies)
        .build();
    Ok(jar.remove(removal))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AuthError> {
    let token = bearer_token(&headers).ok_or(AuthError::Unauthorized)?;
    let result = sqlx::query(
        "update api_tokens set revoked_at = now()
         where token_hash = $1 and revoked_at is null",
    )
    .bind(hash_token(&token))
    .execute(&state.db)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AuthError::Unauthorized);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cli_start(
    State(state): State<AppState>,
    Json(request): Json<CliStartRequest>,
) -> Result<Json<CliStartResponse>, AuthError> {
    cleanup_expired_auth_records(&state).await?;
    if request.poll_secret.len() < 32 {
        return Err(AuthError::BadRequest("poll_secret is too short"));
    }

    let request_id = Uuid::new_v4();
    let expires_in = 600;
    sqlx::query(
        "insert into cli_login_requests (id, poll_secret_hash, expires_at)
         values ($1, $2, $3)",
    )
    .bind(request_id)
    .bind(hash_token(&request.poll_secret))
    .bind(Utc::now() + ChronoDuration::seconds(expires_in))
    .execute(&state.db)
    .await?;

    let verification_url = create_oauth_flow(&state, "cli", Some(request_id), None, None).await?;

    Ok(Json(CliStartResponse {
        request_id,
        verification_url,
        expires_in,
        interval: 2,
    }))
}

pub async fn cli_poll(
    State(state): State<AppState>,
    Json(request): Json<CliPollRequest>,
) -> Result<Json<CliPollResponse>, AuthError> {
    let row = sqlx::query(
        "select user_id, api_token, expires_at
         from cli_login_requests
         where id = $1 and poll_secret_hash = $2",
    )
    .bind(request.request_id)
    .bind(hash_token(&request.poll_secret))
    .fetch_optional(&state.db)
    .await?
    .ok_or(AuthError::InvalidFlow)?;

    let expires_at: chrono::DateTime<Utc> = row.try_get("expires_at")?;
    if expires_at <= Utc::now() {
        return Err(AuthError::InvalidFlow);
    }

    let Some(token) = row.try_get::<Option<String>, _>("api_token")? else {
        return Ok(Json(CliPollResponse::Pending));
    };
    let user_id: Uuid = row
        .try_get::<Option<Uuid>, _>("user_id")?
        .ok_or(AuthError::InvalidFlow)?;
    let user = find_user_by_id(&state, user_id).await?;

    Ok(Json(CliPollResponse::Complete { token, user }))
}

async fn create_oauth_flow(
    state: &AppState,
    flow_kind: &str,
    cli_request_id: Option<Uuid>,
    browser_nonce_hash: Option<String>,
    return_to: Option<String>,
) -> Result<String, AuthError> {
    let oauth_state = random_token();
    sqlx::query(
        "insert into oauth_flows
         (state_hash, flow_kind, cli_request_id, browser_nonce_hash, return_to, expires_at)
         values ($1, $2, $3, $4, $5, $6)",
    )
    .bind(hash_token(&oauth_state))
    .bind(flow_kind)
    .bind(cli_request_id)
    .bind(browser_nonce_hash)
    .bind(return_to)
    .bind(Utc::now() + ChronoDuration::minutes(10))
    .execute(&state.db)
    .await?;

    let callback_url = format!(
        "{}/v1/auth/github/callback",
        state.config.public_api_url.trim_end_matches('/')
    );
    let mut url = reqwest::Url::parse("https://github.com/login/oauth/authorize")
        .map_err(|_| AuthError::Configuration)?;
    url.query_pairs_mut()
        .append_pair("client_id", &state.config.github_client_id)
        .append_pair("redirect_uri", &callback_url)
        .append_pair("scope", "read:user")
        .append_pair("state", &oauth_state);
    Ok(url.to_string())
}

async fn consume_oauth_flow(state: &AppState, oauth_state: &str) -> Result<OAuthFlow, AuthError> {
    let row = sqlx::query(
        "delete from oauth_flows
         where state_hash = $1 and expires_at > now()
         returning flow_kind, cli_request_id, browser_nonce_hash, return_to",
    )
    .bind(hash_token(oauth_state))
    .fetch_optional(&state.db)
    .await?
    .ok_or(AuthError::InvalidFlow)?;

    Ok(OAuthFlow {
        flow_kind: row.try_get("flow_kind")?,
        cli_request_id: row.try_get("cli_request_id")?,
        browser_nonce_hash: row.try_get("browser_nonce_hash")?,
        return_to: row.try_get("return_to")?,
    })
}

async fn fetch_github_user(state: &AppState, code: &str) -> Result<GithubUser, AuthError> {
    let callback_url = format!(
        "{}/v1/auth/github/callback",
        state.config.public_api_url.trim_end_matches('/')
    );
    let client = Client::new();
    let token = client
        .post("https://github.com/login/oauth/access_token")
        .header(header::ACCEPT, "application/json")
        .json(&serde_json::json!({
            "client_id": state.config.github_client_id,
            "client_secret": state.config.github_client_secret,
            "code": code,
            "redirect_uri": callback_url,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<GithubTokenResponse>()
        .await?;

    client
        .get("https://api.github.com/user")
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::USER_AGENT, "bella")
        .bearer_auth(token.access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .map_err(AuthError::from)
}

async fn upsert_user(state: &AppState, github: GithubUser) -> Result<AuthUser, AuthError> {
    let row = sqlx::query(
        "insert into users (id, github_user_id, github_login, name, avatar_url)
         values ($1, $2, $3, $4, $5)
         on conflict (github_user_id) do update
         set github_login = excluded.github_login,
             name = excluded.name,
             avatar_url = excluded.avatar_url,
             updated_at = now()
         returning id, github_login, name, avatar_url",
    )
    .bind(Uuid::new_v4())
    .bind(github.id)
    .bind(github.login)
    .bind(github.name)
    .bind(github.avatar_url)
    .fetch_one(&state.db)
    .await?;
    user_from_row(&row)
}

async fn find_user_by_token(state: &AppState, token: &str) -> Result<AuthUser, AuthError> {
    let token_hash = hash_token(token);
    let row = sqlx::query(
        "select u.id, u.github_login, u.name, u.avatar_url
         from users u
         left join web_sessions s
           on s.user_id = u.id and s.token_hash = $1 and s.expires_at > now()
         left join api_tokens t
           on t.user_id = u.id and t.token_hash = $1 and t.revoked_at is null
         where s.token_hash is not null or t.token_hash is not null",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AuthError::Unauthorized)?;

    sqlx::query("update api_tokens set last_used_at = now() where token_hash = $1")
        .bind(token_hash)
        .execute(&state.db)
        .await?;
    user_from_row(&row)
}

async fn find_user_by_id(state: &AppState, user_id: Uuid) -> Result<AuthUser, AuthError> {
    let row = sqlx::query("select id, github_login, name, avatar_url from users where id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    user_from_row(&row)
}

async fn cleanup_expired_auth_records(state: &AppState) -> Result<(), AuthError> {
    let mut transaction = state.db.begin().await?;
    sqlx::query("delete from oauth_flows where expires_at <= now()")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("delete from cli_login_requests where expires_at <= now()")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("delete from web_sessions where expires_at <= now()")
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

fn user_from_row(row: &sqlx::postgres::PgRow) -> Result<AuthUser, AuthError> {
    Ok(AuthUser {
        id: row.try_get("id")?,
        github_login: row.try_get("github_login")?,
        name: row.try_get("name")?,
        avatar_url: row.try_get("avatar_url")?,
    })
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_owned)
}

fn is_safe_return_to(value: &str, web_url: &str) -> bool {
    reqwest::Url::parse(value)
        .ok()
        .zip(reqwest::Url::parse(web_url).ok())
        .is_some_and(|(candidate, configured)| candidate.origin() == configured.origin())
}

#[derive(Debug)]
pub enum AuthError {
    Unauthorized,
    InvalidFlow,
    BadRequest(&'static str),
    Configuration,
    Database(sqlx::Error),
    Http(reqwest::Error),
}

impl From<sqlx::Error> for AuthError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<reqwest::Error> for AuthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication required"),
            Self::InvalidFlow => (StatusCode::BAD_REQUEST, "invalid or expired login flow"),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Configuration => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid auth configuration",
            ),
            Self::Database(error) => {
                tracing::error!(%error, "authentication database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "authentication failed")
            }
            Self::Http(error) => {
                tracing::error!(%error, "GitHub OAuth request failed");
                (StatusCode::BAD_GATEWAY, "GitHub authentication failed")
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{hash_token, is_safe_return_to, random_token};

    #[test]
    fn return_url_must_match_configured_web_origin() {
        let web_url = "https://bella.example.com";

        assert!(is_safe_return_to(
            "https://bella.example.com/settings",
            web_url
        ));
        assert!(!is_safe_return_to(
            "https://attacker.example.com/settings",
            web_url
        ));
        assert!(!is_safe_return_to("not a url", web_url));
    }

    #[test]
    fn random_tokens_are_unique_and_hash_stably() {
        let first = random_token();
        let second = random_token();

        assert_ne!(first, second);
        assert_eq!(hash_token(&first), hash_token(&first));
        assert_ne!(hash_token(&first), first);
    }
}

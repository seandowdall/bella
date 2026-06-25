use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bella_slack::{PostedMessage, SlackClientError};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState, SlackCloudConfig,
    auth::{AuthError, authenticated_user},
};

const SLACK_OAUTH_SCOPE: &str = "chat:write,channels:read,groups:read";
const SLACK_OAUTH_STATE_TTL_MINUTES: i64 = 10;

#[derive(Debug, Deserialize)]
pub struct InstallUrlRequest {
    return_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallUrlResponse {
    install_url: String,
    expires_in: i64,
}

pub async fn install_url(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    request: Option<Json<InstallUrlRequest>>,
) -> Result<Json<InstallUrlResponse>, SlackError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_admin_membership(&state, user.id, organization_id).await?;
    let slack_cloud = state
        .config
        .slack_cloud
        .as_ref()
        .ok_or(SlackError::CloudNotConfigured)?;
    cleanup_expired_slack_oauth_states(&state).await?;

    let oauth_state = random_token();
    let expires_in = SLACK_OAUTH_STATE_TTL_MINUTES * 60;
    let return_to = request
        .and_then(|Json(request)| request.return_to)
        .filter(|value| is_safe_return_to(value, &state.config.web_url));
    sqlx::query(
        "insert into slack_oauth_states
         (state_hash, organization_id, user_id, return_to, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(hash_token(&oauth_state))
    .bind(organization_id)
    .bind(user.id)
    .bind(return_to)
    .bind(Utc::now() + ChronoDuration::minutes(SLACK_OAUTH_STATE_TTL_MINUTES))
    .execute(&state.db)
    .await?;

    Ok(Json(InstallUrlResponse {
        install_url: build_slack_install_url(slack_cloud, &oauth_state)?,
        expires_in,
    }))
}

pub async fn send_test_message(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<PostedMessage>, SlackError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_admin_membership(&state, user.id, organization_id).await?;
    let slack_client = state
        .slack_client
        .as_ref()
        .ok_or(SlackError::NotConfigured)?;
    if slack_client.organization_id() != organization_id {
        return Err(SlackError::NotConfiguredForOrganization);
    }

    Ok(Json(slack_client.post_test_message().await?))
}

fn build_slack_install_url(
    config: &SlackCloudConfig,
    oauth_state: &str,
) -> Result<String, SlackError> {
    let mut url = reqwest::Url::parse("https://slack.com/oauth/v2/authorize")
        .map_err(|_| SlackError::Configuration)?;
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("scope", SLACK_OAUTH_SCOPE)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("state", oauth_state);
    Ok(url.to_string())
}

async fn cleanup_expired_slack_oauth_states(state: &AppState) -> Result<(), SlackError> {
    sqlx::query("delete from slack_oauth_states where expires_at <= now()")
        .execute(&state.db)
        .await?;
    Ok(())
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn is_safe_return_to(value: &str, web_url: &str) -> bool {
    let Ok(return_to) = reqwest::Url::parse(value) else {
        return false;
    };
    let Ok(web) = reqwest::Url::parse(web_url) else {
        return false;
    };
    return_to.origin() == web.origin()
}

async fn require_admin_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<(), SlackError> {
    let role = sqlx::query(
        "select role from organization_memberships where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(SlackError::NotFound)?;

    if !matches!(role.as_str(), "owner" | "admin") {
        return Err(SlackError::Forbidden);
    }
    Ok(())
}

#[derive(Debug)]
pub enum SlackError {
    Auth(AuthError),
    Client(SlackClientError),
    Configuration,
    CloudNotConfigured,
    Database(sqlx::Error),
    Forbidden,
    NotConfigured,
    NotConfiguredForOrganization,
    NotFound,
}

impl From<AuthError> for SlackError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<SlackClientError> for SlackError {
    fn from(error: SlackClientError) -> Self {
        Self::Client(error)
    }
}

impl From<sqlx::Error> for SlackError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for SlackError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "organization admin access required" })),
            )
                .into_response(),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "organization not found" })),
            )
                .into_response(),
            Self::NotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Slack integration is not configured" })),
            )
                .into_response(),
            Self::CloudNotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Slack Cloud installation is not configured"
                })),
            )
                .into_response(),
            Self::Configuration => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Slack integration is misconfigured" })),
            )
                .into_response(),
            Self::NotConfiguredForOrganization => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Slack is configured for a different organization"
                })),
            )
                .into_response(),
            Self::Client(SlackClientError::ChannelArchived) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Slack channel is archived" })),
            )
                .into_response(),
            Self::Client(SlackClientError::ChannelNotFound) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Slack channel was not found" })),
            )
                .into_response(),
            Self::Client(SlackClientError::MissingScope) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Slack installation is missing a required scope"
                })),
            )
                .into_response(),
            Self::Client(SlackClientError::NotInChannel) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Slack bot is not in the channel" })),
            )
                .into_response(),
            Self::Client(SlackClientError::RateLimited { .. }) => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "Slack rate limited the message" })),
            )
                .into_response(),
            Self::Client(SlackClientError::Rejected { .. }) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack rejected the message" })),
            )
                .into_response(),
            Self::Client(SlackClientError::TokenRevoked) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Slack installation needs attention"
                })),
            )
                .into_response(),
            Self::Client(SlackClientError::Unavailable) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack is unavailable" })),
            )
                .into_response(),
            Self::Database(error) => {
                tracing::error!(%error, "Slack integration request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Slack integration request failed" })),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SLACK_OAUTH_SCOPE, build_slack_install_url, is_safe_return_to};
    use crate::SlackCloudConfig;

    #[test]
    fn builds_slack_install_url_with_minimal_bot_scopes() {
        let config = SlackCloudConfig::from_values(
            Some("123.abc".to_owned()),
            Some("client-secret".to_owned()),
            Some("signing-secret".to_owned()),
            Some("https://api.bella.example/v1/slack/oauth/callback".to_owned()),
        )
        .unwrap()
        .unwrap();

        let install_url = build_slack_install_url(&config, "state-token").unwrap();
        let url = reqwest::Url::parse(&install_url).unwrap();
        let pairs = url.query_pairs().collect::<Vec<_>>();

        assert_eq!(
            url.as_str().split('?').next().unwrap(),
            "https://slack.com/oauth/v2/authorize"
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "client_id" && value == "123.abc")
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "scope" && value == SLACK_OAUTH_SCOPE)
        );
        assert!(pairs.iter().any(|(key, value)| key == "redirect_uri"
            && value == "https://api.bella.example/v1/slack/oauth/callback"));
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "state" && value == "state-token")
        );
    }

    #[test]
    fn accepts_return_urls_only_on_web_origin() {
        assert!(is_safe_return_to(
            "https://app.bella.example/settings/integrations",
            "https://app.bella.example"
        ));
        assert!(!is_safe_return_to(
            "https://evil.example/settings/integrations",
            "https://app.bella.example"
        ));
        assert!(!is_safe_return_to(
            "/settings/integrations",
            "https://app.bella.example"
        ));
    }
}

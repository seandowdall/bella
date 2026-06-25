use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use bella_slack::{PostedMessage, SlackClientError};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

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

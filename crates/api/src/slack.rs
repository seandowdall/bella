use std::env;

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

const POST_MESSAGE_URL: &str = "https://slack.com/api/chat.postMessage";
const TEST_MESSAGE: &str = "Bella Slack integration is connected.";

#[derive(Clone)]
pub struct SlackConfig {
    bot_token: String,
    default_channel_id: String,
}

impl SlackConfig {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let bot_token = optional_env("BELLA_SLACK_BOT_TOKEN")?;
        let default_channel_id = optional_env("BELLA_SLACK_DEFAULT_CHANNEL_ID")?;

        Self::from_values(bot_token, default_channel_id)
    }

    fn from_values(
        bot_token: Option<String>,
        default_channel_id: Option<String>,
    ) -> anyhow::Result<Option<Self>> {
        match (bot_token, default_channel_id) {
            (None, None) => Ok(None),
            (Some(bot_token), Some(default_channel_id)) => Ok(Some(Self {
                bot_token,
                default_channel_id,
            })),
            _ => anyhow::bail!(
                "BELLA_SLACK_BOT_TOKEN and BELLA_SLACK_DEFAULT_CHANNEL_ID must be set together"
            ),
        }
    }
}

fn optional_env(name: &str) -> anyhow::Result<Option<String>> {
    match env::var(name) {
        Ok(value) => {
            let value = value.trim().to_owned();
            if value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).context(format!("could not read {name}")),
    }
}

#[derive(Clone)]
pub struct SlackClient {
    client: Client,
    bot_token: String,
    default_channel_id: String,
}

impl SlackClient {
    pub fn new(client: Client, config: SlackConfig) -> Self {
        Self {
            client,
            bot_token: config.bot_token,
            default_channel_id: config.default_channel_id,
        }
    }

    pub async fn post_test_message(&self) -> Result<PostedMessage, SlackClientError> {
        self.post_message(&self.default_channel_id, TEST_MESSAGE, None)
            .await
    }

    pub async fn post_message(
        &self,
        channel_id: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<PostedMessage, SlackClientError> {
        let response = self
            .client
            .post(POST_MESSAGE_URL)
            .bearer_auth(&self.bot_token)
            .json(&PostMessageRequest {
                channel: channel_id,
                text,
                thread_ts,
            })
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "Slack chat.postMessage request failed");
                SlackClientError::Unavailable
            })?;

        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), "Slack chat.postMessage returned an error status");
            return Err(SlackClientError::Unavailable);
        }

        let payload: PostMessageResponse = response.json().await.map_err(|error| {
            tracing::warn!(%error, "Slack chat.postMessage returned an invalid response");
            SlackClientError::Unavailable
        })?;
        if !payload.ok {
            tracing::warn!(error = ?payload.error, "Slack chat.postMessage was rejected");
            return Err(SlackClientError::Rejected);
        }

        let channel_id = payload.channel.ok_or_else(|| {
            tracing::warn!("Slack chat.postMessage succeeded without a channel ID");
            SlackClientError::Unavailable
        })?;
        let message_ts = payload.ts.ok_or_else(|| {
            tracing::warn!("Slack chat.postMessage succeeded without a message timestamp");
            SlackClientError::Unavailable
        })?;

        Ok(PostedMessage {
            channel_id,
            message_ts,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct PostedMessage {
    channel_id: String,
    message_ts: String,
}

#[derive(Serialize)]
struct PostMessageRequest<'a> {
    channel: &'a str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_ts: Option<&'a str>,
}

#[derive(Deserialize)]
struct PostMessageResponse {
    ok: bool,
    channel: Option<String>,
    ts: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
pub enum SlackClientError {
    Rejected,
    Unavailable,
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
            Self::Client(SlackClientError::Rejected) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack rejected the message" })),
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
    use super::{PostMessageRequest, SlackConfig};

    #[test]
    fn requires_complete_slack_configuration() {
        assert!(SlackConfig::from_values(Some("xoxb-token".to_owned()), None).is_err());
        assert!(SlackConfig::from_values(None, Some("C123".to_owned())).is_err());
        assert!(SlackConfig::from_values(None, None).unwrap().is_none());
        assert!(
            SlackConfig::from_values(Some("xoxb-token".to_owned()), Some("C123".to_owned()))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn omits_thread_timestamp_for_root_messages() {
        let payload = serde_json::to_value(PostMessageRequest {
            channel: "C123",
            text: "Bella is connected.",
            thread_ts: None,
        })
        .unwrap();

        assert_eq!(payload["channel"], "C123");
        assert!(payload.get("thread_ts").is_none());
    }

    #[test]
    fn includes_thread_timestamp_for_replies() {
        let payload = serde_json::to_value(PostMessageRequest {
            channel: "C123",
            text: "Investigation started.",
            thread_ts: Some("123.456"),
        })
        .unwrap();

        assert_eq!(payload["thread_ts"], "123.456");
    }
}

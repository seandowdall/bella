use std::env;

use anyhow::Context;
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const POST_MESSAGE_URL: &str = "https://slack.com/api/chat.postMessage";
const TEST_MESSAGE: &str = "Bella Slack integration is connected.";

#[derive(Clone)]
pub struct SlackConfig {
    bot_token: String,
    default_channel_id: String,
    organization_id: Uuid,
}

impl SlackConfig {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let bot_token = optional_env("BELLA_SLACK_BOT_TOKEN")?;
        let default_channel_id = optional_env("BELLA_SLACK_DEFAULT_CHANNEL_ID")?;
        let organization_id = optional_env("BELLA_SLACK_ORGANIZATION_ID")?;

        Self::from_values(bot_token, default_channel_id, organization_id)
    }

    fn from_values(
        bot_token: Option<String>,
        default_channel_id: Option<String>,
        organization_id: Option<String>,
    ) -> anyhow::Result<Option<Self>> {
        match (bot_token, default_channel_id, organization_id) {
            (None, None, None) => Ok(None),
            (Some(bot_token), Some(default_channel_id), Some(organization_id)) => Ok(Some(Self {
                bot_token,
                default_channel_id,
                organization_id: organization_id
                    .parse()
                    .context("BELLA_SLACK_ORGANIZATION_ID must be a UUID")?,
            })),
            _ => anyhow::bail!(
                "BELLA_SLACK_BOT_TOKEN, BELLA_SLACK_DEFAULT_CHANNEL_ID, and BELLA_SLACK_ORGANIZATION_ID must be set together"
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
    organization_id: Uuid,
}

impl SlackClient {
    pub fn new(client: Client, config: SlackConfig) -> Self {
        Self {
            client,
            bot_token: config.bot_token,
            default_channel_id: config.default_channel_id,
            organization_id: config.organization_id,
        }
    }

    pub fn organization_id(&self) -> Uuid {
        self.organization_id
    }

    pub async fn post_test_message(&self) -> Result<PostedMessage, SlackClientError> {
        self.post_message(&self.default_channel_id, TEST_MESSAGE, None)
            .await
    }

    pub async fn post_incident_opened(
        &self,
        incident: &IncidentSlackReport,
    ) -> Result<PostedMessage, SlackClientError> {
        self.post_message(
            &self.default_channel_id,
            &render_incident_opened(incident),
            None,
        )
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
    pub channel_id: String,
    pub message_ts: String,
}

#[derive(Debug)]
pub enum SlackClientError {
    Rejected,
    Unavailable,
}

#[derive(Debug)]
pub struct IncidentSlackReport {
    pub severity: String,
    pub source: String,
    pub status: String,
    pub detected_at: DateTime<Utc>,
}

pub fn render_incident_opened(incident: &IncidentSlackReport) -> String {
    format!(
        "*Bella incident opened*\n*Incident details are available in Bella.*\nSeverity: {}\nStatus: {}\nSource: {}\nDetected: {}\n\nBella has started an investigation and will update this thread as evidence is collected.",
        escape_mrkdwn(&incident.severity),
        escape_mrkdwn(&incident.status),
        escape_mrkdwn(&incident.source),
        incident
            .detected_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

fn escape_mrkdwn(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{IncidentSlackReport, PostMessageRequest, SlackConfig, render_incident_opened};

    #[test]
    fn requires_complete_slack_configuration() {
        assert!(SlackConfig::from_values(Some("xoxb-token".to_owned()), None, None).is_err());
        assert!(SlackConfig::from_values(None, Some("C123".to_owned()), None).is_err());
        assert!(
            SlackConfig::from_values(None, None, None)
                .unwrap()
                .is_none()
        );
        assert!(
            SlackConfig::from_values(
                Some("xoxb-token".to_owned()),
                Some("C123".to_owned()),
                Some("7f59d282-04ff-4a74-b5d4-e50bea8feb50".to_owned()),
            )
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

    #[test]
    fn renders_a_safe_incident_root_report() {
        let report = IncidentSlackReport {
            severity: "high".to_owned(),
            source: "posthog".to_owned(),
            status: "triaging".to_owned(),
            detected_at: Utc.with_ymd_and_hms(2026, 6, 23, 14, 3, 0).unwrap(),
        };

        let message = render_incident_opened(&report);

        assert!(message.contains("*Bella incident opened*"));
        assert!(message.contains("*Incident details are available in Bella.*"));
        assert!(message.contains("Severity: high"));
        assert!(message.contains("Detected: 2026-06-23T14:03:00Z"));
    }
}

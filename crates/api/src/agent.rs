use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Duration, Utc};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap as ReqwestHeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState, agent_settings,
    auth::{AuthError, authenticated_user},
};

#[derive(Debug, Deserialize)]
pub struct AgentMessageRequest {
    message: String,
    llm_setting_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct AgentMessageResponse {
    answer: String,
    metric_type: &'static str,
    freshness: Option<String>,
    agent_mode: &'static str,
    sources: Vec<&'static str>,
    suggestions: Vec<&'static str>,
}

pub async fn message(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<AgentMessageRequest>,
) -> Result<Json<AgentMessageResponse>, AgentError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id).await?;

    let normalized = request.message.to_lowercase();
    let mut response = if asks_about_sync(&normalized) {
        sync_status_answer(&state, organization_id).await?
    } else if asks_about_breakdown(&normalized) {
        breakdown_answer(&state, organization_id, days_for_question(&normalized)).await?
    } else {
        spend_answer(&state, organization_id, days_for_question(&normalized)).await?
    };
    if let Some(config) =
        agent_settings::load_agent_llm_config(&state, organization_id, request.llm_setting_id)
            .await
            .map_err(|error| AgentError::Llm(error.to_string()))?
    {
        response.answer = llm_assisted_answer(&state, &config, &request.message, &response).await?;
        response.agent_mode = "llm_assisted";
    }

    Ok(Json(response))
}

async fn llm_assisted_answer(
    state: &AppState,
    config: &agent_settings::AgentLlmConfig,
    user_message: &str,
    deterministic: &AgentMessageResponse,
) -> Result<String, AgentError> {
    let prompt = format!(
        "User question:\n{user_message}\n\nTrusted Bella tool result:\n{}\n\nFreshness:\n{}\n\nSources:\n{}",
        deterministic.answer,
        deterministic
            .freshness
            .as_deref()
            .unwrap_or("No sync freshness available."),
        deterministic.sources.join(", ")
    );
    match config.provider.as_str() {
        "openai" => openai_answer(state, config, &prompt).await,
        "anthropic" => anthropic_answer(state, config, &prompt).await,
        _ => Err(AgentError::Llm(
            "unsupported configured LLM provider".to_owned(),
        )),
    }
}

async fn openai_answer(
    state: &AppState,
    config: &agent_settings::AgentLlmConfig,
    prompt: &str,
) -> Result<String, AgentError> {
    let response = state
        .provider_client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(&config.api_key)
        .json(&serde_json::json!({
            "model": config.model,
            "temperature": 0.2,
            "messages": [
                {
                    "role": "system",
                    "content": "You are Bella, an AI cost visibility assistant. Answer using only the trusted Bella tool result. Do not invent data. Be concise and call out provider-reported freshness when useful."
                },
                { "role": "user", "content": prompt }
            ]
        }))
        .send()
        .await
        .map_err(|error| AgentError::Llm(error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AgentError::Llm(format!(
            "OpenAI BYOK request failed with HTTP {status}: {body}"
        )));
    }
    let body: OpenAiChatResponse = response
        .json()
        .await
        .map_err(|error| AgentError::Llm(error.to_string()))?;
    body.choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| AgentError::Llm("OpenAI returned an empty answer".to_owned()))
}

async fn anthropic_answer(
    state: &AppState,
    config: &agent_settings::AgentLlmConfig,
    prompt: &str,
) -> Result<String, AgentError> {
    let mut headers = ReqwestHeaderMap::new();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&config.api_key)
            .map_err(|error| AgentError::Llm(error.to_string()))?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.remove(AUTHORIZATION);
    let response = state
        .provider_client
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .json(&serde_json::json!({
            "model": config.model,
            "max_tokens": 600,
            "temperature": 0.2,
            "system": "You are Bella, an AI cost visibility assistant. Answer using only the trusted Bella tool result. Do not invent data. Be concise and call out provider-reported freshness when useful.",
            "messages": [
                { "role": "user", "content": prompt }
            ]
        }))
        .send()
        .await
        .map_err(|error| AgentError::Llm(error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AgentError::Llm(format!(
            "Anthropic BYOK request failed with HTTP {status}: {body}"
        )));
    }
    let body: AnthropicMessageResponse = response
        .json()
        .await
        .map_err(|error| AgentError::Llm(error.to_string()))?;
    body.content
        .into_iter()
        .find_map(|part| match part {
            AnthropicContent::Text { text } => Some(text),
            AnthropicContent::Other => None,
        })
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| AgentError::Llm("Anthropic returned an empty answer".to_owned()))
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

async fn spend_answer(
    state: &AppState,
    organization_id: Uuid,
    days: i64,
) -> Result<AgentMessageResponse, AgentError> {
    let (start_at, end_at) = window(days);
    let row = sqlx::query(
        "with spend as (
             select coalesce(sum(c.amount_micros), 0)::bigint as total_spend_micros
             from cost_snapshots c
             join provider_accounts p on p.id = c.provider_account_id
             where p.organization_id = $1
               and c.bucket_start >= $2
               and c.bucket_start < $3
         ), usage as (
             select coalesce(sum(u.input_tokens), 0)::bigint as input_tokens,
                    coalesce(sum(u.output_tokens), 0)::bigint as output_tokens,
                    coalesce(sum(u.request_count), 0)::bigint as request_count
             from usage_buckets u
             join provider_accounts p on p.id = u.provider_account_id
             where p.organization_id = $1
               and u.bucket_start >= $2
               and u.bucket_start < $3
         )
         select spend.total_spend_micros, usage.input_tokens,
                usage.output_tokens, usage.request_count
         from spend, usage",
    )
    .bind(organization_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(&state.db)
    .await?;
    let spend_micros: i64 = row.get("total_spend_micros");
    let input_tokens: i64 = row.get("input_tokens");
    let output_tokens: i64 = row.get("output_tokens");
    let request_count: i64 = row.get("request_count");
    let freshness = freshness(state, organization_id).await?;

    Ok(AgentMessageResponse {
        answer: format!(
            "Provider-reported AI spend for the last {days} day{} is {}. Bella imported {} input tokens, {} output tokens, and {} requests for this period.",
            if days == 1 { "" } else { "s" },
            format_micros(spend_micros),
            format_count(input_tokens),
            format_count(output_tokens),
            format_count(request_count)
        ),
        metric_type: "provider_reported",
        freshness,
        agent_mode: "deterministic",
        sources: vec!["cost_snapshots", "usage_buckets", "provider_accounts"],
        suggestions: default_suggestions(),
    })
}

async fn breakdown_answer(
    state: &AppState,
    organization_id: Uuid,
    days: i64,
) -> Result<AgentMessageResponse, AgentError> {
    let (start_at, end_at) = window(days);
    let rows = sqlx::query(
        "with usage_by_model as (
             select u.provider, u.model,
                    sum(u.input_tokens)::bigint as input_tokens,
                    sum(u.output_tokens)::bigint as output_tokens,
                    sum(u.request_count)::bigint as request_count
             from usage_buckets u
             join provider_accounts p on p.id = u.provider_account_id
             where p.organization_id = $1
               and u.bucket_start >= $2
               and u.bucket_start < $3
             group by u.provider, u.model
         ), costs_by_model as (
             select c.provider, c.model,
                    sum(c.amount_micros)::bigint as amount_micros
             from cost_snapshots c
             join provider_accounts p on p.id = c.provider_account_id
             where p.organization_id = $1
               and c.bucket_start >= $2
               and c.bucket_start < $3
             group by c.provider, c.model
         )
         select coalesce(u.provider, c.provider) as provider,
                coalesce(nullif(u.model, ''), nullif(c.model, ''), 'unknown') as model,
                coalesce(c.amount_micros, 0)::bigint as amount_micros,
                coalesce(u.input_tokens, 0)::bigint as input_tokens,
                coalesce(u.output_tokens, 0)::bigint as output_tokens,
                coalesce(u.request_count, 0)::bigint as request_count
         from usage_by_model u
         full outer join costs_by_model c
           on c.provider = u.provider and c.model = u.model
         order by amount_micros desc, input_tokens desc, model
         limit 5",
    )
    .bind(organization_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_all(&state.db)
    .await?;

    let answer = if rows.is_empty() {
        "I do not see imported provider usage for that period yet. Connect a provider or run a sync, then ask again.".to_owned()
    } else {
        let lines = rows
            .iter()
            .map(|row| {
                format!(
                    "{} {}: {}, {} input tokens, {} output tokens, {} requests",
                    row.get::<String, _>("provider"),
                    row.get::<String, _>("model"),
                    format_micros(row.get("amount_micros")),
                    format_count(row.get("input_tokens")),
                    format_count(row.get("output_tokens")),
                    format_count(row.get("request_count"))
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("Top provider/model usage for the last {days} days: {lines}.")
    };

    Ok(AgentMessageResponse {
        answer,
        metric_type: "provider_reported",
        freshness: freshness(state, organization_id).await?,
        agent_mode: "deterministic",
        sources: vec!["cost_snapshots", "usage_buckets"],
        suggestions: default_suggestions(),
    })
}

async fn sync_status_answer(
    state: &AppState,
    organization_id: Uuid,
) -> Result<AgentMessageResponse, AgentError> {
    let rows = sqlx::query(
        "select provider, display_name, status, last_synced_at, next_sync_at, last_sync_error
         from provider_accounts
         where organization_id = $1
         order by provider, display_name",
    )
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?;

    let answer = if rows.is_empty() {
        "No provider accounts are connected yet. Connect OpenAI from Providers, then Bella can import provider-reported usage and costs.".to_owned()
    } else {
        let lines = rows
            .iter()
            .map(|row| {
                let last_sync = row
                    .get::<Option<DateTime<Utc>>, _>("last_synced_at")
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_else(|| "never".to_owned());
                let error = row
                    .get::<Option<String>, _>("last_sync_error")
                    .map(|value| format!(" Last error: {value}."))
                    .unwrap_or_default();
                format!(
                    "{} {} is {}. Last sync: {}.{}",
                    row.get::<String, _>("provider"),
                    row.get::<String, _>("display_name"),
                    row.get::<String, _>("status"),
                    last_sync,
                    error
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("Provider sync status: {lines}")
    };

    Ok(AgentMessageResponse {
        answer,
        metric_type: "provider_reported",
        freshness: freshness(state, organization_id).await?,
        agent_mode: "deterministic",
        sources: vec!["provider_accounts", "provider_sync_runs"],
        suggestions: default_suggestions(),
    })
}

async fn freshness(state: &AppState, organization_id: Uuid) -> Result<Option<String>, AgentError> {
    let row = sqlx::query(
        "select max(last_synced_at) as last_synced_at
         from provider_accounts
         where organization_id = $1",
    )
    .bind(organization_id)
    .fetch_one(&state.db)
    .await?;
    Ok(row
        .get::<Option<DateTime<Utc>>, _>("last_synced_at")
        .map(|value| format!("Latest provider sync completed at {}.", value.to_rfc3339())))
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<(), AgentError> {
    let exists = sqlx::query(
        "select 1 from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .is_some();
    if !exists {
        return Err(AgentError::NotFound);
    }
    Ok(())
}

fn asks_about_sync(message: &str) -> bool {
    message.contains("sync")
        || message.contains("fresh")
        || message.contains("failing")
        || message.contains("error")
}

fn asks_about_breakdown(message: &str) -> bool {
    message.contains("breakdown")
        || message.contains("provider")
        || message.contains("model")
        || message.contains("which")
}

fn days_for_question(message: &str) -> i64 {
    if message.contains("today") {
        1
    } else if message.contains("7") || message.contains("week") {
        7
    } else if message.contains("90") || message.contains("quarter") {
        90
    } else {
        30
    }
}

fn window(days: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    let end = Utc::now();
    (end - Duration::days(days), end)
}

fn format_micros(value: i64) -> String {
    format!("${:.2}", value as f64 / 1_000_000.0)
}

fn format_count(value: i64) -> String {
    let mut chars = value.to_string().chars().rev().collect::<Vec<_>>();
    let mut formatted = String::new();
    for (index, char) in chars.drain(..).enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(char);
    }
    formatted.chars().rev().collect()
}

fn default_suggestions() -> Vec<&'static str> {
    vec![
        "What is our AI spend today?",
        "Break down spend by provider and model.",
        "When did OpenAI last sync?",
    ]
}

#[derive(Debug)]
pub enum AgentError {
    Auth(AuthError),
    NotFound,
    Llm(String),
    Database(sqlx::Error),
}

impl From<AuthError> for AgentError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for AgentError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for AgentError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "organization not found" })),
            )
                .into_response(),
            Self::Llm(error) => {
                tracing::warn!(%error, "agent LLM request failed");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "error": format!("Bella could not use the configured BYOK model: {error}") })),
                )
                    .into_response()
            }
            Self::Database(error) => {
                tracing::error!(%error, "agent database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "Bella could not query usage data. Restart the API on this branch and make sure database migrations have run."
                    })),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{days_for_question, format_count};

    #[test]
    fn parses_common_time_ranges() {
        assert_eq!(days_for_question("what is spend today"), 1);
        assert_eq!(days_for_question("last 7 days"), 7);
        assert_eq!(days_for_question("last quarter"), 90);
        assert_eq!(days_for_question("last month"), 30);
    }

    #[test]
    fn formats_counts_with_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(120000), "120,000");
    }
}

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

#[derive(Debug, Deserialize)]
pub struct AgentMessageRequest {
    message: String,
}

#[derive(Debug, Serialize)]
pub struct AgentMessageResponse {
    answer: String,
    metric_type: &'static str,
    freshness: Option<String>,
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
    let response = if asks_about_sync(&normalized) {
        sync_status_answer(&state, organization_id).await?
    } else if asks_about_breakdown(&normalized) {
        breakdown_answer(&state, organization_id, days_for_question(&normalized)).await?
    } else {
        spend_answer(&state, organization_id, days_for_question(&normalized)).await?
    };

    Ok(Json(response))
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

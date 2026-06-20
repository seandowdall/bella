use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

#[derive(Debug, Deserialize)]
pub struct SdkUsageEventRequest {
    event_id: String,
    provider_account_id: Uuid,
    provider: String,
    model: Option<String>,
    operation: Option<String>,
    status: SdkUsageStatus,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    usage: Option<SdkUsage>,
    cost: Option<SdkCost>,
    metadata: Option<Map<String, Value>>,
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SdkUsageStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Deserialize)]
struct SdkUsage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SdkCost {
    amount_micros: i64,
    currency: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SdkUsageEventResponse {
    event_id: String,
    accepted: bool,
}

pub async fn record_usage_event(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<SdkUsageEventRequest>,
) -> Result<(StatusCode, Json<SdkUsageEventResponse>), SdkIngestionError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id).await?;
    validate_request(&request)?;

    let account = sqlx::query(
        "select provider
         from provider_accounts
         where id = $1 and organization_id = $2 and status <> 'disabled'",
    )
    .bind(request.provider_account_id)
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(SdkIngestionError::BadRequest(
        "provider_account_id does not belong to organization",
    ))?;
    let account_provider: String = account.get("provider");
    let provider = normalize_token(&request.provider, 80)?;
    if provider != account_provider {
        return Err(SdkIngestionError::BadRequest(
            "provider does not match provider account",
        ));
    }

    let model = normalize_optional(&request.model, 160);
    let operation = normalize_optional(&request.operation, 160);
    let usage = request.usage.as_ref();
    let input_tokens = usage.and_then(|value| value.input_tokens).unwrap_or(0);
    let output_tokens = usage.and_then(|value| value.output_tokens).unwrap_or(0);
    let total_tokens = usage
        .and_then(|value| value.total_tokens)
        .unwrap_or(input_tokens + output_tokens);
    if input_tokens < 0 || output_tokens < 0 || total_tokens < 0 {
        return Err(SdkIngestionError::BadRequest(
            "token counts must be non-negative",
        ));
    }
    let cost_micros = request.cost.as_ref().map(|cost| cost.amount_micros);
    if cost_micros.is_some_and(|value| value < 0) {
        return Err(SdkIngestionError::BadRequest("cost must be non-negative"));
    }
    let currency = request
        .cost
        .as_ref()
        .and_then(|cost| cost.currency.as_deref())
        .map(|value| normalize_token(value, 12))
        .transpose()?
        .unwrap_or_else(|| "usd".to_string());
    let metadata = request.metadata.clone().unwrap_or_default();
    let error_message = request
        .error_message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(1_000).collect::<String>());
    let status = match request.status {
        SdkUsageStatus::Succeeded => "succeeded",
        SdkUsageStatus::Failed => "failed",
    };
    let bucket_start = request
        .started_at
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or(SdkIngestionError::BadRequest("invalid started_at"))?
        .and_utc();
    let bucket_end = bucket_start + Duration::days(1);

    let mut transaction = state.db.begin().await?;
    let inserted = sqlx::query(
        "insert into sdk_usage_events
         (id, organization_id, provider_account_id, event_id, provider, model, operation,
          status, started_at, ended_at, input_tokens, output_tokens, total_tokens,
          cost_micros, currency, request_metadata, error_message)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                 $15, $16, $17)
         on conflict (organization_id, event_id) do nothing
         returning id",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(request.provider_account_id)
    .bind(&request.event_id)
    .bind(&provider)
    .bind(&model)
    .bind(&operation)
    .bind(status)
    .bind(request.started_at)
    .bind(request.ended_at)
    .bind(input_tokens)
    .bind(output_tokens)
    .bind(total_tokens)
    .bind(cost_micros)
    .bind(&currency)
    .bind(Value::Object(metadata))
    .bind(error_message)
    .fetch_optional(&mut *transaction)
    .await?
    .is_some();

    if inserted {
        sqlx::query(
            "insert into usage_buckets
             (id, provider_account_id, provider, bucket_start, bucket_end, model, operation,
              input_tokens, output_tokens, request_count)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1)
             on conflict (provider_account_id, bucket_start, bucket_end, model,
                          project_external_id, user_external_id, api_key_external_id, operation)
             do update set input_tokens = usage_buckets.input_tokens + excluded.input_tokens,
                           output_tokens = usage_buckets.output_tokens + excluded.output_tokens,
                           request_count = usage_buckets.request_count + 1,
                           updated_at = now()",
        )
        .bind(Uuid::new_v4())
        .bind(request.provider_account_id)
        .bind(&provider)
        .bind(bucket_start)
        .bind(bucket_end)
        .bind(&model)
        .bind(&operation)
        .bind(input_tokens)
        .bind(output_tokens)
        .execute(&mut *transaction)
        .await?;

        if let Some(amount_micros) = cost_micros {
            sqlx::query(
                "insert into cost_snapshots
                 (id, provider_account_id, provider, bucket_start, bucket_end, line_item,
                  model, amount_micros, currency)
                 values ($1, $2, $3, $4, $5, 'sdk_usage', $6, $7, $8)
                 on conflict (provider_account_id, bucket_start, bucket_end, line_item,
                              model, project_external_id, currency)
                 do update set amount_micros = cost_snapshots.amount_micros + excluded.amount_micros,
                               updated_at = now()",
            )
            .bind(Uuid::new_v4())
            .bind(request.provider_account_id)
            .bind(&provider)
            .bind(bucket_start)
            .bind(bucket_end)
            .bind(&model)
            .bind(amount_micros)
            .bind(&currency)
            .execute(&mut *transaction)
            .await?;
        }
    }

    transaction.commit().await?;

    let status = if inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(SdkUsageEventResponse {
            event_id: request.event_id,
            accepted: inserted,
        }),
    ))
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<(), SdkIngestionError> {
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
        return Err(SdkIngestionError::NotFound);
    }
    Ok(())
}

fn validate_request(request: &SdkUsageEventRequest) -> Result<(), SdkIngestionError> {
    let event_id = request.event_id.trim();
    if event_id.is_empty() || event_id.chars().count() > 160 {
        return Err(SdkIngestionError::BadRequest(
            "event_id must contain between 1 and 160 characters",
        ));
    }
    if request.started_at > request.ended_at {
        return Err(SdkIngestionError::BadRequest(
            "started_at must be before ended_at",
        ));
    }
    Ok(())
}

fn normalize_optional(value: &Option<String>, max_len: usize) -> String {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(max_len).collect())
        .unwrap_or_default()
}

fn normalize_token(value: &str, max_len: usize) -> Result<String, SdkIngestionError> {
    let value = value.trim().to_lowercase();
    if value.is_empty() || value.chars().count() > max_len {
        return Err(SdkIngestionError::BadRequest("invalid token value"));
    }
    Ok(value)
}

#[derive(Debug)]
pub enum SdkIngestionError {
    Auth(AuthError),
    BadRequest(&'static str),
    NotFound,
    Database(sqlx::Error),
}

impl IntoResponse for SdkIngestionError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::Database(error) => {
                tracing::error!(error = %error, "sdk ingestion request failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

impl From<AuthError> for SdkIngestionError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for SdkIngestionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UsageSummaryResponse {
    start: NaiveDate,
    end: NaiveDate,
    total_spend_micros: i64,
    input_tokens: i64,
    output_tokens: i64,
    request_count: i64,
    daily_spend: Vec<DailySpend>,
    model_breakdown: Vec<ModelBreakdown>,
}

#[derive(Debug, Serialize)]
pub struct DailySpend {
    date: NaiveDate,
    amount_micros: i64,
}

#[derive(Debug, Serialize)]
pub struct ModelBreakdown {
    provider: String,
    model: String,
    amount_micros: i64,
    input_tokens: i64,
    output_tokens: i64,
    request_count: i64,
}

pub async fn summary(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    Query(query): Query<SummaryQuery>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<UsageSummaryResponse>, ReportingError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id).await?;

    let (start_date, end_date, start_at, end_at) = date_window(query)?;

    let spend_row = sqlx::query(
        "select coalesce(sum(c.amount_micros), 0)::bigint as total_spend_micros
         from cost_snapshots c
         join provider_accounts p on p.id = c.provider_account_id
         where p.organization_id = $1
           and c.bucket_start >= $2
           and c.bucket_start < $3",
    )
    .bind(organization_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(&state.db)
    .await?;

    let usage_row = sqlx::query(
        "select coalesce(sum(u.input_tokens), 0)::bigint as input_tokens,
                coalesce(sum(u.output_tokens), 0)::bigint as output_tokens,
                coalesce(sum(u.request_count), 0)::bigint as request_count
         from usage_buckets u
         join provider_accounts p on p.id = u.provider_account_id
         where p.organization_id = $1
           and u.bucket_start >= $2
           and u.bucket_start < $3",
    )
    .bind(organization_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(&state.db)
    .await?;

    let daily_spend = sqlx::query(
        "select c.bucket_start::date as date,
                coalesce(sum(c.amount_micros), 0)::bigint as amount_micros
         from cost_snapshots c
         join provider_accounts p on p.id = c.provider_account_id
         where p.organization_id = $1
           and c.bucket_start >= $2
           and c.bucket_start < $3
         group by c.bucket_start::date
         order by c.bucket_start::date",
    )
    .bind(organization_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_all(&state.db)
    .await?
    .iter()
    .map(|row| DailySpend {
        date: row.get("date"),
        amount_micros: row.get("amount_micros"),
    })
    .collect();

    let model_breakdown = sqlx::query(
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
         order by amount_micros desc, input_tokens desc, model",
    )
    .bind(organization_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_all(&state.db)
    .await?
    .iter()
    .map(|row| ModelBreakdown {
        provider: row.get("provider"),
        model: row.get("model"),
        amount_micros: row.get("amount_micros"),
        input_tokens: row.get("input_tokens"),
        output_tokens: row.get("output_tokens"),
        request_count: row.get("request_count"),
    })
    .collect();

    Ok(Json(UsageSummaryResponse {
        start: start_date,
        end: end_date,
        total_spend_micros: spend_row.get("total_spend_micros"),
        input_tokens: usage_row.get("input_tokens"),
        output_tokens: usage_row.get("output_tokens"),
        request_count: usage_row.get("request_count"),
        daily_spend,
        model_breakdown,
    }))
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<(), ReportingError> {
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
        return Err(ReportingError::NotFound);
    }
    Ok(())
}

fn date_window(
    query: SummaryQuery,
) -> Result<(NaiveDate, NaiveDate, DateTime<Utc>, DateTime<Utc>), ReportingError> {
    let today = Utc::now().date_naive();
    let end = query
        .end
        .as_deref()
        .map(parse_date)
        .transpose()?
        .unwrap_or(today);
    let start = query
        .start
        .as_deref()
        .map(parse_date)
        .transpose()?
        .unwrap_or_else(|| end - chrono::Duration::days(29));
    if start > end {
        return Err(ReportingError::BadRequest("start must be before end"));
    }
    let exclusive_end = end
        .checked_add_days(Days::new(1))
        .ok_or(ReportingError::BadRequest("invalid end date"))?;
    Ok((
        start,
        end,
        start
            .and_hms_opt(0, 0, 0)
            .ok_or(ReportingError::BadRequest("invalid start date"))?
            .and_utc(),
        exclusive_end
            .and_hms_opt(0, 0, 0)
            .ok_or(ReportingError::BadRequest("invalid end date"))?
            .and_utc(),
    ))
}

fn parse_date(value: &str) -> Result<NaiveDate, ReportingError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ReportingError::BadRequest("dates must be YYYY-MM-DD"))
}

#[derive(Debug)]
pub enum ReportingError {
    Auth(AuthError),
    BadRequest(&'static str),
    NotFound,
    Database(sqlx::Error),
}

impl From<AuthError> for ReportingError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for ReportingError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for ReportingError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "organization not found" })),
            )
                .into_response(),
            Self::Database(error) => {
                tracing::error!(%error, "reporting database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "reporting request failed" })),
                )
                    .into_response()
            }
        }
    }
}

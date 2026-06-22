use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct PosthogWebhookQuery {
    secret: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PosthogWebhookResponse {
    accepted: bool,
    signal_id: Uuid,
    incident_id: Uuid,
    incident_status: String,
}

struct NormalizedPosthogSignal {
    signal_type: String,
    source_event_id: String,
    fingerprint: String,
    title: String,
    severity: String,
    detected_at: DateTime<Utc>,
}

pub async fn webhook(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    Query(query): Query<PosthogWebhookQuery>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<PosthogWebhookResponse>), PosthogWebhookError> {
    verify_secret(&state, organization_id, &headers, query.secret.as_deref()).await?;
    ensure_organization(&state, organization_id).await?;

    let normalized = normalize_signal(&payload);
    let mut transaction = state.db.begin().await?;

    let incident_id = Uuid::new_v4();
    let incident = sqlx::query(
        "insert into incidents
         (id, organization_id, title, status, severity, source, fingerprint, detected_at, metadata)
         values ($1, $2, $3, 'triaging', $4, 'posthog', $5, $6, $7)
         on conflict (organization_id, source, fingerprint) where resolved_at is null
         do update set title = excluded.title,
                       severity = excluded.severity,
                       updated_at = now(),
                       metadata = incidents.metadata || excluded.metadata
         returning id, status",
    )
    .bind(incident_id)
    .bind(organization_id)
    .bind(&normalized.title)
    .bind(&normalized.severity)
    .bind(&normalized.fingerprint)
    .bind(normalized.detected_at)
    .bind(serde_json::json!({
        "first_source": "posthog",
        "last_signal_type": normalized.signal_type,
    }))
    .fetch_one(&mut *transaction)
    .await?;
    let incident_id: Uuid = incident.get("id");
    let incident_status: String = incident.get("status");

    let signal = sqlx::query(
        "insert into signals
         (id, organization_id, incident_id, source, signal_type, source_event_id,
          fingerprint, title, severity, payload, received_at)
         values ($1, $2, $3, 'posthog', $4, $5, $6, $7, $8, $9, now())
         on conflict (organization_id, source, source_event_id) where source_event_id is not null
         do update set incident_id = excluded.incident_id,
                       title = excluded.title,
                       severity = excluded.severity,
                       payload = excluded.payload
         returning id, (xmax = 0) as inserted",
    )
    .bind(Uuid::new_v4())
    .bind(organization_id)
    .bind(incident_id)
    .bind(&normalized.signal_type)
    .bind(&normalized.source_event_id)
    .bind(&normalized.fingerprint)
    .bind(&normalized.title)
    .bind(&normalized.severity)
    .bind(&payload)
    .fetch_one(&mut *transaction)
    .await?;
    let signal_id: Uuid = signal.get("id");
    let inserted_signal: bool = signal.get("inserted");

    if inserted_signal {
        sqlx::query(
            "insert into incident_events
             (id, organization_id, incident_id, event_type, title, metadata)
             values ($1, $2, $3, 'signal.received', $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(organization_id)
        .bind(incident_id)
        .bind(format!("PostHog signal received: {}", normalized.title))
        .bind(serde_json::json!({
            "signal_id": signal_id,
            "signal_type": normalized.signal_type,
            "source_event_id": normalized.source_event_id,
        }))
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;

    let status = if inserted_signal {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(PosthogWebhookResponse {
            accepted: inserted_signal,
            signal_id,
            incident_id,
            incident_status,
        }),
    ))
}

async fn verify_secret(
    state: &AppState,
    organization_id: Uuid,
    headers: &HeaderMap,
    query_secret: Option<&str>,
) -> Result<(), PosthogWebhookError> {
    let provided = query_secret
        .or_else(|| header_value(headers, "x-bella-webhook-secret"))
        .or_else(|| header_value(headers, "x-posthog-webhook-secret"))
        .or_else(|| bearer_token(headers));
    let Some(provided) = provided else {
        return Err(PosthogWebhookError::Unauthorized);
    };

    if let Some(expected) = state.config.posthog_webhook_secret.as_deref()
        && constant_time_eq(provided.as_bytes(), expected.as_bytes())
    {
        return Ok(());
    }

    let rows = sqlx::query(
        "select c.credential_ciphertext, c.credential_nonce
         from integrations i
         join integration_credentials c on c.integration_id = i.id
         where i.organization_id = $1
           and i.integration_type = 'posthog'
           and i.status <> 'disabled'
           and c.kind = 'webhook_secret'",
    )
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?;
    if rows.is_empty() && state.config.posthog_webhook_secret.is_none() {
        return Err(PosthogWebhookError::NotConfigured);
    }
    for row in rows {
        let ciphertext: Vec<u8> = row.get("credential_ciphertext");
        let nonce: Vec<u8> = row.get("credential_nonce");
        let plaintext = state
            .credential_cipher
            .decrypt(&ciphertext, &nonce)
            .map_err(|_| PosthogWebhookError::Credential)?;
        if constant_time_eq(provided.as_bytes(), &plaintext) {
            return Ok(());
        }
    }
    Err(PosthogWebhookError::Unauthorized)
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    value.strip_prefix("Bearer ").map(str::trim)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

async fn ensure_organization(
    state: &AppState,
    organization_id: Uuid,
) -> Result<(), PosthogWebhookError> {
    let exists = sqlx::query("select 1 from organizations where id = $1")
        .bind(organization_id)
        .fetch_optional(&state.db)
        .await?
        .is_some();
    if !exists {
        return Err(PosthogWebhookError::NotFound);
    }
    Ok(())
}

fn normalize_signal(payload: &Value) -> NormalizedPosthogSignal {
    let event_name = string_at(payload, &["event"]).unwrap_or("posthog_event");
    let signal_type = if event_name == "$exception" {
        "posthog.exception_event"
    } else if string_at(payload, &["issue", "id"]).is_some()
        || string_at(payload, &["issue", "name"]).is_some()
    {
        "posthog.error_issue"
    } else {
        "posthog.webhook"
    }
    .to_string();

    let payload_hash = payload_hash(payload);
    let source_event_id = first_string(
        payload,
        &[
            &["uuid"],
            &["event_id"],
            &["id"],
            &["issue", "id"],
            &["properties", "$exception_id"],
        ],
    )
    .map(ToOwned::to_owned)
    .unwrap_or_else(|| payload_hash.clone());
    let fingerprint = first_string(
        payload,
        &[
            &["properties", "$exception_fingerprint"],
            &["properties", "$exception_type"],
            &["issue", "id"],
            &["issue", "fingerprint"],
        ],
    )
    .map(|value| truncate(value, 240))
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| source_event_id.clone());
    let title = first_string(
        payload,
        &[
            &["issue", "name"],
            &["issue", "title"],
            &["title"],
            &["name"],
            &["properties", "$exception_message"],
            &["properties", "$exception_type"],
            &["event"],
        ],
    )
    .map(|value| truncate(value, 240))
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| "PostHog error signal".to_string());
    let severity = first_string(
        payload,
        &[&["severity"], &["level"], &["properties", "level"]],
    )
    .map(normalize_severity)
    .unwrap_or_else(|| "unknown".to_string());
    let detected_at = first_string(
        payload,
        &[
            &["timestamp"],
            &["created_at"],
            &["issue", "first_seen"],
            &["properties", "timestamp"],
        ],
    )
    .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
    .map(|value| value.with_timezone(&Utc))
    .unwrap_or_else(Utc::now);

    NormalizedPosthogSignal {
        signal_type,
        source_event_id: truncate(&source_event_id, 160),
        fingerprint,
        title,
        severity,
        detected_at,
    }
}

fn first_string<'a>(payload: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths.iter().find_map(|path| string_at(payload, path))
}

fn string_at<'a>(payload: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = payload;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn payload_hash(payload: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn truncate(value: &str, max_len: usize) -> String {
    value.trim().chars().take(max_len).collect()
}

fn normalize_severity(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "critical" | "fatal" | "panic" => "critical",
        "high" | "error" => "high",
        "medium" | "warning" | "warn" => "medium",
        "low" => "low",
        "info" | "debug" => "info",
        _ => "unknown",
    }
    .to_string()
}

#[derive(Debug)]
pub enum PosthogWebhookError {
    Credential,
    Database(sqlx::Error),
    NotConfigured,
    NotFound,
    Unauthorized,
}

impl IntoResponse for PosthogWebhookError {
    fn into_response(self) -> Response {
        match self {
            Self::Credential => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Self::Database(error) => {
                tracing::error!(error = %error, "PostHog webhook ingestion failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::NotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                "PostHog webhook ingestion is not configured",
            )
                .into_response(),
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
        }
    }
}

impl From<sqlx::Error> for PosthogWebhookError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

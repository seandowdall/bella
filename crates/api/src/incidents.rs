use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

#[derive(Debug, Serialize)]
pub struct IncidentListItem {
    id: Uuid,
    title: String,
    status: String,
    severity: String,
    source: String,
    fingerprint: String,
    signal_count: i64,
    detected_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct IncidentDetail {
    id: Uuid,
    organization_id: Uuid,
    title: String,
    status: String,
    severity: String,
    source: String,
    fingerprint: String,
    summary: Option<String>,
    impact: Option<String>,
    detected_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    metadata: Value,
    signals: Vec<SignalDetail>,
    events: Vec<IncidentEventDetail>,
}

#[derive(Debug, Serialize)]
pub struct SignalDetail {
    id: Uuid,
    source: String,
    signal_type: String,
    source_event_id: Option<String>,
    title: String,
    severity: String,
    payload: Value,
    received_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct IncidentEventDetail {
    id: Uuid,
    event_type: String,
    title: String,
    body: Option<String>,
    metadata: Value,
    created_at: DateTime<Utc>,
}

pub async fn list(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<Vec<IncidentListItem>>, IncidentError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id).await?;

    let incidents = sqlx::query(
        "select i.id, i.title, i.status, i.severity, i.source, i.fingerprint,
                i.detected_at, i.updated_at, i.resolved_at,
                count(s.id)::bigint as signal_count
         from incidents i
         left join signals s on s.incident_id = i.id
         where i.organization_id = $1
         group by i.id
         order by i.detected_at desc
         limit 100",
    )
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|row| IncidentListItem {
        id: row.get("id"),
        title: row.get("title"),
        status: row.get("status"),
        severity: row.get("severity"),
        source: row.get("source"),
        fingerprint: row.get("fingerprint"),
        signal_count: row.get("signal_count"),
        detected_at: row.get("detected_at"),
        updated_at: row.get("updated_at"),
        resolved_at: row.get("resolved_at"),
    })
    .collect();

    Ok(Json(incidents))
}

pub async fn get(
    State(state): State<AppState>,
    Path((organization_id, incident_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<IncidentDetail>, IncidentError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id).await?;

    let row = sqlx::query(
        "select id, organization_id, title, status, severity, source, fingerprint,
                summary, impact, detected_at, resolved_at, metadata
         from incidents
         where id = $1 and organization_id = $2",
    )
    .bind(incident_id)
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(IncidentError::NotFound)?;

    let signals = sqlx::query(
        "select id, source, signal_type, source_event_id, title, severity, payload, received_at
         from signals
         where incident_id = $1 and organization_id = $2
         order by received_at desc",
    )
    .bind(incident_id)
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|row| SignalDetail {
        id: row.get("id"),
        source: row.get("source"),
        signal_type: row.get("signal_type"),
        source_event_id: row.get("source_event_id"),
        title: row.get("title"),
        severity: row.get("severity"),
        payload: row.get("payload"),
        received_at: row.get("received_at"),
    })
    .collect();

    let events = sqlx::query(
        "select id, event_type, title, body, metadata, created_at
         from incident_events
         where incident_id = $1 and organization_id = $2
         order by created_at asc",
    )
    .bind(incident_id)
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|row| IncidentEventDetail {
        id: row.get("id"),
        event_type: row.get("event_type"),
        title: row.get("title"),
        body: row.get("body"),
        metadata: row.get("metadata"),
        created_at: row.get("created_at"),
    })
    .collect();

    Ok(Json(IncidentDetail {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        title: row.get("title"),
        status: row.get("status"),
        severity: row.get("severity"),
        source: row.get("source"),
        fingerprint: row.get("fingerprint"),
        summary: row.get("summary"),
        impact: row.get("impact"),
        detected_at: row.get("detected_at"),
        resolved_at: row.get("resolved_at"),
        metadata: row.get("metadata"),
        signals,
        events,
    }))
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<(), IncidentError> {
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
        return Err(IncidentError::NotFound);
    }
    Ok(())
}

#[derive(Debug)]
pub enum IncidentError {
    Auth(AuthError),
    Database(sqlx::Error),
    NotFound,
}

impl IntoResponse for IncidentError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::Database(error) => {
                tracing::error!(error = %error, "incident request failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
        }
    }
}

impl From<AuthError> for IncidentError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for IncidentError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
    credentials,
};

#[derive(Debug, Serialize)]
pub struct IntegrationResponse {
    id: Uuid,
    integration_type: String,
    display_name: String,
    status: String,
    credential_fingerprint: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct PosthogConnectionResponse {
    integration: IntegrationResponse,
    webhook_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertPosthogRequest {
    display_name: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<Vec<IntegrationResponse>>, IntegrationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, false).await?;

    let rows = sqlx::query(
        "select i.id, i.integration_type, i.display_name, i.status,
                c.credential_fingerprint, i.created_at, i.updated_at
         from integrations i
         left join integration_credentials c
           on c.integration_id = i.id and c.kind = 'webhook_secret'
         where i.organization_id = $1
         order by i.integration_type, i.display_name",
    )
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.iter().map(integration_from_row).collect()))
}

pub async fn connect_posthog(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<UpsertPosthogRequest>,
) -> Result<(StatusCode, Json<PosthogConnectionResponse>), IntegrationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;

    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("PostHog")
        .chars()
        .take(120)
        .collect::<String>();
    let secret = generate_secret();
    let (ciphertext, nonce) = state
        .credential_cipher
        .encrypt(secret.as_bytes())
        .map_err(|_| IntegrationError::Encryption)?;
    let fingerprint = credentials::fingerprint(&secret);

    let mut transaction = state.db.begin().await?;
    let row = upsert_posthog_integration(
        &mut transaction,
        organization_id,
        &display_name,
        user.id,
        &ciphertext,
        &nonce,
        &fingerprint,
    )
    .await?;
    transaction.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(PosthogConnectionResponse {
            integration: integration_from_row(&row),
            webhook_secret: secret,
        }),
    ))
}

async fn upsert_posthog_integration(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    display_name: &str,
    user_id: Uuid,
    ciphertext: &[u8],
    nonce: &[u8; 12],
    fingerprint: &str,
) -> Result<sqlx::postgres::PgRow, sqlx::Error> {
    let integration_id = Uuid::new_v4();
    let credential_id = Uuid::new_v4();
    sqlx::query(
        "insert into integrations
         (id, organization_id, integration_type, display_name, status, metadata)
         values ($1, $2, 'posthog', $3, 'connected', '{}'::jsonb)
         on conflict (organization_id, integration_type, display_name)
         do update set status = 'connected', updated_at = now()",
    )
    .bind(integration_id)
    .bind(organization_id)
    .bind(display_name)
    .execute(&mut **transaction)
    .await?;

    let integration_row = sqlx::query(
        "select id from integrations
         where organization_id = $1 and integration_type = 'posthog' and display_name = $2",
    )
    .bind(organization_id)
    .bind(display_name)
    .fetch_one(&mut **transaction)
    .await?;
    let integration_id: Uuid = integration_row.get("id");

    sqlx::query(
        "insert into integration_credentials
         (id, integration_id, kind, credential_ciphertext, credential_nonce,
          credential_fingerprint, created_by)
         values ($1, $2, 'webhook_secret', $3, $4, $5, $6)
         on conflict (integration_id, kind)
         do update set credential_ciphertext = excluded.credential_ciphertext,
                       credential_nonce = excluded.credential_nonce,
                       credential_fingerprint = excluded.credential_fingerprint,
                       updated_at = now()",
    )
    .bind(credential_id)
    .bind(integration_id)
    .bind(ciphertext)
    .bind(nonce.as_slice())
    .bind(fingerprint)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;

    sqlx::query(
        "select i.id, i.integration_type, i.display_name, i.status,
                c.credential_fingerprint, i.created_at, i.updated_at
         from integrations i
         left join integration_credentials c
           on c.integration_id = i.id and c.kind = 'webhook_secret'
         where i.id = $1",
    )
    .bind(integration_id)
    .fetch_one(&mut **transaction)
    .await
}

fn integration_from_row(row: &sqlx::postgres::PgRow) -> IntegrationResponse {
    IntegrationResponse {
        id: row.get("id"),
        integration_type: row.get("integration_type"),
        display_name: row.get("display_name"),
        status: row.get("status"),
        credential_fingerprint: row.get("credential_fingerprint"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn generate_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
    require_admin: bool,
) -> Result<(), IntegrationError> {
    let role = sqlx::query(
        "select role from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(IntegrationError::NotFound)?;
    if require_admin && !matches!(role.as_str(), "owner" | "admin") {
        return Err(IntegrationError::Forbidden);
    }
    Ok(())
}

#[derive(Debug)]
pub enum IntegrationError {
    Auth(AuthError),
    Database(sqlx::Error),
    Encryption,
    Forbidden,
    NotFound,
}

impl IntoResponse for IntegrationError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::Database(error) => {
                tracing::error!(error = %error, "integration request failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::Encryption => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Self::Forbidden => StatusCode::FORBIDDEN.into_response(),
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
        }
    }
}

impl From<AuthError> for IntegrationError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for IntegrationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

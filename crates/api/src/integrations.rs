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
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use std::net::IpAddr;
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
    metadata: Value,
    credential_fingerprint: Option<String>,
    api_token_fingerprint: Option<String>,
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
    posthog_host: Option<String>,
    posthog_project_id: Option<String>,
    api_token: Option<String>,
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
                i.metadata,
                c.credential_fingerprint,
                api.credential_fingerprint as api_token_fingerprint,
                i.created_at, i.updated_at
         from integrations i
         left join integration_credentials c
           on c.integration_id = i.id and c.kind = 'webhook_secret'
         left join integration_credentials api
           on api.integration_id = i.id and api.kind = 'api_token'
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
    let api_token = request
        .api_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let api_token_secret = if let Some(token) = api_token {
        let (ciphertext, nonce) = state
            .credential_cipher
            .encrypt(token.as_bytes())
            .map_err(|_| IntegrationError::Encryption)?;
        Some((ciphertext, nonce, credentials::fingerprint(token)))
    } else {
        None
    };
    let metadata = posthog_metadata(
        request.posthog_host.as_deref(),
        request.posthog_project_id.as_deref(),
    )?;

    let mut transaction = state.db.begin().await?;
    let upsert = PosthogIntegrationUpsert {
        organization_id,
        display_name: &display_name,
        user_id: user.id,
        metadata: &metadata,
        webhook_ciphertext: &ciphertext,
        webhook_nonce: &nonce,
        webhook_fingerprint: &fingerprint,
        api_token: api_token_secret.as_ref(),
    };
    let row = upsert_posthog_integration(&mut transaction, &upsert).await?;
    transaction.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(PosthogConnectionResponse {
            integration: integration_from_row(&row),
            webhook_secret: secret,
        }),
    ))
}

struct PosthogIntegrationUpsert<'a> {
    organization_id: Uuid,
    display_name: &'a str,
    user_id: Uuid,
    metadata: &'a Value,
    webhook_ciphertext: &'a [u8],
    webhook_nonce: &'a [u8; 12],
    webhook_fingerprint: &'a str,
    api_token: Option<&'a (Vec<u8>, [u8; 12], String)>,
}

async fn upsert_posthog_integration(
    transaction: &mut Transaction<'_, Postgres>,
    upsert: &PosthogIntegrationUpsert<'_>,
) -> Result<sqlx::postgres::PgRow, sqlx::Error> {
    let integration_id = Uuid::new_v4();
    let credential_id = Uuid::new_v4();
    sqlx::query(
        "insert into integrations
         (id, organization_id, integration_type, display_name, status, metadata)
         values ($1, $2, 'posthog', $3, 'connected', $4)
         on conflict (organization_id, integration_type, display_name)
         do update set status = 'connected',
                       metadata = integrations.metadata || excluded.metadata,
                       updated_at = now()",
    )
    .bind(integration_id)
    .bind(upsert.organization_id)
    .bind(upsert.display_name)
    .bind(upsert.metadata)
    .execute(&mut **transaction)
    .await?;

    let integration_row = sqlx::query(
        "select id from integrations
         where organization_id = $1 and integration_type = 'posthog' and display_name = $2",
    )
    .bind(upsert.organization_id)
    .bind(upsert.display_name)
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
    .bind(upsert.webhook_ciphertext)
    .bind(upsert.webhook_nonce.as_slice())
    .bind(upsert.webhook_fingerprint)
    .bind(upsert.user_id)
    .execute(&mut **transaction)
    .await?;

    if let Some((api_ciphertext, api_nonce, api_fingerprint)) = upsert.api_token {
        sqlx::query(
            "insert into integration_credentials
             (id, integration_id, kind, credential_ciphertext, credential_nonce,
              credential_fingerprint, created_by)
             values ($1, $2, 'api_token', $3, $4, $5, $6)
             on conflict (integration_id, kind)
             do update set credential_ciphertext = excluded.credential_ciphertext,
                           credential_nonce = excluded.credential_nonce,
                           credential_fingerprint = excluded.credential_fingerprint,
                           updated_at = now()",
        )
        .bind(Uuid::new_v4())
        .bind(integration_id)
        .bind(api_ciphertext)
        .bind(api_nonce.as_slice())
        .bind(api_fingerprint)
        .bind(upsert.user_id)
        .execute(&mut **transaction)
        .await?;
    }

    sqlx::query(
        "select i.id, i.integration_type, i.display_name, i.status,
                i.metadata,
                c.credential_fingerprint,
                api.credential_fingerprint as api_token_fingerprint,
                i.created_at, i.updated_at
         from integrations i
         left join integration_credentials c
           on c.integration_id = i.id and c.kind = 'webhook_secret'
         left join integration_credentials api
           on api.integration_id = i.id and api.kind = 'api_token'
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
        metadata: row.get("metadata"),
        credential_fingerprint: row.get("credential_fingerprint"),
        api_token_fingerprint: row.get("api_token_fingerprint"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn posthog_metadata(
    posthog_host: Option<&str>,
    posthog_project_id: Option<&str>,
) -> Result<Value, IntegrationError> {
    let mut metadata = serde_json::Map::new();
    if let Some(host) = posthog_host
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let url = validate_posthog_origin(host)?;
        metadata.insert(
            "posthog_host".to_string(),
            Value::String(url.origin().ascii_serialization()),
        );
    }
    if let Some(project_id) = posthog_project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "posthog_project_id".to_string(),
            Value::String(project_id.chars().take(120).collect()),
        );
    }
    Ok(Value::Object(metadata))
}

fn validate_posthog_origin(value: &str) -> Result<Url, IntegrationError> {
    let url = Url::parse(value).map_err(|_| IntegrationError::InvalidPosthogConfig)?;
    let host = url
        .host_str()
        .ok_or(IntegrationError::InvalidPosthogConfig)?;
    let allowed_origins = allowed_posthog_origins();
    let origin = url.origin().ascii_serialization();

    if allowed_origins.iter().any(|allowed| allowed == &origin) {
        return Ok(url);
    }
    if cfg!(debug_assertions)
        && matches!(url.scheme(), "http" | "https")
        && matches!(host, "localhost" | "127.0.0.1" | "::1")
    {
        return Ok(url);
    }
    if url.scheme() != "https" {
        return Err(IntegrationError::InvalidPosthogConfig);
    }
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err(IntegrationError::InvalidPosthogConfig);
    }
    if let Ok(ip) = host.parse::<IpAddr>()
        && !is_public_ip(ip)
    {
        return Err(IntegrationError::InvalidPosthogConfig);
    }
    Err(IntegrationError::InvalidPosthogConfig)
}

fn allowed_posthog_origins() -> Vec<String> {
    let configured = std::env::var("BELLA_ALLOWED_POSTHOG_ORIGINS").unwrap_or_else(|_| {
        "https://us.posthog.com,https://eu.posthog.com,https://app.posthog.com".to_string()
    });
    configured
        .split(',')
        .filter_map(|value| Url::parse(value.trim()).ok())
        .filter(|url| url.scheme() == "https" && url.host_str().is_some())
        .map(|url| url.origin().ascii_serialization())
        .collect()
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified())
        }
        IpAddr::V6(ip) => {
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
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
    InvalidPosthogConfig,
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
            Self::InvalidPosthogConfig => (
                StatusCode::BAD_REQUEST,
                "PostHog host must be an allowed HTTPS origin and project ID must be present",
            )
                .into_response(),
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

#[cfg(test)]
mod tests {
    use super::{is_public_ip, validate_posthog_origin};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn allows_known_posthog_cloud_origins() {
        assert!(validate_posthog_origin("https://us.posthog.com").is_ok());
        assert!(validate_posthog_origin("https://eu.posthog.com/project/123").is_ok());
    }

    #[test]
    fn rejects_unconfigured_and_insecure_posthog_origins() {
        assert!(validate_posthog_origin("http://us.posthog.com").is_err());
        assert!(validate_posthog_origin("https://example.com").is_err());
    }

    #[test]
    fn rejects_private_and_metadata_ip_hosts() {
        assert!(validate_posthog_origin("https://10.0.0.1").is_err());
        assert!(validate_posthog_origin("https://192.168.1.5").is_err());
        assert!(validate_posthog_origin("https://169.254.169.254").is_err());
    }

    #[test]
    fn classifies_non_public_ips() {
        assert!(!is_public_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(!is_public_ip(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(!is_public_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }
}

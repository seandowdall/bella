use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
    credentials, provider_validation,
};

#[derive(Clone, Copy, Serialize)]
pub struct ProviderDefinition {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    ingestion: &'static str,
    credential_label: &'static str,
    credential_placeholder: &'static str,
    docs_url: &'static str,
}

const PROVIDERS: &[ProviderDefinition] = &[
    provider(
        "openai",
        "OpenAI",
        "Direct API",
        "usage_api",
        "Admin API key",
        "sk-admin-...",
        "https://platform.openai.com/settings/organization/admin-keys",
    ),
    provider(
        "anthropic",
        "Anthropic",
        "Direct API",
        "usage_api",
        "Admin API key",
        "sk-ant-admin-...",
        "https://docs.anthropic.com/en/api/admin-api",
    ),
    provider(
        "google_ai",
        "Google Gemini",
        "Direct API",
        "cloud_billing",
        "API key or service account JSON",
        "AIza... or JSON",
        "https://ai.google.dev/gemini-api/docs/api-key",
    ),
    provider(
        "azure_openai",
        "Azure OpenAI",
        "Cloud",
        "cloud_billing",
        "API key",
        "Azure resource key",
        "https://learn.microsoft.com/azure/ai-foundry/openai/how-to/managed-identity",
    ),
    provider(
        "aws_bedrock",
        "AWS Bedrock",
        "Cloud",
        "cloud_billing",
        "Access credentials JSON",
        "{\"access_key_id\":\"...\"}",
        "https://docs.aws.amazon.com/bedrock/latest/userguide/security-iam.html",
    ),
    provider(
        "mistral",
        "Mistral AI",
        "Direct API",
        "connection_only",
        "API key",
        "Mistral API key",
        "https://console.mistral.ai/api-keys",
    ),
    provider(
        "deepseek",
        "DeepSeek",
        "Direct API",
        "connection_only",
        "API key",
        "sk-...",
        "https://api-docs.deepseek.com/",
    ),
    provider(
        "cohere",
        "Cohere",
        "Direct API",
        "connection_only",
        "API key",
        "Cohere API key",
        "https://docs.cohere.com/docs/rate-limits",
    ),
    provider(
        "groq",
        "Groq",
        "Inference",
        "connection_only",
        "API key",
        "gsk_...",
        "https://console.groq.com/keys",
    ),
    provider(
        "together",
        "Together AI",
        "Inference",
        "connection_only",
        "API key",
        "Together API key",
        "https://docs.together.ai/docs/api-key",
    ),
    provider(
        "fireworks",
        "Fireworks AI",
        "Inference",
        "connection_only",
        "API key",
        "Fireworks API key",
        "https://docs.fireworks.ai/api-reference/introduction",
    ),
    provider(
        "xai",
        "xAI",
        "Direct API",
        "connection_only",
        "API key",
        "xai-...",
        "https://docs.x.ai/docs/tutorial",
    ),
    provider(
        "perplexity",
        "Perplexity",
        "Direct API",
        "connection_only",
        "API key",
        "pplx-...",
        "https://docs.perplexity.ai/guides/getting-started",
    ),
    provider(
        "openrouter",
        "OpenRouter",
        "Gateway",
        "connection_only",
        "API key",
        "sk-or-...",
        "https://openrouter.ai/settings/keys",
    ),
    provider(
        "cerebras",
        "Cerebras",
        "Inference",
        "connection_only",
        "API key",
        "csk-...",
        "https://inference-docs.cerebras.ai/quickstart",
    ),
    provider(
        "replicate",
        "Replicate",
        "Inference",
        "connection_only",
        "API token",
        "r8_...",
        "https://replicate.com/account/api-tokens",
    ),
    provider(
        "hugging_face",
        "Hugging Face",
        "Inference",
        "connection_only",
        "Access token",
        "hf_...",
        "https://huggingface.co/docs/hub/security-tokens",
    ),
    provider(
        "cloudflare_workers_ai",
        "Cloudflare Workers AI",
        "Cloud",
        "cloud_billing",
        "API token",
        "Cloudflare API token",
        "https://developers.cloudflare.com/workers-ai/get-started/rest-api/",
    ),
    provider(
        "vertex_ai",
        "Google Vertex AI",
        "Cloud",
        "cloud_billing",
        "Service account JSON",
        "{\"type\":\"service_account\"}",
        "https://cloud.google.com/vertex-ai/docs/authentication",
    ),
];

const fn provider(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    ingestion: &'static str,
    credential_label: &'static str,
    credential_placeholder: &'static str,
    docs_url: &'static str,
) -> ProviderDefinition {
    ProviderDefinition {
        id,
        name,
        category,
        ingestion,
        credential_label,
        credential_placeholder,
        docs_url,
    }
}

#[derive(Debug, Serialize)]
pub struct ProviderAccountResponse {
    id: Uuid,
    organization_id: Uuid,
    workspace_id: Uuid,
    workspace_name: String,
    provider: String,
    display_name: String,
    credential_fingerprint: String,
    status: String,
    validated_at: Option<chrono::DateTime<chrono::Utc>>,
    validation_error: Option<String>,
    last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    next_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    last_sync_error: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct UpsertProviderAccountRequest {
    workspace_id: Uuid,
    provider: String,
    display_name: String,
    credentials: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub struct UpdateProviderAccountRequest {
    display_name: String,
}

pub async fn catalog() -> Json<&'static [ProviderDefinition]> {
    Json(PROVIDERS)
}

pub async fn list(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<Vec<ProviderAccountResponse>>, ProviderAccountError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, false).await?;
    let rows = sqlx::query(
        "select p.id, p.organization_id, p.workspace_id, w.name as workspace_name,
                p.provider, p.display_name, p.credential_fingerprint, p.status,
                p.validated_at, p.validation_error, p.last_synced_at,
                p.next_sync_at, p.last_sync_error, p.created_at
         from provider_accounts p
         join workspaces w on w.id = p.workspace_id
         where p.organization_id = $1
         order by p.created_at, p.id",
    )
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.iter()
            .map(account_from_row)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub async fn upsert(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<UpsertProviderAccountRequest>,
) -> Result<Json<ProviderAccountResponse>, ProviderAccountError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;

    let provider = request.provider.trim().to_lowercase();
    if !PROVIDERS.iter().any(|definition| definition.id == provider) {
        return Err(ProviderAccountError::BadRequest("unsupported provider"));
    }
    let display_name = normalize_display_name(&request.display_name)?;
    let secret = request
        .credentials
        .get("secret")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or(ProviderAccountError::BadRequest(
            "credentials.secret is required",
        ))?;
    if secret.len() > 32_768 {
        return Err(ProviderAccountError::BadRequest(
            "credential payload is too large",
        ));
    }
    let workspace_exists =
        sqlx::query("select 1 from workspaces where id = $1 and organization_id = $2")
            .bind(request.workspace_id)
            .bind(organization_id)
            .fetch_optional(&state.db)
            .await?
            .is_some();
    if !workspace_exists {
        return Err(ProviderAccountError::BadRequest(
            "workspace does not belong to organization",
        ));
    }

    let plaintext =
        serde_json::to_vec(&request.credentials).map_err(|_| ProviderAccountError::Encryption)?;
    let (ciphertext, nonce) = state
        .credential_cipher
        .encrypt(&plaintext)
        .map_err(|_| ProviderAccountError::Encryption)?;
    let fingerprint = credentials::fingerprint(secret);
    let validation = provider_validation::validate(
        &state.provider_client,
        &provider,
        secret,
        &state.config.openai_base_url,
    )
    .await;
    let account_id = Uuid::new_v4();

    let row = sqlx::query(
        "insert into provider_accounts
         (id, organization_id, workspace_id, provider, display_name,
          credential_ciphertext, credential_nonce, credential_fingerprint, status,
          validated_at, validation_error, created_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         on conflict (workspace_id, provider, display_name) do update
         set credential_ciphertext = excluded.credential_ciphertext,
             credential_nonce = excluded.credential_nonce,
             credential_fingerprint = excluded.credential_fingerprint,
             status = excluded.status,
             validated_at = excluded.validated_at,
             validation_error = excluded.validation_error,
             updated_at = now()
         returning id, organization_id, workspace_id,
                   (select name from workspaces where id = workspace_id) as workspace_name,
                   provider, display_name, credential_fingerprint, status,
                    validated_at, validation_error, last_synced_at,
                    next_sync_at, last_sync_error, created_at",
    )
    .bind(account_id)
    .bind(organization_id)
    .bind(request.workspace_id)
    .bind(provider)
    .bind(display_name)
    .bind(ciphertext)
    .bind(nonce.as_slice())
    .bind(fingerprint)
    .bind(validation.status)
    .bind(validation.validated_at)
    .bind(validation.error)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(account_from_row(&row)?))
}

pub async fn delete(
    State(state): State<AppState>,
    Path((organization_id, account_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<StatusCode, ProviderAccountError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let result =
        sqlx::query("delete from provider_accounts where id = $1 and organization_id = $2")
            .bind(account_id)
            .bind(organization_id)
            .execute(&state.db)
            .await?;
    if result.rows_affected() == 0 {
        return Err(ProviderAccountError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update(
    State(state): State<AppState>,
    Path((organization_id, account_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<UpdateProviderAccountRequest>,
) -> Result<Json<ProviderAccountResponse>, ProviderAccountError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let display_name = normalize_display_name(&request.display_name)?;

    let duplicate = sqlx::query(
        "select 1
         from provider_accounts target
         join provider_accounts existing
           on existing.workspace_id = target.workspace_id
          and existing.provider = target.provider
          and existing.display_name = $1
          and existing.id <> target.id
         where target.id = $2 and target.organization_id = $3",
    )
    .bind(&display_name)
    .bind(account_id)
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    .is_some();
    if duplicate {
        return Err(ProviderAccountError::Conflict(
            "a provider account with that name already exists",
        ));
    }

    let row = sqlx::query(
        "update provider_accounts p
         set display_name = $1, updated_at = now()
         from workspaces w
         where p.id = $2
           and p.organization_id = $3
           and w.id = p.workspace_id
         returning p.id, p.organization_id, p.workspace_id,
                   w.name as workspace_name, p.provider, p.display_name,
                   p.credential_fingerprint, p.status, p.validated_at,
                    p.validation_error, p.last_synced_at, p.next_sync_at,
                    p.last_sync_error, p.created_at",
    )
    .bind(display_name)
    .bind(account_id)
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ProviderAccountError::NotFound)?;

    Ok(Json(account_from_row(&row)?))
}

pub async fn sync_now(
    State(state): State<AppState>,
    Path((organization_id, account_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<bella_ingestion::SyncOutcome>, ProviderAccountError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;

    let row = sqlx::query(
        "select provider, status
         from provider_accounts
         where id = $1 and organization_id = $2",
    )
    .bind(account_id)
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ProviderAccountError::NotFound)?;
    let provider: String = row.get("provider");
    let status: String = row.get("status");
    if provider != "openai" {
        return Err(ProviderAccountError::BadRequest(
            "sync is only implemented for openai provider accounts",
        ));
    }
    if status != "verified" {
        return Err(ProviderAccountError::BadRequest(
            "provider account must be verified before syncing",
        ));
    }

    let ingestor = bella_ingestion::openai::OpenAiIngestor::new(
        state.db.clone(),
        state.provider_client.clone(),
        state.credential_cipher.clone(),
        state.config.openai_base_url.clone(),
    );
    let outcome = ingestor
        .sync_account(account_id)
        .await
        .map_err(|error| ProviderAccountError::Sync(error.to_string()))?;

    Ok(Json(outcome))
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
    require_admin: bool,
) -> Result<(), ProviderAccountError> {
    let role = sqlx::query(
        "select role from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(ProviderAccountError::NotFound)?;
    if require_admin && !matches!(role.as_str(), "owner" | "admin") {
        return Err(ProviderAccountError::Forbidden);
    }
    Ok(())
}

fn normalize_display_name(value: &str) -> Result<String, ProviderAccountError> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() || value.chars().count() > 80 {
        return Err(ProviderAccountError::BadRequest(
            "display name must contain between 1 and 80 characters",
        ));
    }
    Ok(value)
}

fn account_from_row(row: &sqlx::postgres::PgRow) -> Result<ProviderAccountResponse, sqlx::Error> {
    Ok(ProviderAccountResponse {
        id: row.try_get("id")?,
        organization_id: row.try_get("organization_id")?,
        workspace_id: row.try_get("workspace_id")?,
        workspace_name: row.try_get("workspace_name")?,
        provider: row.try_get("provider")?,
        display_name: row.try_get("display_name")?,
        credential_fingerprint: row.try_get("credential_fingerprint")?,
        status: row.try_get("status")?,
        validated_at: row.try_get("validated_at")?,
        validation_error: row.try_get("validation_error")?,
        last_synced_at: row.try_get("last_synced_at")?,
        next_sync_at: row.try_get("next_sync_at")?,
        last_sync_error: row.try_get("last_sync_error")?,
        created_at: row.try_get("created_at")?,
    })
}

#[derive(Debug)]
pub enum ProviderAccountError {
    Auth(AuthError),
    BadRequest(&'static str),
    Conflict(&'static str),
    Forbidden,
    NotFound,
    Encryption,
    Sync(String),
    Database(sqlx::Error),
}

impl From<AuthError> for ProviderAccountError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for ProviderAccountError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for ProviderAccountError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::Conflict(message) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "organization admin access required" })),
            )
                .into_response(),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "organization or provider account not found" })),
            )
                .into_response(),
            Self::Encryption => {
                tracing::error!("provider credential encryption failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "credential storage failed" })),
                )
                    .into_response()
            }
            Self::Sync(error) => {
                tracing::warn!(%error, "provider account sync failed");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "error": error })),
                )
                    .into_response()
            }
            Self::Database(error) => {
                tracing::error!(%error, "provider account database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "provider account request failed" })),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_display_name;

    #[test]
    fn normalizes_provider_account_names() {
        assert_eq!(
            normalize_display_name("  Production   admin  ").unwrap(),
            "Production admin"
        );
        assert!(normalize_display_name("").is_err());
    }
}

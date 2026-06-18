use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
    credentials,
};

#[derive(Debug, Serialize)]
pub struct AgentLlmSettingsListResponse {
    items: Vec<AgentLlmSettingsResponse>,
    default_id: Option<Uuid>,
    mode: &'static str,
}

#[derive(Debug, Serialize)]
pub struct AgentLlmSettingsResponse {
    id: Uuid,
    display_name: String,
    provider: String,
    model: String,
    credential_fingerprint: String,
    is_default: bool,
}

#[derive(Debug)]
pub struct AgentLlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveAgentLlmSettingsRequest {
    display_name: String,
    provider: String,
    model: String,
    api_key: Option<String>,
    is_default: Option<bool>,
}

pub async fn list_settings(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<AgentLlmSettingsListResponse>, AgentSettingsError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, false).await?;

    Ok(Json(load_settings(&state, organization_id).await?))
}

pub async fn create_settings(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<SaveAgentLlmSettingsRequest>,
) -> Result<(StatusCode, Json<AgentLlmSettingsResponse>), AgentSettingsError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AgentSettingsError::BadRequest("api_key is required"))?;
    let encrypted = encrypt_api_key(&state, api_key)?;
    let setting_id = Uuid::new_v4();
    let values = normalized_values(&request)?;

    let mut transaction = state.db.begin().await?;
    let should_default = request.is_default.unwrap_or(false)
        || !organization_has_settings(&mut transaction, organization_id).await?;
    if should_default {
        clear_default(&mut transaction, organization_id).await?;
    }
    let row = sqlx::query(
        "insert into organization_agent_llm_settings
           (id, organization_id, display_name, provider, model,
            api_key_ciphertext, api_key_nonce, api_key_fingerprint,
            is_default, created_by, updated_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
         returning id, display_name, provider, model,
                   api_key_fingerprint, is_default",
    )
    .bind(setting_id)
    .bind(organization_id)
    .bind(values.display_name)
    .bind(values.provider)
    .bind(values.model)
    .bind(encrypted.ciphertext)
    .bind(encrypted.nonce)
    .bind(encrypted.fingerprint)
    .bind(should_default)
    .bind(user.id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok((StatusCode::CREATED, Json(settings_from_row(&row)?)))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Path((organization_id, setting_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<SaveAgentLlmSettingsRequest>,
) -> Result<Json<AgentLlmSettingsResponse>, AgentSettingsError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let values = normalized_values(&request)?;
    let encrypted = if let Some(api_key) = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(encrypt_api_key(&state, api_key)?)
    } else {
        None
    };

    let mut transaction = state.db.begin().await?;
    if request.is_default.unwrap_or(false) {
        clear_default(&mut transaction, organization_id).await?;
    }
    let row = sqlx::query(
        "update organization_agent_llm_settings
         set display_name = $1,
              provider = $2,
              model = $3,
              api_key_ciphertext = coalesce($4, api_key_ciphertext),
              api_key_nonce = coalesce($5, api_key_nonce),
              api_key_fingerprint = coalesce($6, api_key_fingerprint),
              is_default = case when $7 then true else is_default end,
              updated_by = $8,
              updated_at = now()
         where organization_id = $9 and id = $10
         returning id, display_name, provider, model,
                    api_key_fingerprint, is_default",
    )
    .bind(values.display_name)
    .bind(values.provider)
    .bind(values.model)
    .bind(encrypted.as_ref().map(|value| value.ciphertext.as_slice()))
    .bind(encrypted.as_ref().map(|value| value.nonce.as_slice()))
    .bind(encrypted.as_ref().map(|value| value.fingerprint.as_str()))
    .bind(request.is_default.unwrap_or(false))
    .bind(user.id)
    .bind(organization_id)
    .bind(setting_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(AgentSettingsError::NotFound)?;
    transaction.commit().await?;

    Ok(Json(settings_from_row(&row)?))
}

pub async fn delete_settings(
    State(state): State<AppState>,
    Path((organization_id, setting_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<StatusCode, AgentSettingsError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let mut transaction = state.db.begin().await?;
    let row = sqlx::query(
        "delete from organization_agent_llm_settings
         where organization_id = $1 and id = $2
         returning is_default",
    )
    .bind(organization_id)
    .bind(setting_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(AgentSettingsError::NotFound)?;
    if row.get::<bool, _>("is_default") {
        sqlx::query(
            "update organization_agent_llm_settings
             set is_default = true
             where id = (
               select id from organization_agent_llm_settings
               where organization_id = $1
               order by updated_at desc, created_at desc
               limit 1
             )",
        )
        .bind(organization_id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_default(
    State(state): State<AppState>,
    Path((organization_id, setting_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<AgentLlmSettingsResponse>, AgentSettingsError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let mut transaction = state.db.begin().await?;
    let exists = sqlx::query(
        "select 1 from organization_agent_llm_settings where organization_id = $1 and id = $2",
    )
    .bind(organization_id)
    .bind(setting_id)
    .fetch_optional(&mut *transaction)
    .await?
    .is_some();
    if !exists {
        return Err(AgentSettingsError::NotFound);
    }
    clear_default(&mut transaction, organization_id).await?;
    let row = sqlx::query(
        "update organization_agent_llm_settings
         set is_default = true, updated_by = $1, updated_at = now()
         where organization_id = $2 and id = $3
         returning id, display_name, provider, model,
                    api_key_fingerprint, is_default",
    )
    .bind(user.id)
    .bind(organization_id)
    .bind(setting_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(Json(settings_from_row(&row)?))
}

pub async fn load_agent_llm_config(
    state: &AppState,
    organization_id: Uuid,
    setting_id: Option<Uuid>,
) -> Result<Option<AgentLlmConfig>, anyhow::Error> {
    let row = if let Some(setting_id) = setting_id {
        sqlx::query(
            "select provider, model, api_key_ciphertext, api_key_nonce
             from organization_agent_llm_settings
             where organization_id = $1 and id = $2",
        )
        .bind(organization_id)
        .bind(setting_id)
        .fetch_optional(&state.db)
        .await?
    } else {
        sqlx::query(
            "select provider, model, api_key_ciphertext, api_key_nonce
             from organization_agent_llm_settings
             where organization_id = $1 and is_default
             limit 1",
        )
        .bind(organization_id)
        .fetch_optional(&state.db)
        .await?
    };

    let Some(row) = row else {
        return Ok(None);
    };
    let ciphertext: Vec<u8> = row.try_get("api_key_ciphertext")?;
    let nonce: Vec<u8> = row.try_get("api_key_nonce")?;
    let plaintext = state.credential_cipher.decrypt(&ciphertext, &nonce)?;
    let api_key = String::from_utf8(plaintext)
        .map_err(|_| anyhow::anyhow!("agent LLM credential is not valid UTF-8"))?;

    Ok(Some(AgentLlmConfig {
        provider: row.try_get("provider")?,
        model: row.try_get("model")?,
        api_key,
    }))
}

async fn load_settings(
    state: &AppState,
    organization_id: Uuid,
) -> Result<AgentLlmSettingsListResponse, AgentSettingsError> {
    let rows = sqlx::query(
        "select id, display_name, provider, model,
                api_key_fingerprint, is_default
         from organization_agent_llm_settings
         where organization_id = $1
         order by is_default desc, display_name, created_at",
    )
    .bind(organization_id)
    .fetch_all(&state.db)
    .await?;
    let items = rows
        .iter()
        .map(settings_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let default_id = items
        .iter()
        .find(|item| item.is_default)
        .map(|item| item.id);

    Ok(AgentLlmSettingsListResponse {
        mode: if default_id.is_some() {
            "llm_assisted"
        } else {
            "deterministic"
        },
        items,
        default_id,
    })
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
    require_admin: bool,
) -> Result<(), AgentSettingsError> {
    let role = sqlx::query(
        "select role from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(AgentSettingsError::NotFound)?;
    if require_admin && !matches!(role.as_str(), "owner" | "admin") {
        return Err(AgentSettingsError::Forbidden);
    }
    Ok(())
}

struct NormalizedSettings {
    display_name: String,
    provider: String,
    model: String,
}

struct EncryptedApiKey {
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    fingerprint: String,
}

fn normalized_values(
    request: &SaveAgentLlmSettingsRequest,
) -> Result<NormalizedSettings, AgentSettingsError> {
    Ok(NormalizedSettings {
        display_name: normalize_display_name(&request.display_name)?,
        provider: normalize_provider(&request.provider)?,
        model: normalize_model(&request.model)?,
    })
}

fn encrypt_api_key(state: &AppState, api_key: &str) -> Result<EncryptedApiKey, AgentSettingsError> {
    if api_key.len() > 32_768 {
        return Err(AgentSettingsError::BadRequest("api key is too large"));
    }
    let (ciphertext, nonce) = state
        .credential_cipher
        .encrypt(api_key.as_bytes())
        .map_err(|_| AgentSettingsError::Encryption)?;
    Ok(EncryptedApiKey {
        ciphertext,
        nonce: nonce.to_vec(),
        fingerprint: credentials::fingerprint(api_key),
    })
}

async fn organization_has_settings(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("select 1 from organization_agent_llm_settings where organization_id = $1")
            .bind(organization_id)
            .fetch_optional(&mut **transaction)
            .await?
            .is_some(),
    )
}

async fn clear_default(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "update organization_agent_llm_settings
         set is_default = false
         where organization_id = $1",
    )
    .bind(organization_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn settings_from_row(row: &sqlx::postgres::PgRow) -> Result<AgentLlmSettingsResponse, sqlx::Error> {
    Ok(AgentLlmSettingsResponse {
        id: row.try_get("id")?,
        display_name: row.try_get("display_name")?,
        provider: row.try_get("provider")?,
        model: row.try_get("model")?,
        credential_fingerprint: row.try_get("api_key_fingerprint")?,
        is_default: row.try_get("is_default")?,
    })
}

fn normalize_display_name(value: &str) -> Result<String, AgentSettingsError> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() || value.chars().count() > 80 {
        return Err(AgentSettingsError::BadRequest(
            "display name must contain between 1 and 80 characters",
        ));
    }
    Ok(value)
}

fn normalize_provider(value: &str) -> Result<String, AgentSettingsError> {
    let provider = value.trim().to_lowercase();
    if matches!(provider.as_str(), "openai" | "anthropic") {
        Ok(provider)
    } else {
        Err(AgentSettingsError::BadRequest("unsupported LLM provider"))
    }
}

fn normalize_model(value: &str) -> Result<String, AgentSettingsError> {
    let model = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if model.is_empty() || model.chars().count() > 120 {
        return Err(AgentSettingsError::BadRequest(
            "model must contain between 1 and 120 characters",
        ));
    }
    Ok(model)
}

#[derive(Debug)]
pub enum AgentSettingsError {
    Auth(AuthError),
    BadRequest(&'static str),
    Forbidden,
    NotFound,
    Encryption,
    Database(sqlx::Error),
}

impl From<AuthError> for AgentSettingsError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for AgentSettingsError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for AgentSettingsError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
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
                Json(serde_json::json!({ "error": "agent model configuration not found" })),
            )
                .into_response(),
            Self::Encryption => {
                tracing::error!("agent LLM credential encryption failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "agent credential storage failed" })),
                )
                    .into_response()
            }
            Self::Database(error) => {
                tracing::error!(%error, "agent settings database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "agent settings request failed. Restart the API on this branch and make sure database migrations have run."
                    })),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_display_name, normalize_model, normalize_provider};

    #[test]
    fn validates_supported_providers() {
        assert_eq!(normalize_provider(" OpenAI ").unwrap(), "openai");
        assert!(normalize_provider("local").is_err());
    }

    #[test]
    fn validates_names_and_models() {
        assert_eq!(
            normalize_display_name("  Claude   prod ").unwrap(),
            "Claude prod"
        );
        assert_eq!(normalize_model(" gpt-4.1   mini ").unwrap(), "gpt-4.1 mini");
        assert!(normalize_display_name("").is_err());
        assert!(normalize_model("").is_err());
    }
}

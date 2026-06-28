use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bella_github::{GithubClient, GithubError, GithubRepository};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, authenticated_user},
};

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    installation_id: Option<i64>,
    setup_action: Option<String>,
    state: String,
}

#[derive(Debug, Serialize)]
pub struct GithubRepositoriesResponse {
    repositories: Vec<GithubRepositoryResponse>,
    refreshed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GithubRepositoryResponse {
    id: Uuid,
    github_repository_id: i64,
    full_name: String,
    private: bool,
    default_branch: String,
    html_url: String,
    selected: bool,
    updated_at: DateTime<Utc>,
}

struct GithubInstallFlow {
    organization_id: Uuid,
    user_id: Uuid,
    return_to: Option<String>,
}

pub async fn start(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    Query(query): Query<StartQuery>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Redirect, GithubIntegrationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    let github = github_client(&state)?;
    cleanup_expired_install_flows(&state).await?;

    let install_state = random_token();
    let return_to = query
        .return_to
        .filter(|value| is_safe_return_to(value, &state.config.web_url))
        .unwrap_or_else(|| {
            format!(
                "{}/integrations/github",
                state.config.web_url.trim_end_matches('/')
            )
        });
    sqlx::query(
        "insert into github_installation_flows
         (state_hash, organization_id, user_id, return_to, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(hash_token(&install_state))
    .bind(organization_id)
    .bind(user.id)
    .bind(return_to)
    .bind(Utc::now() + ChronoDuration::minutes(15))
    .execute(&state.db)
    .await?;

    Ok(Redirect::temporary(
        &github.config().install_url(&install_state),
    ))
}

pub async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Result<Redirect, GithubIntegrationError> {
    let github = github_client(&state)?;
    let flow = consume_install_flow(&state, &query.state).await?;
    require_membership(&state, flow.user_id, flow.organization_id, true).await?;

    let Some(installation_id) = query.installation_id else {
        return Ok(Redirect::to(&callback_url(
            &state,
            flow.return_to.as_deref(),
            "cancelled",
        )));
    };
    let installation = github.installation(installation_id).await?;
    let metadata = json!({
        "installation_id": installation.id,
        "account_id": installation.account.id,
        "account_login": installation.account.login,
        "account_type": installation.account.account_type,
        "account_url": installation.account.html_url,
        "repository_selection": installation.repository_selection,
        "permissions": installation.permissions,
        "setup_action": query.setup_action.unwrap_or_else(|| "install".to_string()),
    });
    let integration_id = upsert_github_integration(
        &state,
        flow.organization_id,
        &format!("GitHub · {}", installation.account.login),
        &metadata,
    )
    .await?;

    match github.repositories(installation.id).await {
        Ok(repositories) => sync_repositories(&state, integration_id, &repositories).await?,
        Err(error) => {
            tracing::warn!(
                ?error,
                "could not sync GitHub repositories during installation"
            );
        }
    }

    Ok(Redirect::to(&callback_url(
        &state,
        flow.return_to.as_deref(),
        "connected",
    )))
}

pub async fn repositories(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<GithubRepositoriesResponse>, GithubIntegrationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, false).await?;
    let github = github_client(&state)?;
    let (integration_id, installation_id) = github_integration(&state, organization_id).await?;
    let repositories = github.repositories(installation_id).await?;
    sync_repositories(&state, integration_id, &repositories).await?;
    let rows = sqlx::query(
        "select id, github_repository_id, full_name, private, default_branch,
                html_url, selected, updated_at
         from github_repositories
         where integration_id = $1
         order by full_name",
    )
    .bind(integration_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(GithubRepositoriesResponse {
        repositories: rows.iter().map(repository_from_row).collect(),
        refreshed_at: Utc::now(),
    }))
}

pub async fn disconnect(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<StatusCode, GithubIntegrationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_membership(&state, user.id, organization_id, true).await?;
    sqlx::query(
        "delete from integrations where organization_id = $1 and integration_type = 'github'",
    )
    .bind(organization_id)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, GithubIntegrationError> {
    let github = github_client(&state)?;
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|value| value.to_str().ok())
        .ok_or(GithubIntegrationError::InvalidSignature)?;
    if !github.verify_webhook_signature(signature, &body) {
        return Err(GithubIntegrationError::InvalidSignature);
    }

    let event = headers
        .get("x-github-event")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");
    let payload = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    update_installation_status_from_webhook(&state, event, &payload).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn upsert_github_integration(
    state: &AppState,
    organization_id: Uuid,
    display_name: &str,
    metadata: &Value,
) -> Result<Uuid, sqlx::Error> {
    let integration_id = if let Some(row) = sqlx::query(
        "select id from integrations
         where organization_id = $1 and integration_type = 'github'
         order by created_at
         limit 1",
    )
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    {
        let integration_id: Uuid = row.get("id");
        sqlx::query(
            "update integrations
             set display_name = $2,
                 status = 'connected',
                 metadata = $3,
                 updated_at = now()
             where id = $1",
        )
        .bind(integration_id)
        .bind(display_name)
        .bind(metadata)
        .execute(&state.db)
        .await?;
        integration_id
    } else {
        let integration_id = Uuid::new_v4();
        sqlx::query(
            "insert into integrations
             (id, organization_id, integration_type, display_name, status, metadata)
             values ($1, $2, 'github', $3, 'connected', $4)",
        )
        .bind(integration_id)
        .bind(organization_id)
        .bind(display_name)
        .bind(metadata)
        .execute(&state.db)
        .await?;
        integration_id
    };
    Ok(integration_id)
}

async fn sync_repositories(
    state: &AppState,
    integration_id: Uuid,
    repositories: &[GithubRepository],
) -> Result<(), sqlx::Error> {
    for repository in repositories {
        sqlx::query(
            "insert into github_repositories
             (id, integration_id, github_repository_id, full_name, private,
              default_branch, html_url, selected, last_seen_at)
             values ($1, $2, $3, $4, $5, $6, $7, true, now())
             on conflict (integration_id, github_repository_id)
             do update set full_name = excluded.full_name,
                           private = excluded.private,
                           default_branch = excluded.default_branch,
                           html_url = excluded.html_url,
                           last_seen_at = now(),
                           updated_at = now()",
        )
        .bind(Uuid::new_v4())
        .bind(integration_id)
        .bind(repository.id)
        .bind(&repository.full_name)
        .bind(repository.private)
        .bind(&repository.default_branch)
        .bind(&repository.html_url)
        .execute(&state.db)
        .await?;
    }
    Ok(())
}

async fn github_integration(
    state: &AppState,
    organization_id: Uuid,
) -> Result<(Uuid, i64), GithubIntegrationError> {
    let row = sqlx::query(
        "select id, metadata
         from integrations
         where organization_id = $1 and integration_type = 'github'
         order by created_at
         limit 1",
    )
    .bind(organization_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(GithubIntegrationError::NotFound)?;
    let metadata: Value = row.get("metadata");
    let installation_id = metadata
        .get("installation_id")
        .and_then(Value::as_i64)
        .ok_or(GithubIntegrationError::NotFound)?;
    Ok((row.get("id"), installation_id))
}

async fn update_installation_status_from_webhook(
    state: &AppState,
    event: &str,
    payload: &Value,
) -> Result<(), sqlx::Error> {
    if event != "installation" {
        return Ok(());
    }
    let Some(installation_id) = payload
        .get("installation")
        .and_then(|installation| installation.get("id"))
        .and_then(Value::as_i64)
    else {
        return Ok(());
    };
    let action = payload.get("action").and_then(Value::as_str).unwrap_or("");
    let status = match action {
        "deleted" | "suspend" => "disabled",
        "unsuspend" | "new_permissions_accepted" => "connected",
        _ => return Ok(()),
    };
    sqlx::query(
        "update integrations
         set status = $1,
             updated_at = now()
         where integration_type = 'github'
           and metadata->>'installation_id' = $2",
    )
    .bind(status)
    .bind(installation_id.to_string())
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn consume_install_flow(
    state: &AppState,
    install_state: &str,
) -> Result<GithubInstallFlow, GithubIntegrationError> {
    let row = sqlx::query(
        "delete from github_installation_flows
         where state_hash = $1 and expires_at > now()
         returning organization_id, user_id, return_to",
    )
    .bind(hash_token(install_state))
    .fetch_optional(&state.db)
    .await?
    .ok_or(GithubIntegrationError::InvalidFlow)?;
    Ok(GithubInstallFlow {
        organization_id: row.get("organization_id"),
        user_id: row.get("user_id"),
        return_to: row.get("return_to"),
    })
}

async fn cleanup_expired_install_flows(state: &AppState) -> Result<(), sqlx::Error> {
    sqlx::query("delete from github_installation_flows where expires_at <= now()")
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
    require_admin: bool,
) -> Result<(), GithubIntegrationError> {
    let role = sqlx::query(
        "select role from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(GithubIntegrationError::NotFound)?;
    if require_admin && !matches!(role.as_str(), "owner" | "admin") {
        return Err(GithubIntegrationError::Forbidden);
    }
    Ok(())
}

fn repository_from_row(row: &sqlx::postgres::PgRow) -> GithubRepositoryResponse {
    GithubRepositoryResponse {
        id: row.get("id"),
        github_repository_id: row.get("github_repository_id"),
        full_name: row.get("full_name"),
        private: row.get("private"),
        default_branch: row.get("default_branch"),
        html_url: row.get("html_url"),
        selected: row.get("selected"),
        updated_at: row.get("updated_at"),
    }
}

fn github_client(state: &AppState) -> Result<&GithubClient, GithubIntegrationError> {
    state
        .github_client
        .as_ref()
        .ok_or(GithubIntegrationError::Configuration)
}

fn callback_url(state: &AppState, return_to: Option<&str>, status: &str) -> String {
    let base = return_to.unwrap_or(state.config.web_url.as_str());
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}github={status}")
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn is_safe_return_to(value: &str, web_url: &str) -> bool {
    reqwest::Url::parse(value)
        .ok()
        .zip(reqwest::Url::parse(web_url).ok())
        .is_some_and(|(value, web)| value.origin() == web.origin())
}

#[derive(Debug)]
pub enum GithubIntegrationError {
    Auth(AuthError),
    Configuration,
    Database(sqlx::Error),
    Forbidden,
    Github(GithubError),
    InvalidFlow,
    InvalidSignature,
    NotFound,
}

impl IntoResponse for GithubIntegrationError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::Configuration => (
                StatusCode::SERVICE_UNAVAILABLE,
                "GitHub App integration is not configured",
            )
                .into_response(),
            Self::Database(error) => {
                tracing::error!(error = %error, "GitHub integration request failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::Forbidden => StatusCode::FORBIDDEN.into_response(),
            Self::Github(GithubError::Configuration) => {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::Github(GithubError::Rejected) => (
                StatusCode::BAD_GATEWAY,
                "GitHub rejected the app credentials or installation permissions",
            )
                .into_response(),
            Self::Github(GithubError::Unavailable) => {
                (StatusCode::BAD_GATEWAY, "GitHub is unavailable").into_response()
            }
            Self::InvalidFlow => StatusCode::BAD_REQUEST.into_response(),
            Self::InvalidSignature => StatusCode::UNAUTHORIZED.into_response(),
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
        }
    }
}

impl From<AuthError> for GithubIntegrationError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for GithubIntegrationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<GithubError> for GithubIntegrationError {
    fn from(error: GithubError) -> Self {
        Self::Github(error)
    }
}

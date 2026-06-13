use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{AuthError, AuthUser, authenticated_user},
};

const IDEMPOTENCY_KEY: &str = "idempotency-key";

#[derive(Debug, Serialize)]
pub struct OrganizationResponse {
    id: Uuid,
    slug: String,
    name: String,
    role: String,
    default_workspace: WorkspaceResponse,
}

#[derive(Debug, Serialize)]
struct WorkspaceResponse {
    id: Uuid,
    slug: String,
    name: String,
}

#[derive(Deserialize)]
pub struct CreateOrganizationRequest {
    name: String,
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<Vec<OrganizationResponse>>, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    ensure_default_organization(&state, &user).await?;

    let rows = sqlx::query(
        "select o.id, o.slug, o.name, m.role,
                w.id as workspace_id, w.slug as workspace_slug, w.name as workspace_name
         from organization_memberships m
         join organizations o on o.id = m.organization_id
         join workspaces w on w.organization_id = o.id and w.slug = 'default'
         where m.user_id = $1
         order by o.created_at, o.id",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(organization_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)?)
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<CreateOrganizationRequest>,
) -> Result<(StatusCode, Json<OrganizationResponse>), OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    let idempotency_key = headers
        .get(IDEMPOTENCY_KEY)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 200)
        .ok_or(OrganizationError::BadRequest(
            "Idempotency-Key header is required",
        ))?;
    let name = normalize_name(&request.name)?;
    let slug = slugify(&name);
    let request_hash = hash_value(&name);

    let mut transaction = state.db.begin().await?;
    lock_key(
        &mut transaction,
        &format!("organization-create:{}:{idempotency_key}", user.id),
    )
    .await?;

    if let Some(existing) = sqlx::query(
        "select request_hash, organization_id
         from organization_create_requests
         where user_id = $1 and idempotency_key = $2",
    )
    .bind(user.id)
    .bind(idempotency_key)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let existing_hash: String = existing.try_get("request_hash")?;
        if existing_hash != request_hash {
            return Err(OrganizationError::Conflict(
                "Idempotency-Key was already used with a different request",
            ));
        }
        let organization_id: Uuid = existing.try_get("organization_id")?;
        let organization = fetch_organization(&mut transaction, user.id, organization_id).await?;
        transaction.commit().await?;
        return Ok((StatusCode::OK, Json(organization)));
    }

    if sqlx::query("select 1 from organizations where slug = $1")
        .bind(&slug)
        .fetch_optional(&mut *transaction)
        .await?
        .is_some()
    {
        return Err(OrganizationError::Conflict(
            "an organization with that slug already exists",
        ));
    }

    let organization_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    sqlx::query("insert into organizations (id, slug, name) values ($1, $2, $3)")
        .bind(organization_id)
        .bind(&slug)
        .bind(&name)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "insert into organization_memberships (organization_id, user_id, role)
         values ($1, $2, 'owner')",
    )
    .bind(organization_id)
    .bind(user.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "insert into workspaces (id, organization_id, slug, name)
         values ($1, $2, 'default', 'Default')",
    )
    .bind(workspace_id)
    .bind(organization_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "insert into organization_create_requests
         (user_id, idempotency_key, request_hash, organization_id)
         values ($1, $2, $3, $4)",
    )
    .bind(user.id)
    .bind(idempotency_key)
    .bind(request_hash)
    .bind(organization_id)
    .execute(&mut *transaction)
    .await?;

    let organization = fetch_organization(&mut transaction, user.id, organization_id).await?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(organization)))
}

pub async fn ensure_default_organization(
    state: &AppState,
    user: &AuthUser,
) -> Result<OrganizationResponse, sqlx::Error> {
    let mut transaction = state.db.begin().await?;
    lock_key(&mut transaction, &format!("user-onboarding:{}", user.id)).await?;

    if let Some(row) = sqlx::query(
        "select organization_id
         from organization_memberships
         where user_id = $1
         order by created_at, organization_id
         limit 1",
    )
    .bind(user.id)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let organization_id: Uuid = row.try_get("organization_id")?;
        let organization = fetch_organization(&mut transaction, user.id, organization_id).await?;
        transaction.commit().await?;
        return Ok(organization);
    }

    let display_name = format!("{}'s organization", user.github_login);
    let base_slug = slugify(&user.github_login);
    let slug = if sqlx::query("select 1 from organizations where slug = $1")
        .bind(&base_slug)
        .fetch_optional(&mut *transaction)
        .await?
        .is_none()
    {
        base_slug
    } else {
        format!("{base_slug}-{}", &user.id.simple().to_string()[..8])
    };
    let organization_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();

    sqlx::query("insert into organizations (id, slug, name) values ($1, $2, $3)")
        .bind(organization_id)
        .bind(&slug)
        .bind(&display_name)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "insert into organization_memberships (organization_id, user_id, role)
         values ($1, $2, 'owner')",
    )
    .bind(organization_id)
    .bind(user.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "insert into workspaces (id, organization_id, slug, name)
         values ($1, $2, 'default', 'Default')",
    )
    .bind(workspace_id)
    .bind(organization_id)
    .execute(&mut *transaction)
    .await?;

    let organization = fetch_organization(&mut transaction, user.id, organization_id).await?;
    transaction.commit().await?;
    Ok(organization)
}

async fn fetch_organization(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<OrganizationResponse, sqlx::Error> {
    let row = sqlx::query(
        "select o.id, o.slug, o.name, m.role,
                w.id as workspace_id, w.slug as workspace_slug, w.name as workspace_name
         from organizations o
         join organization_memberships m
           on m.organization_id = o.id and m.user_id = $1
         join workspaces w on w.organization_id = o.id and w.slug = 'default'
         where o.id = $2",
    )
    .bind(user_id)
    .bind(organization_id)
    .fetch_one(&mut **transaction)
    .await?;
    organization_from_row(&row)
}

fn organization_from_row(row: &sqlx::postgres::PgRow) -> Result<OrganizationResponse, sqlx::Error> {
    Ok(OrganizationResponse {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        role: row.try_get("role")?,
        default_workspace: WorkspaceResponse {
            id: row.try_get("workspace_id")?,
            slug: row.try_get("workspace_slug")?,
            name: row.try_get("workspace_name")?,
        },
    })
}

async fn lock_key(
    transaction: &mut Transaction<'_, Postgres>,
    key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(key)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn normalize_name(value: &str) -> Result<String, OrganizationError> {
    let name = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 80 {
        return Err(OrganizationError::BadRequest(
            "organization name must contain between 1 and 80 characters",
        ));
    }
    Ok(name)
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        "organization".to_owned()
    } else {
        slug
    }
}

fn hash_value(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[derive(Debug)]
pub enum OrganizationError {
    Auth(AuthError),
    BadRequest(&'static str),
    Conflict(&'static str),
    Database(sqlx::Error),
}

impl From<AuthError> for OrganizationError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<sqlx::Error> for OrganizationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for OrganizationError {
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
            Self::Database(error) => {
                tracing::error!(%error, "organization database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "organization request failed" })),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_name, slugify};

    #[test]
    fn normalizes_names_and_slugs() {
        assert_eq!(normalize_name("  Acme   AI  ").unwrap(), "Acme AI");
        assert_eq!(slugify("Acme AI"), "acme-ai");
        assert_eq!(slugify("Déjà Vu"), "d-j-vu");
    }
}

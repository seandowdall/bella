use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{RngCore, rngs::OsRng};
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

#[derive(Debug, Serialize)]
pub struct OrganizationMembersResponse {
    members: Vec<MemberResponse>,
    invitations: Vec<InvitationResponse>,
}

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    user_id: Uuid,
    github_login: String,
    name: Option<String>,
    avatar_url: Option<String>,
    primary_email: Option<String>,
    role: String,
    created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct InvitationResponse {
    id: Uuid,
    email: String,
    role: String,
    status: String,
    invited_by_github_login: String,
    expires_at: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateOrganizationRequest {
    name: String,
}

#[derive(Deserialize)]
pub struct CreateInvitationRequest {
    email: String,
    role: String,
}

#[derive(Deserialize)]
pub struct UpdateMemberRequest {
    role: String,
}

#[derive(Deserialize)]
pub struct AcceptInvitationRequest {
    token: String,
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<Vec<OrganizationResponse>>, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    if crate::auth::github_email_allowed_by_config(&state, user.primary_email.as_deref()) {
        ensure_default_organization(&state, &user).await?;
    }

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

pub async fn members(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<OrganizationMembersResponse>, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    let viewer_role = require_membership(&state, user.id, organization_id).await?;
    let can_manage = matches!(viewer_role.as_str(), "owner" | "admin");

    let member_rows = sqlx::query(
        "select u.id as user_id, u.github_login, u.name, u.avatar_url,
                case when $2 or u.id = $3 then u.primary_email else null end as primary_email,
                m.role, m.created_at
         from organization_memberships m
         join users u on u.id = m.user_id
         where m.organization_id = $1
         order by case m.role when 'owner' then 0 when 'admin' then 1 else 2 end,
                  m.created_at, u.github_login",
    )
    .bind(organization_id)
    .bind(can_manage)
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    let invitations = if can_manage {
        let invitation_rows = sqlx::query(
            "select i.id, i.email, i.role,
                    case
                      when i.revoked_at is not null then 'revoked'
                      when i.expires_at <= now() then 'expired'
                      else 'pending'
                    end as status,
                    inviter.github_login as invited_by_github_login,
                    i.expires_at, i.created_at
             from organization_invitations i
             join users inviter on inviter.id = i.invited_by_user_id
             where i.organization_id = $1 and i.accepted_at is null
             order by i.created_at desc",
        )
        .bind(organization_id)
        .fetch_all(&state.db)
        .await?;
        invitation_rows
            .iter()
            .map(invitation_from_row)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    Ok(Json(OrganizationMembersResponse {
        members: member_rows
            .iter()
            .map(member_from_row)
            .collect::<Result<Vec<_>, _>>()?,
        invitations,
    }))
}

pub async fn create_invitation(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<InvitationResponse>), OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    let actor_role = require_membership(&state, user.id, organization_id).await?;
    ensure_admin_role(&actor_role)?;
    let email = normalize_email(&request.email)?;
    let invite_role = normalize_invitation_role(&request.role)?;
    let token = random_token();
    let token_hash = hash_value(&token);
    let expires_at = Utc::now() + ChronoDuration::days(7);

    let mut transaction = state.db.begin().await?;
    lock_key(
        &mut transaction,
        &format!("organization-invite:{organization_id}:{email}"),
    )
    .await?;

    if sqlx::query(
        "select 1
         from organization_memberships m
         join users u on u.id = m.user_id
         where m.organization_id = $1 and lower(u.primary_email) = $2",
    )
    .bind(organization_id)
    .bind(&email)
    .fetch_optional(&mut *transaction)
    .await?
    .is_some()
    {
        return Err(OrganizationError::Conflict(
            "that email is already a member of this organization",
        ));
    }

    if sqlx::query(
        "select 1
         from organization_invitations
         where organization_id = $1
           and email = $2
           and accepted_at is null
           and revoked_at is null
           and expires_at > now()",
    )
    .bind(organization_id)
    .bind(&email)
    .fetch_optional(&mut *transaction)
    .await?
    .is_some()
    {
        return Err(OrganizationError::Conflict(
            "that email already has a pending invitation",
        ));
    }

    let invitation_id = Uuid::new_v4();
    sqlx::query(
        "insert into organization_invitations
         (id, organization_id, email, role, token_hash, invited_by_user_id, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(invitation_id)
    .bind(organization_id)
    .bind(&email)
    .bind(invite_role)
    .bind(token_hash)
    .bind(user.id)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await?;

    let organization_name: String =
        sqlx::query_scalar("select name from organizations where id = $1")
            .bind(organization_id)
            .fetch_one(&mut *transaction)
            .await?;
    let invitation = fetch_invitation(&mut transaction, invitation_id).await?;
    transaction.commit().await?;

    if let Err(error) =
        send_invitation_email(&state, &email, &organization_name, invite_role, &token).await
    {
        sqlx::query("update organization_invitations set revoked_at = now() where id = $1")
            .bind(invitation_id)
            .execute(&state.db)
            .await?;
        return Err(error);
    }

    Ok((StatusCode::CREATED, Json(invitation)))
}

pub async fn revoke_invitation(
    State(state): State<AppState>,
    Path((organization_id, invitation_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<StatusCode, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    let actor_role = require_membership(&state, user.id, organization_id).await?;
    ensure_admin_role(&actor_role)?;
    let result = sqlx::query(
        "update organization_invitations
         set revoked_at = now()
         where id = $1
           and organization_id = $2
           and accepted_at is null
           and revoked_at is null
           and expires_at > now()",
    )
    .bind(invitation_id)
    .bind(organization_id)
    .execute(&state.db)
    .await?;
    if result.rows_affected() != 1 {
        return Err(OrganizationError::NotFound("pending invitation not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_invitation(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<AcceptInvitationRequest>,
) -> Result<Json<OrganizationResponse>, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    let email = user
        .primary_email
        .as_deref()
        .map(str::to_ascii_lowercase)
        .ok_or(OrganizationError::Forbidden(
            "your GitHub account does not expose a verified primary email",
        ))?;
    let token = request.token.trim();
    if token.is_empty() {
        return Err(OrganizationError::BadRequest(
            "invitation token is required",
        ));
    }
    let token_hash = hash_value(token);
    let mut transaction = state.db.begin().await?;
    let row = sqlx::query(
        "select id, organization_id, email, role
         from organization_invitations
         where token_hash = $1
           and accepted_at is null
           and revoked_at is null
           and expires_at > now()
         for update",
    )
    .bind(token_hash)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(OrganizationError::NotFound(
        "invitation not found or expired",
    ))?;
    let invitation_id: Uuid = row.try_get("id")?;
    let organization_id: Uuid = row.try_get("organization_id")?;
    let invited_email: String = row.try_get("email")?;
    let role: String = row.try_get("role")?;
    if invited_email != email {
        return Err(OrganizationError::Forbidden(
            "sign in with the GitHub account that owns the invited email",
        ));
    }

    sqlx::query(
        "insert into organization_memberships (organization_id, user_id, role)
         values ($1, $2, $3)
         on conflict (organization_id, user_id) do nothing",
    )
    .bind(organization_id)
    .bind(user.id)
    .bind(&role)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "update organization_invitations
         set accepted_at = now(), accepted_by_user_id = $1
         where id = $2",
    )
    .bind(user.id)
    .bind(invitation_id)
    .execute(&mut *transaction)
    .await?;

    let organization = fetch_organization(&mut transaction, user.id, organization_id).await?;
    transaction.commit().await?;
    Ok(Json(organization))
}

pub async fn update_member(
    State(state): State<AppState>,
    Path((organization_id, member_user_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<UpdateMemberRequest>,
) -> Result<Json<MemberResponse>, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    let actor_role = require_membership(&state, user.id, organization_id).await?;
    if actor_role != "owner" {
        return Err(OrganizationError::Forbidden(
            "organization owner access required",
        ));
    }
    let next_role = normalize_invitation_role(&request.role)?;
    let current_role = require_membership(&state, member_user_id, organization_id).await?;
    if current_role == "owner" {
        return Err(OrganizationError::BadRequest(
            "owner role cannot be changed here",
        ));
    }
    sqlx::query(
        "update organization_memberships
         set role = $1
         where organization_id = $2 and user_id = $3",
    )
    .bind(next_role)
    .bind(organization_id)
    .bind(member_user_id)
    .execute(&state.db)
    .await?;
    let row = sqlx::query(
        "select u.id as user_id, u.github_login, u.name, u.avatar_url, u.primary_email,
                m.role, m.created_at
         from organization_memberships m
         join users u on u.id = m.user_id
         where m.organization_id = $1 and m.user_id = $2",
    )
    .bind(organization_id)
    .bind(member_user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(member_from_row(&row)?))
}

pub async fn remove_member(
    State(state): State<AppState>,
    Path((organization_id, member_user_id)): Path<(Uuid, Uuid)>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<StatusCode, OrganizationError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    if user.id == member_user_id {
        return Err(OrganizationError::BadRequest(
            "members cannot remove themselves",
        ));
    }
    let actor_role = require_membership(&state, user.id, organization_id).await?;
    let target_role = require_membership(&state, member_user_id, organization_id).await?;
    match (actor_role.as_str(), target_role.as_str()) {
        ("owner", "admin" | "member") | ("admin", "member") => {}
        _ => {
            return Err(OrganizationError::Forbidden(
                "insufficient role to remove that member",
            ));
        }
    }
    let result = sqlx::query(
        "delete from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(member_user_id)
    .execute(&state.db)
    .await?;
    if result.rows_affected() != 1 {
        return Err(OrganizationError::NotFound("member not found"));
    }
    Ok(StatusCode::NO_CONTENT)
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

fn member_from_row(row: &sqlx::postgres::PgRow) -> Result<MemberResponse, sqlx::Error> {
    Ok(MemberResponse {
        user_id: row.try_get("user_id")?,
        github_login: row.try_get("github_login")?,
        name: row.try_get("name")?,
        avatar_url: row.try_get("avatar_url")?,
        primary_email: row.try_get("primary_email")?,
        role: row.try_get("role")?,
        created_at: row.try_get("created_at")?,
    })
}

fn invitation_from_row(row: &sqlx::postgres::PgRow) -> Result<InvitationResponse, sqlx::Error> {
    Ok(InvitationResponse {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        role: row.try_get("role")?,
        status: row.try_get("status")?,
        invited_by_github_login: row.try_get("invited_by_github_login")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
    })
}

async fn fetch_invitation(
    transaction: &mut Transaction<'_, Postgres>,
    invitation_id: Uuid,
) -> Result<InvitationResponse, sqlx::Error> {
    let row = sqlx::query(
        "select i.id, i.email, i.role,
                case
                  when i.revoked_at is not null then 'revoked'
                  when i.expires_at <= now() then 'expired'
                  else 'pending'
                end as status,
                inviter.github_login as invited_by_github_login,
                i.expires_at, i.created_at
         from organization_invitations i
         join users inviter on inviter.id = i.invited_by_user_id
         where i.id = $1",
    )
    .bind(invitation_id)
    .fetch_one(&mut **transaction)
    .await?;
    invitation_from_row(&row)
}

async fn require_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<String, OrganizationError> {
    sqlx::query(
        "select role from organization_memberships
         where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(OrganizationError::NotFound("organization not found"))
}

fn ensure_admin_role(role: &str) -> Result<(), OrganizationError> {
    if matches!(role, "owner" | "admin") {
        Ok(())
    } else {
        Err(OrganizationError::Forbidden(
            "organization owner or admin access required",
        ))
    }
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

fn normalize_email(value: &str) -> Result<String, OrganizationError> {
    let email = value.trim().to_ascii_lowercase();
    if email.is_empty()
        || email.len() > 254
        || email.contains(char::is_whitespace)
        || !email.contains('@')
        || email.starts_with('@')
        || email.ends_with('@')
    {
        return Err(OrganizationError::BadRequest(
            "invite email must be a valid email address",
        ));
    }
    Ok(email)
}

fn normalize_invitation_role(value: &str) -> Result<&'static str, OrganizationError> {
    match value {
        "admin" => Ok("admin"),
        "member" => Ok("member"),
        _ => Err(OrganizationError::BadRequest(
            "role must be either admin or member",
        )),
    }
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

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

async fn send_invitation_email(
    state: &AppState,
    email: &str,
    organization_name: &str,
    role: &str,
    token: &str,
) -> Result<(), OrganizationError> {
    let api_key =
        state
            .config
            .resend_api_key
            .as_deref()
            .ok_or(OrganizationError::Configuration(
                "RESEND_API_KEY is required to send invitations",
            ))?;
    let from = state
        .config
        .email_from
        .as_deref()
        .ok_or(OrganizationError::Configuration(
            "BELLA_EMAIL_FROM is required to send invitations",
        ))?;
    let invite_url = format!(
        "{}/invite#token={}",
        state.config.web_url.trim_end_matches('/'),
        token
    );
    let text = format!(
        "You have been invited to join {organization_name} on Bella as {role}.\n\nAccept the invitation:\n{invite_url}\n\nThis invitation expires in 7 days."
    );
    state
        .provider_client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "from": from,
            "to": [email],
            "subject": format!("Join {organization_name} on Bella"),
            "text": text,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[derive(Debug)]
pub enum OrganizationError {
    Auth(AuthError),
    BadRequest(&'static str),
    Configuration(&'static str),
    Conflict(&'static str),
    Forbidden(&'static str),
    NotFound(&'static str),
    Email(reqwest::Error),
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

impl From<reqwest::Error> for OrganizationError {
    fn from(error: reqwest::Error) -> Self {
        Self::Email(error)
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
            Self::Configuration(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::Conflict(message) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::Forbidden(message) => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::NotFound(message) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
            Self::Email(error) => {
                tracing::error!(%error, "organization invitation email error");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "error": "could not send invitation email" })),
                )
                    .into_response()
            }
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
    use super::{normalize_email, normalize_name, slugify};

    #[test]
    fn normalizes_names_and_slugs() {
        assert_eq!(normalize_name("  Acme   AI  ").unwrap(), "Acme AI");
        assert_eq!(slugify("Acme AI"), "acme-ai");
        assert_eq!(slugify("Déjà Vu"), "d-j-vu");
    }

    #[test]
    fn normalizes_invite_emails() {
        assert_eq!(
            normalize_email("  PERSON@Example.COM ").unwrap(),
            "person@example.com"
        );
        assert!(normalize_email("not-an-email").is_err());
    }
}

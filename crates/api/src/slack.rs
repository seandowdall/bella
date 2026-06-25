use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bella_slack::{PostedMessage, SlackClientError};
use chrono::{Duration as ChronoDuration, Utc};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState, SlackCloudConfig,
    auth::{AuthError, authenticated_user},
    credentials,
};

const SLACK_OAUTH_SCOPE: &str = "chat:write,channels:read,groups:read";
const SLACK_OAUTH_STATE_TTL_MINUTES: i64 = 10;
const SLACK_SIGNATURE_VERSION: &str = "v0";
const SLACK_SIGNATURE_MAX_AGE_SECONDS: i64 = 60 * 5;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct InstallUrlRequest {
    return_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallUrlResponse {
    install_url: String,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

struct SlackOAuthState {
    organization_id: Uuid,
    user_id: Uuid,
    return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackOAuthAccessResponse {
    ok: bool,
    error: Option<String>,
    access_token: Option<String>,
    scope: Option<String>,
    app_id: Option<String>,
    bot_user_id: Option<String>,
    team: Option<SlackOAuthTeam>,
    enterprise: Option<SlackOAuthEnterprise>,
}

#[derive(Debug, Deserialize)]
struct SlackOAuthTeam {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct SlackOAuthEnterprise {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SlackEventEnvelope {
    #[serde(rename = "type")]
    envelope_type: String,
    challenge: Option<String>,
    event_id: Option<String>,
    team_id: Option<String>,
    api_app_id: Option<String>,
    event: Option<SlackEventPayload>,
}

#[derive(Debug, Deserialize)]
struct SlackEventPayload {
    #[serde(rename = "type")]
    event_type: String,
}

#[derive(Debug, Serialize)]
struct SlackEventAck {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct SlackUrlVerificationResponse {
    challenge: String,
}

pub async fn install_url(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
    request: Option<Json<InstallUrlRequest>>,
) -> Result<Json<InstallUrlResponse>, SlackError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_admin_membership(&state, user.id, organization_id).await?;
    let slack_cloud = state
        .config
        .slack_cloud
        .as_ref()
        .ok_or(SlackError::CloudNotConfigured)?;
    cleanup_expired_slack_oauth_states(&state).await?;

    let oauth_state = random_token();
    let expires_in = SLACK_OAUTH_STATE_TTL_MINUTES * 60;
    let return_to = request
        .and_then(|Json(request)| request.return_to)
        .filter(|value| is_safe_return_to(value, &state.config.web_url));
    sqlx::query(
        "insert into slack_oauth_states
         (state_hash, organization_id, user_id, return_to, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(hash_token(&oauth_state))
    .bind(organization_id)
    .bind(user.id)
    .bind(return_to)
    .bind(Utc::now() + ChronoDuration::minutes(SLACK_OAUTH_STATE_TTL_MINUTES))
    .execute(&state.db)
    .await?;

    Ok(Json(InstallUrlResponse {
        install_url: build_slack_install_url(slack_cloud, &oauth_state)?,
        expires_in,
    }))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Result<Redirect, SlackError> {
    let redirect = match complete_oauth_callback(&state, query).await {
        Ok(return_to) => redirect_with_slack_status(&return_to, "installed"),
        Err(error) => {
            tracing::warn!(error = ?error, "Slack OAuth callback failed");
            redirect_with_slack_status(&state.config.web_url, "error")
        }
    };
    Ok(Redirect::to(&redirect))
}

pub async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, SlackError> {
    let slack_cloud = state
        .config
        .slack_cloud
        .as_ref()
        .ok_or(SlackError::CloudNotConfigured)?;
    verify_slack_signature(&headers, &body, &slack_cloud.signing_secret)?;

    let envelope = serde_json::from_slice::<SlackEventEnvelope>(&body)
        .map_err(|_| SlackError::InvalidEventPayload)?;
    match envelope.envelope_type.as_str() {
        "url_verification" => {
            let challenge = envelope.challenge.ok_or(SlackError::InvalidEventPayload)?;
            Ok(Json(SlackUrlVerificationResponse { challenge }).into_response())
        }
        "event_callback" => {
            handle_slack_event_callback(&state, envelope).await?;
            Ok(Json(SlackEventAck { ok: true }).into_response())
        }
        _ => Ok(Json(SlackEventAck { ok: true }).into_response()),
    }
}

pub async fn send_test_message(
    State(state): State<AppState>,
    Path(organization_id): Path<Uuid>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<PostedMessage>, SlackError> {
    let user = authenticated_user(&state, &jar, &headers).await?;
    require_admin_membership(&state, user.id, organization_id).await?;
    let slack_client = state
        .slack_client
        .as_ref()
        .ok_or(SlackError::NotConfigured)?;
    if slack_client.organization_id() != organization_id {
        return Err(SlackError::NotConfiguredForOrganization);
    }

    Ok(Json(slack_client.post_test_message().await?))
}

async fn complete_oauth_callback(
    state: &AppState,
    query: CallbackQuery,
) -> Result<String, SlackError> {
    let slack_cloud = state
        .config
        .slack_cloud
        .as_ref()
        .ok_or(SlackError::CloudNotConfigured)?;
    if query.error.is_some() {
        return Err(SlackError::OAuthRejected);
    }
    let code = query.code.ok_or(SlackError::InvalidOAuthState)?;
    let oauth_state = query.state.ok_or(SlackError::InvalidOAuthState)?;
    let flow = consume_slack_oauth_state(state, &oauth_state).await?;
    let response = exchange_slack_oauth_code(state, slack_cloud, &code).await?;
    store_slack_installation(state, flow.organization_id, flow.user_id, response).await?;
    Ok(flow.return_to.unwrap_or_else(|| {
        format!(
            "{}/settings/integrations",
            state.config.web_url.trim_end_matches('/')
        )
    }))
}

async fn consume_slack_oauth_state(
    state: &AppState,
    oauth_state: &str,
) -> Result<SlackOAuthState, SlackError> {
    let row = sqlx::query(
        "update slack_oauth_states
         set consumed_at = now()
         where state_hash = $1
           and expires_at > now()
           and consumed_at is null
         returning organization_id, user_id, return_to",
    )
    .bind(hash_token(oauth_state))
    .fetch_optional(&state.db)
    .await?
    .ok_or(SlackError::InvalidOAuthState)?;

    Ok(SlackOAuthState {
        organization_id: row.get("organization_id"),
        user_id: row.get("user_id"),
        return_to: row.get("return_to"),
    })
}

async fn exchange_slack_oauth_code(
    state: &AppState,
    config: &SlackCloudConfig,
    code: &str,
) -> Result<SlackOAuthAccessResponse, SlackError> {
    let response = state
        .provider_client
        .post("https://slack.com/api/oauth.v2.access")
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(%error, "Slack OAuth token exchange request failed");
            SlackError::SlackUnavailable
        })?;
    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "Slack OAuth token exchange returned an error status");
        return Err(SlackError::SlackUnavailable);
    }
    let response = response
        .json::<SlackOAuthAccessResponse>()
        .await
        .map_err(|error| {
            tracing::warn!(%error, "Slack OAuth token exchange returned invalid JSON");
            SlackError::SlackUnavailable
        })?;
    if !response.ok {
        tracing::warn!(
            error = response.error.as_deref(),
            "Slack OAuth token exchange was rejected"
        );
        return Err(SlackError::OAuthRejected);
    }
    validate_oauth_access_response(&response)?;
    Ok(response)
}

fn validate_oauth_access_response(response: &SlackOAuthAccessResponse) -> Result<(), SlackError> {
    let scopes = parse_scope_list(response.scope.as_deref().unwrap_or_default());
    for required in SLACK_OAUTH_SCOPE.split(',') {
        if !scopes.iter().any(|scope| scope == required) {
            return Err(SlackError::MissingRequiredScope);
        }
    }
    if response.access_token.as_deref().is_none_or(str::is_empty)
        || response.app_id.as_deref().is_none_or(str::is_empty)
        || response.bot_user_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(SlackError::InvalidOAuthResponse);
    }
    let team = response
        .team
        .as_ref()
        .ok_or(SlackError::InvalidOAuthResponse)?;
    if team.id.trim().is_empty() || team.name.trim().is_empty() {
        return Err(SlackError::InvalidOAuthResponse);
    }
    Ok(())
}

async fn store_slack_installation(
    state: &AppState,
    organization_id: Uuid,
    user_id: Uuid,
    response: SlackOAuthAccessResponse,
) -> Result<(), SlackError> {
    let access_token = response
        .access_token
        .as_deref()
        .ok_or(SlackError::InvalidOAuthResponse)?;
    let (ciphertext, nonce) = state
        .credential_cipher
        .encrypt(access_token.as_bytes())
        .map_err(|_| SlackError::Encryption)?;
    let credential_fingerprint = credentials::fingerprint(access_token);
    let team = response
        .team
        .as_ref()
        .ok_or(SlackError::InvalidOAuthResponse)?;
    let scopes = parse_scope_list(response.scope.as_deref().unwrap_or_default());
    let display_name = format!("Slack - {}", truncate(&team.name, 112));

    let mut transaction = state.db.begin().await?;
    let integration_id = upsert_slack_integration(
        &mut transaction,
        organization_id,
        &display_name,
        user_id,
        &ciphertext,
        &nonce,
        &credential_fingerprint,
    )
    .await?;
    upsert_slack_installation(
        &mut transaction,
        integration_id,
        organization_id,
        user_id,
        &response,
        &scopes,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn upsert_slack_integration(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    display_name: &str,
    user_id: Uuid,
    ciphertext: &[u8],
    nonce: &[u8; 12],
    credential_fingerprint: &str,
) -> Result<Uuid, SlackError> {
    let integration_id = Uuid::new_v4();
    sqlx::query(
        "insert into integrations
         (id, organization_id, integration_type, display_name, status, metadata)
         values ($1, $2, 'slack', $3, 'connected', '{}'::jsonb)
         on conflict (organization_id, integration_type, display_name)
         do update set status = 'connected', updated_at = now()
         returning id",
    )
    .bind(integration_id)
    .bind(organization_id)
    .bind(display_name)
    .fetch_one(&mut **transaction)
    .await?;

    let integration_row = sqlx::query(
        "select id from integrations
         where organization_id = $1
           and integration_type = 'slack'
           and display_name = $2",
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
         values ($1, $2, 'bot_token', $3, $4, $5, $6)
         on conflict (integration_id, kind)
         do update set credential_ciphertext = excluded.credential_ciphertext,
                       credential_nonce = excluded.credential_nonce,
                       credential_fingerprint = excluded.credential_fingerprint,
                       updated_at = now()",
    )
    .bind(Uuid::new_v4())
    .bind(integration_id)
    .bind(ciphertext)
    .bind(nonce.as_slice())
    .bind(credential_fingerprint)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;

    Ok(integration_id)
}

async fn upsert_slack_installation(
    transaction: &mut Transaction<'_, Postgres>,
    integration_id: Uuid,
    organization_id: Uuid,
    user_id: Uuid,
    response: &SlackOAuthAccessResponse,
    scopes: &[String],
) -> Result<(), SlackError> {
    let team = response
        .team
        .as_ref()
        .ok_or(SlackError::InvalidOAuthResponse)?;
    let app_id = response
        .app_id
        .as_deref()
        .ok_or(SlackError::InvalidOAuthResponse)?;
    let bot_user_id = response
        .bot_user_id
        .as_deref()
        .ok_or(SlackError::InvalidOAuthResponse)?;
    let enterprise_id = response
        .enterprise
        .as_ref()
        .map(|enterprise| enterprise.id.as_str());

    sqlx::query(
        "insert into slack_installations
         (id, integration_id, organization_id, slack_team_id, slack_team_name,
          slack_enterprise_id, slack_app_id, slack_bot_user_id, scopes, status,
          status_reason, installed_by, installed_at, revoked_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'connected',
                 null, $10, now(), null)
         on conflict (organization_id, slack_team_id)
         do update set integration_id = excluded.integration_id,
                       slack_team_name = excluded.slack_team_name,
                       slack_enterprise_id = excluded.slack_enterprise_id,
                       slack_app_id = excluded.slack_app_id,
                       slack_bot_user_id = excluded.slack_bot_user_id,
                       scopes = excluded.scopes,
                       status = 'connected',
                       status_reason = null,
                       installed_by = excluded.installed_by,
                       installed_at = now(),
                       revoked_at = null,
                       updated_at = now()",
    )
    .bind(Uuid::new_v4())
    .bind(integration_id)
    .bind(organization_id)
    .bind(team.id.trim())
    .bind(truncate(&team.name, 120))
    .bind(enterprise_id)
    .bind(app_id.trim())
    .bind(bot_user_id.trim())
    .bind(scopes)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

async fn handle_slack_event_callback(
    state: &AppState,
    envelope: SlackEventEnvelope,
) -> Result<(), SlackError> {
    let event_id = envelope.event_id.ok_or(SlackError::InvalidEventPayload)?;
    let event_type = envelope
        .event
        .as_ref()
        .map(|event| event.event_type.as_str())
        .unwrap_or("unknown");
    let installation = find_slack_installation(
        state,
        envelope.team_id.as_deref(),
        envelope.api_app_id.as_deref(),
    )
    .await?;
    let inserted = record_slack_event(
        state,
        &event_id,
        envelope.team_id.as_deref(),
        envelope.api_app_id.as_deref(),
        event_type,
        installation
            .as_ref()
            .map(|installation| installation.organization_id),
        installation.as_ref().map(|installation| installation.id),
    )
    .await?;
    if !inserted {
        return Ok(());
    }

    if let Some(installation) = installation {
        match event_type {
            "app_uninstalled" => {
                mark_slack_installation_uninstalled(state, installation.id).await?;
                mark_slack_event_processed(state, &event_id, "processed", None).await?;
            }
            "tokens_revoked" => {
                mark_slack_installation_needs_attention(state, installation.id, "token_revoked")
                    .await?;
                mark_slack_event_processed(state, &event_id, "processed", None).await?;
            }
            _ => {
                mark_slack_event_processed(state, &event_id, "ignored", None).await?;
            }
        }
    } else {
        mark_slack_event_processed(state, &event_id, "ignored", Some("installation_not_found"))
            .await?;
    }
    Ok(())
}

struct SlackInstallationRef {
    id: Uuid,
    organization_id: Uuid,
}

async fn find_slack_installation(
    state: &AppState,
    slack_team_id: Option<&str>,
    slack_app_id: Option<&str>,
) -> Result<Option<SlackInstallationRef>, SlackError> {
    let Some(slack_team_id) = slack_team_id else {
        return Ok(None);
    };
    let Some(slack_app_id) = slack_app_id else {
        return Ok(None);
    };
    let row = sqlx::query(
        "select id, organization_id
         from slack_installations
         where slack_team_id = $1 and slack_app_id = $2
         order by updated_at desc
         limit 1",
    )
    .bind(slack_team_id)
    .bind(slack_app_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(row.map(|row| SlackInstallationRef {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
    }))
}

async fn record_slack_event(
    state: &AppState,
    slack_event_id: &str,
    slack_team_id: Option<&str>,
    slack_app_id: Option<&str>,
    slack_event_type: &str,
    organization_id: Option<Uuid>,
    slack_installation_id: Option<Uuid>,
) -> Result<bool, SlackError> {
    let row = sqlx::query(
        "insert into slack_events
         (id, slack_event_id, slack_team_id, slack_app_id, slack_event_type,
          organization_id, slack_installation_id)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict (slack_event_id) do nothing
         returning id",
    )
    .bind(Uuid::new_v4())
    .bind(slack_event_id)
    .bind(slack_team_id)
    .bind(slack_app_id)
    .bind(truncate(slack_event_type, 120))
    .bind(organization_id)
    .bind(slack_installation_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(row.is_some())
}

async fn mark_slack_event_processed(
    state: &AppState,
    slack_event_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), SlackError> {
    sqlx::query(
        "update slack_events
         set status = $2,
             last_error = $3,
             processed_at = now(),
             updated_at = now()
         where slack_event_id = $1",
    )
    .bind(slack_event_id)
    .bind(status)
    .bind(error.map(|value| truncate(value, 1_000)))
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn mark_slack_installation_uninstalled(
    state: &AppState,
    slack_installation_id: Uuid,
) -> Result<(), SlackError> {
    sqlx::query(
        "update slack_installations
         set status = 'uninstalled',
             status_reason = 'app_uninstalled',
             revoked_at = now(),
             updated_at = now()
         where id = $1",
    )
    .bind(slack_installation_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn mark_slack_installation_needs_attention(
    state: &AppState,
    slack_installation_id: Uuid,
    reason: &str,
) -> Result<(), SlackError> {
    sqlx::query(
        "update slack_installations
         set status = 'needs_attention',
             status_reason = $2,
             updated_at = now()
         where id = $1",
    )
    .bind(slack_installation_id)
    .bind(reason)
    .execute(&state.db)
    .await?;
    Ok(())
}

fn build_slack_install_url(
    config: &SlackCloudConfig,
    oauth_state: &str,
) -> Result<String, SlackError> {
    let mut url = reqwest::Url::parse("https://slack.com/oauth/v2/authorize")
        .map_err(|_| SlackError::Configuration)?;
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("scope", SLACK_OAUTH_SCOPE)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("state", oauth_state);
    Ok(url.to_string())
}

async fn cleanup_expired_slack_oauth_states(state: &AppState) -> Result<(), SlackError> {
    sqlx::query("delete from slack_oauth_states where expires_at <= now()")
        .execute(&state.db)
        .await?;
    Ok(())
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn verify_slack_signature(
    headers: &HeaderMap,
    body: &[u8],
    signing_secret: &str,
) -> Result<(), SlackError> {
    let timestamp = header_value(headers, "x-slack-request-timestamp")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(SlackError::InvalidSignature)?;
    let now = Utc::now().timestamp();
    if (now - timestamp).abs() > SLACK_SIGNATURE_MAX_AGE_SECONDS {
        return Err(SlackError::InvalidSignature);
    }

    let signature =
        header_value(headers, "x-slack-signature").ok_or(SlackError::InvalidSignature)?;
    let signature_hex = signature
        .strip_prefix("v0=")
        .ok_or(SlackError::InvalidSignature)?;
    let signature_bytes = decode_hex(signature_hex).ok_or(SlackError::InvalidSignature)?;

    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|_| SlackError::Configuration)?;
    mac.update(SLACK_SIGNATURE_VERSION.as_bytes());
    mac.update(b":");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b":");
    mac.update(body);
    mac.verify_slice(&signature_bytes)
        .map_err(|_| SlackError::InvalidSignature)
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let high = hex_value(chunk[0])?;
            let low = hex_value(chunk[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn is_safe_return_to(value: &str, web_url: &str) -> bool {
    let Ok(return_to) = reqwest::Url::parse(value) else {
        return false;
    };
    let Ok(web) = reqwest::Url::parse(web_url) else {
        return false;
    };
    return_to.origin() == web.origin()
}

fn redirect_with_slack_status(return_to: &str, status: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(return_to) else {
        return format!("{return_to}?slack={status}");
    };
    url.query_pairs_mut().append_pair("slack", status);
    url.to_string()
}

fn parse_scope_list(scope: &str) -> Vec<String> {
    scope
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn truncate(value: &str, max_len: usize) -> String {
    value.trim().chars().take(max_len).collect()
}

async fn require_admin_membership(
    state: &AppState,
    user_id: Uuid,
    organization_id: Uuid,
) -> Result<(), SlackError> {
    let role = sqlx::query(
        "select role from organization_memberships where organization_id = $1 and user_id = $2",
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|row| row.get::<String, _>("role"))
    .ok_or(SlackError::NotFound)?;

    if !matches!(role.as_str(), "owner" | "admin") {
        return Err(SlackError::Forbidden);
    }
    Ok(())
}

#[derive(Debug)]
pub enum SlackError {
    Auth(AuthError),
    Client(SlackClientError),
    Configuration,
    CloudNotConfigured,
    Database(sqlx::Error),
    Encryption,
    Forbidden,
    InvalidOAuthResponse,
    InvalidOAuthState,
    InvalidEventPayload,
    InvalidSignature,
    MissingRequiredScope,
    NotConfigured,
    NotConfiguredForOrganization,
    NotFound,
    OAuthRejected,
    SlackUnavailable,
}

impl From<AuthError> for SlackError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl From<SlackClientError> for SlackError {
    fn from(error: SlackClientError) -> Self {
        Self::Client(error)
    }
}

impl From<sqlx::Error> for SlackError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for SlackError {
    fn into_response(self) -> Response {
        match self {
            Self::Auth(error) => error.into_response(),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "organization admin access required" })),
            )
                .into_response(),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "organization not found" })),
            )
                .into_response(),
            Self::NotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Slack integration is not configured" })),
            )
                .into_response(),
            Self::CloudNotConfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Slack Cloud installation is not configured"
                })),
            )
                .into_response(),
            Self::Configuration => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Slack integration is misconfigured" })),
            )
                .into_response(),
            Self::Encryption => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Slack integration request failed" })),
            )
                .into_response(),
            Self::InvalidOAuthResponse | Self::MissingRequiredScope | Self::OAuthRejected => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack authorization failed" })),
            )
                .into_response(),
            Self::InvalidOAuthState => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Slack authorization session expired" })),
            )
                .into_response(),
            Self::InvalidEventPayload => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid Slack event payload" })),
            )
                .into_response(),
            Self::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid Slack signature" })),
            )
                .into_response(),
            Self::SlackUnavailable => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack is unavailable" })),
            )
                .into_response(),
            Self::NotConfiguredForOrganization => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Slack is configured for a different organization"
                })),
            )
                .into_response(),
            Self::Client(SlackClientError::ChannelArchived) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Slack channel is archived" })),
            )
                .into_response(),
            Self::Client(SlackClientError::ChannelNotFound) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Slack channel was not found" })),
            )
                .into_response(),
            Self::Client(SlackClientError::MissingScope) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Slack installation is missing a required scope"
                })),
            )
                .into_response(),
            Self::Client(SlackClientError::NotInChannel) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Slack bot is not in the channel" })),
            )
                .into_response(),
            Self::Client(SlackClientError::RateLimited { .. }) => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "Slack rate limited the message" })),
            )
                .into_response(),
            Self::Client(SlackClientError::Rejected { .. }) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack rejected the message" })),
            )
                .into_response(),
            Self::Client(SlackClientError::TokenRevoked) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Slack installation needs attention"
                })),
            )
                .into_response(),
            Self::Client(SlackClientError::Unavailable) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "Slack is unavailable" })),
            )
                .into_response(),
            Self::Database(error) => {
                tracing::error!(%error, "Slack integration request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Slack integration request failed" })),
                )
                    .into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SLACK_OAUTH_SCOPE, SlackOAuthAccessResponse, SlackOAuthTeam, build_slack_install_url,
        decode_hex, is_safe_return_to, parse_scope_list, redirect_with_slack_status,
        validate_oauth_access_response, verify_slack_signature,
    };
    use crate::SlackCloudConfig;
    use axum::http::HeaderMap;
    use chrono::Utc;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type TestHmacSha256 = Hmac<Sha256>;

    #[test]
    fn builds_slack_install_url_with_minimal_bot_scopes() {
        let config = SlackCloudConfig::from_values(
            Some("123.abc".to_owned()),
            Some("client-secret".to_owned()),
            Some("signing-secret".to_owned()),
            Some("https://api.bella.example/v1/slack/oauth/callback".to_owned()),
        )
        .unwrap()
        .unwrap();

        let install_url = build_slack_install_url(&config, "state-token").unwrap();
        let url = reqwest::Url::parse(&install_url).unwrap();
        let pairs = url.query_pairs().collect::<Vec<_>>();

        assert_eq!(
            url.as_str().split('?').next().unwrap(),
            "https://slack.com/oauth/v2/authorize"
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "client_id" && value == "123.abc")
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "scope" && value == SLACK_OAUTH_SCOPE)
        );
        assert!(pairs.iter().any(|(key, value)| key == "redirect_uri"
            && value == "https://api.bella.example/v1/slack/oauth/callback"));
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "state" && value == "state-token")
        );
    }

    #[test]
    fn accepts_return_urls_only_on_web_origin() {
        assert!(is_safe_return_to(
            "https://app.bella.example/settings/integrations",
            "https://app.bella.example"
        ));
        assert!(!is_safe_return_to(
            "https://evil.example/settings/integrations",
            "https://app.bella.example"
        ));
        assert!(!is_safe_return_to(
            "/settings/integrations",
            "https://app.bella.example"
        ));
    }

    #[test]
    fn appends_slack_status_to_redirect_url() {
        assert_eq!(
            redirect_with_slack_status(
                "https://app.bella.example/settings/integrations",
                "installed"
            ),
            "https://app.bella.example/settings/integrations?slack=installed"
        );
        assert_eq!(
            redirect_with_slack_status(
                "https://app.bella.example/settings/integrations?tab=chat",
                "error"
            ),
            "https://app.bella.example/settings/integrations?tab=chat&slack=error"
        );
    }

    #[test]
    fn parses_comma_separated_slack_scopes() {
        assert_eq!(
            parse_scope_list("chat:write, channels:read,,groups:read"),
            vec!["chat:write", "channels:read", "groups:read"]
        );
    }

    #[test]
    fn validates_required_oauth_response_fields_and_scopes() {
        let response = SlackOAuthAccessResponse {
            ok: true,
            error: None,
            access_token: Some("xoxb-token".to_owned()),
            scope: Some(SLACK_OAUTH_SCOPE.to_owned()),
            app_id: Some("A123".to_owned()),
            bot_user_id: Some("U123".to_owned()),
            team: Some(SlackOAuthTeam {
                id: "T123".to_owned(),
                name: "Acme".to_owned(),
            }),
            enterprise: None,
        };

        validate_oauth_access_response(&response).unwrap();
    }

    #[test]
    fn rejects_oauth_response_missing_required_scope() {
        let response = SlackOAuthAccessResponse {
            ok: true,
            error: None,
            access_token: Some("xoxb-token".to_owned()),
            scope: Some("chat:write,channels:read".to_owned()),
            app_id: Some("A123".to_owned()),
            bot_user_id: Some("U123".to_owned()),
            team: Some(SlackOAuthTeam {
                id: "T123".to_owned(),
                name: "Acme".to_owned(),
            }),
            enterprise: None,
        };

        assert!(validate_oauth_access_response(&response).is_err());
    }

    #[test]
    fn verifies_valid_slack_signature() {
        let body = br#"{"type":"event_callback","event_id":"Ev123"}"#;
        let timestamp = Utc::now().timestamp().to_string();
        let signature = test_signature("signing-secret", &timestamp, body);
        let mut headers = HeaderMap::new();
        headers.insert("x-slack-request-timestamp", timestamp.parse().unwrap());
        headers.insert("x-slack-signature", signature.parse().unwrap());

        verify_slack_signature(&headers, body, "signing-secret").unwrap();
    }

    #[test]
    fn rejects_stale_slack_signature_timestamp() {
        let body = br#"{"type":"event_callback","event_id":"Ev123"}"#;
        let timestamp = (Utc::now().timestamp() - 600).to_string();
        let signature = test_signature("signing-secret", &timestamp, body);
        let mut headers = HeaderMap::new();
        headers.insert("x-slack-request-timestamp", timestamp.parse().unwrap());
        headers.insert("x-slack-signature", signature.parse().unwrap());

        assert!(verify_slack_signature(&headers, body, "signing-secret").is_err());
    }

    #[test]
    fn rejects_malformed_hex_signatures() {
        assert!(decode_hex("abc").is_none());
        assert!(decode_hex("zz").is_none());
    }

    fn test_signature(signing_secret: &str, timestamp: &str, body: &[u8]) -> String {
        let mut mac = TestHmacSha256::new_from_slice(signing_secret.as_bytes()).unwrap();
        mac.update(b"v0:");
        mac.update(timestamp.as_bytes());
        mac.update(b":");
        mac.update(body);
        format!("v0={}", encode_hex(&mac.finalize().into_bytes()))
    }

    fn encode_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

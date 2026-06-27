mod agent;
mod agent_settings;
mod auth;
mod credentials;
mod incidents;
mod integrations;
mod organizations;
mod posthog_ingestion;
mod provider_accounts;
mod provider_validation;
mod reporting;
mod sdk_ingestion;
mod security;
mod slack;

use axum::{
    Json, Router,
    http::{HeaderName, Method, StatusCode, header},
    middleware,
    response::IntoResponse,
    routing::{get, patch, post},
};
use bella_db::DbPool;
use bella_slack::{SlackClient, SlackConfig};
use serde::Serialize;
use std::{env, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Clone)]
struct AppState {
    db: DbPool,
    config: Config,
    credential_cipher: credentials::CredentialCipher,
    provider_client: reqwest::Client,
    slack_client: Option<SlackClient>,
    rate_limiter: Arc<security::RateLimiter>,
}

#[derive(Clone)]
struct Config {
    github_client_id: String,
    github_client_secret: String,
    github_allowed_emails: Vec<String>,
    public_api_url: String,
    web_url: String,
    secure_cookies: bool,
    openai_base_url: String,
    slack: Option<SlackConfig>,
    posthog_webhook_secret: Option<String>,
    allow_global_posthog_webhook_secret: bool,
    posthog_ingestion_enabled: bool,
    resend_api_key: Option<String>,
    email_from: Option<String>,
    allowed_origins: Vec<String>,
    trust_proxy_headers: bool,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let github_client_id = required_env("GITHUB_OAUTH_CLIENT_ID")?;
        let github_client_secret = required_env("GITHUB_OAUTH_CLIENT_SECRET")?;
        let github_allowed_emails = parse_csv_env("BELLA_ALLOWED_GITHUB_EMAILS")?;
        let public_api_url = env::var("BELLA_PUBLIC_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let web_url =
            env::var("BELLA_WEB_URL").unwrap_or_else(|_| "http://127.0.0.1:5173".to_string());
        let openai_base_url =
            env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_string());
        let posthog_webhook_secret = env::var("POSTHOG_WEBHOOK_SECRET")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let allow_global_posthog_webhook_secret = parse_bool_env(
            "BELLA_ALLOW_GLOBAL_POSTHOG_WEBHOOK_SECRET",
            cfg!(debug_assertions),
        )?;
        if posthog_webhook_secret.is_some() && !allow_global_posthog_webhook_secret {
            anyhow::bail!(
                "POSTHOG_WEBHOOK_SECRET is disabled by default; use per-integration secrets or set BELLA_ALLOW_GLOBAL_POSTHOG_WEBHOOK_SECRET=true"
            );
        }
        let posthog_ingestion_enabled = parse_bool_env("BELLA_POSTHOG_INGESTION_ENABLED", true)?;
        let resend_api_key = env::var("RESEND_API_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let email_from = env::var("BELLA_EMAIL_FROM")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let public_api = reqwest::Url::parse(&public_api_url)
            .map_err(|error| anyhow::anyhow!("invalid BELLA_PUBLIC_API_URL: {error}"))?;
        let web = reqwest::Url::parse(&web_url)
            .map_err(|error| anyhow::anyhow!("invalid BELLA_WEB_URL: {error}"))?;
        if !matches!(public_api.scheme(), "http" | "https")
            || !matches!(web.scheme(), "http" | "https")
        {
            anyhow::bail!("Bella public URLs must use http or https");
        }

        let secure_cookies = match env::var("BELLA_SECURE_COOKIES") {
            Ok(value) => value.parse::<bool>().map_err(|_| {
                anyhow::anyhow!("BELLA_SECURE_COOKIES must be either true or false")
            })?,
            Err(env::VarError::NotPresent) => public_api.scheme() == "https",
            Err(error) => return Err(error.into()),
        };
        if public_api.scheme() == "https" && !secure_cookies {
            anyhow::bail!("BELLA_SECURE_COOKIES must be true when BELLA_PUBLIC_API_URL uses HTTPS");
        }

        let slack = SlackConfig::from_env()?;
        let allowed_origins = parse_origin_allowlist(&web_url)?;
        let trust_proxy_headers = parse_bool_env("BELLA_TRUST_PROXY_HEADERS", false)?;

        Ok(Self {
            github_client_id,
            github_client_secret,
            github_allowed_emails,
            public_api_url,
            web_url,
            secure_cookies,
            openai_base_url,
            slack,
            posthog_webhook_secret,
            allow_global_posthog_webhook_secret,
            posthog_ingestion_enabled,
            resend_api_key,
            email_from,
            allowed_origins,
            trust_proxy_headers,
        })
    }
}

fn parse_bool_env(name: &str, default: bool) -> anyhow::Result<bool> {
    match env::var(name) {
        Ok(value) => value
            .parse::<bool>()
            .map_err(|_| anyhow::anyhow!("{name} must be either true or false")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn parse_origin_allowlist(web_url: &str) -> anyhow::Result<Vec<String>> {
    let configured = env::var("BELLA_ALLOWED_ORIGINS").unwrap_or_else(|_| web_url.to_string());
    let mut origins = Vec::new();
    for value in configured
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let url = reqwest::Url::parse(value).map_err(|error| {
            anyhow::anyhow!("invalid BELLA_ALLOWED_ORIGINS entry {value:?}: {error}")
        })?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            anyhow::bail!("BELLA_ALLOWED_ORIGINS entries must be http or https origins");
        }
        let origin = url.origin().ascii_serialization();
        if !origins.contains(&origin) {
            origins.push(origin);
        }
    }
    if origins.is_empty() {
        anyhow::bail!("BELLA_ALLOWED_ORIGINS must include at least one trusted web origin");
    }
    Ok(origins)
}

fn parse_csv_env(name: &str) -> anyhow::Result<Vec<String>> {
    match env::var(name) {
        Ok(value) => Ok(value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()),
        Err(env::VarError::NotPresent) => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn required_env(name: &str) -> anyhow::Result<String> {
    let value = env::var(name)?;
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(value)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    postgres: &'static str,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .compact()
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://bella:bella@127.0.0.1:5432/bella".to_string());
    let bind_addr: SocketAddr = match env::var("BELLA_API_BIND_ADDR") {
        Ok(value) => value.parse()?,
        Err(env::VarError::NotPresent) => {
            let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
            format!("0.0.0.0:{port}").parse()?
        }
        Err(error) => return Err(error.into()),
    };
    let config = Config::from_env()?;
    let credential_cipher = credentials::CredentialCipher::from_base64(&required_env(
        "BELLA_CREDENTIAL_ENCRYPTION_KEY",
    )?)?;
    let provider_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let slack_client = config
        .slack
        .as_ref()
        .map(|slack_config| SlackClient::new(provider_client.clone(), slack_config.clone()));

    let db = bella_db::connect(&database_url).await?;
    bella_db::run_migrations(&db).await?;
    let allowed_origins = Arc::new(config.allowed_origins.clone());
    let app_state = AppState {
        db,
        config,
        credential_cipher,
        provider_client,
        slack_client,
        rate_limiter: Arc::new(security::RateLimiter::new()),
    };
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            origin
                .to_str()
                .is_ok_and(|value| allowed_origins.iter().any(|allowed| allowed == value))
        }))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static("x-bella-webhook-secret"),
            HeaderName::from_static("x-posthog-webhook-secret"),
        ]);

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/auth/github/start", get(auth::web_start))
        .route("/v1/auth/github/callback", get(auth::callback))
        .route("/v1/auth/logout", post(auth::logout))
        .route("/v1/auth/token/revoke", post(auth::revoke_token))
        .route("/v1/auth/cli/start", post(auth::cli_start))
        .route("/v1/auth/cli/poll", post(auth::cli_poll))
        .route("/v1/me", get(auth::me))
        .route(
            "/v1/organizations",
            get(organizations::list).post(organizations::create),
        )
        .route("/v1/providers", get(provider_accounts::catalog))
        .route(
            "/v1/organizations/:organization_id/provider-accounts",
            get(provider_accounts::list).post(provider_accounts::upsert),
        )
        .route(
            "/v1/organizations/:organization_id/provider-accounts/:account_id",
            patch(provider_accounts::update).delete(provider_accounts::delete),
        )
        .route(
            "/v1/organizations/:organization_id/provider-accounts/:account_id/sync",
            post(provider_accounts::sync_now),
        )
        .route(
            "/v1/organizations/:organization_id/usage/summary",
            get(reporting::summary),
        )
        .route(
            "/v1/organizations/:organization_id/sdk/usage-events",
            post(sdk_ingestion::record_usage_event),
        )
        .route(
            "/v1/organizations/:organization_id/webhooks/posthog",
            post(posthog_ingestion::webhook),
        )
        .route(
            "/v1/organizations/:organization_id/integrations/posthog/check",
            post(posthog_ingestion::check_connection),
        )
        .route(
            "/v1/organizations/:organization_id/integrations/posthog/sync",
            post(posthog_ingestion::sync_now),
        )
        .route(
            "/v1/organizations/:organization_id/incidents",
            get(incidents::list),
        )
        .route(
            "/v1/organizations/:organization_id/incidents/:incident_id",
            get(incidents::get).patch(incidents::update),
        )
        .route(
            "/v1/organizations/:organization_id/integrations",
            get(integrations::list),
        )
        .route(
            "/v1/organizations/:organization_id/integrations/posthog",
            post(integrations::connect_posthog),
        )
        .route(
            "/v1/organizations/:organization_id/agent/messages",
            post(agent::message),
        )
        .route(
            "/v1/organizations/:organization_id/agent/settings",
            get(agent_settings::list_settings).post(agent_settings::create_settings),
        )
        .route(
            "/v1/organizations/:organization_id/agent/settings/:setting_id",
            axum::routing::put(agent_settings::update_settings)
                .delete(agent_settings::delete_settings),
        )
        .route(
            "/v1/organizations/:organization_id/agent/settings/:setting_id/default",
            post(agent_settings::set_default),
        )
        .route(
            "/v1/organizations/:organization_id/integrations/slack/test-message",
            post(slack::send_test_message),
        )
        .route(
            "/v1/organizations/:organization_id/members",
            get(organizations::members),
        )
        .route(
            "/v1/organizations/:organization_id/members/:member_user_id",
            patch(organizations::update_member).delete(organizations::remove_member),
        )
        .route(
            "/v1/organizations/:organization_id/invitations",
            post(organizations::create_invitation),
        )
        .route(
            "/v1/organizations/:organization_id/invitations/:invitation_id",
            axum::routing::delete(organizations::revoke_invitation),
        )
        .route(
            "/v1/invitations/accept",
            post(organizations::accept_invitation),
        )
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            security::guard,
        ))
        .with_state(app_state);

    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!("bella-api listening on http://{bind_addr}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    match bella_db::health_check(&state.db).await {
        Ok(()) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                postgres: "ok",
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "degraded",
                postgres: "unavailable",
            }),
        ),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

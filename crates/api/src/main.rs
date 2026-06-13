mod auth;
mod credentials;
mod organizations;
mod provider_accounts;
mod provider_validation;

use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
};
use bella_db::DbPool;
use serde::Serialize;
use std::{env, net::SocketAddr};
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Clone)]
struct AppState {
    db: DbPool,
    config: Config,
    credential_cipher: credentials::CredentialCipher,
    provider_client: reqwest::Client,
}

#[derive(Clone)]
struct Config {
    github_client_id: String,
    github_client_secret: String,
    public_api_url: String,
    web_url: String,
    secure_cookies: bool,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let github_client_id = required_env("GITHUB_OAUTH_CLIENT_ID")?;
        let github_client_secret = required_env("GITHUB_OAUTH_CLIENT_SECRET")?;
        let public_api_url = env::var("BELLA_PUBLIC_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let web_url =
            env::var("BELLA_WEB_URL").unwrap_or_else(|_| "http://127.0.0.1:5173".to_string());

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

        Ok(Self {
            github_client_id,
            github_client_secret,
            public_api_url,
            web_url,
            secure_cookies,
        })
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
    let bind_addr: SocketAddr = env::var("BELLA_API_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()?;
    let config = Config::from_env()?;
    let credential_cipher = credentials::CredentialCipher::from_base64(&required_env(
        "BELLA_CREDENTIAL_ENCRYPTION_KEY",
    )?)?;
    let provider_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let db = bella_db::connect(&database_url).await?;
    bella_db::run_migrations(&db).await?;

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
        .with_state(AppState {
            db,
            config,
            credential_cipher,
            provider_client,
        });

    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!("bella-api listening on http://{bind_addr}");

    axum::serve(listener, app)
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

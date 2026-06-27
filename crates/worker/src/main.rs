mod incident_delivery;

use std::{env, net::SocketAddr, time::Duration};

use axum::{Router, routing::get};
use bella_ingestion::posthog::PosthogIngestor;
use bella_slack::{SlackClient, SlackConfig};
use sqlx::Row;
use tracing_subscriber::{EnvFilter, fmt};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .compact()
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://bella:bella@127.0.0.1:5432/bella".to_string());
    let openai_base_url =
        env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_string());
    let poll_interval = env::var("BELLA_WORKER_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);
    let posthog_ingestion_enabled = parse_bool_env("BELLA_POSTHOG_INGESTION_ENABLED", true)?;
    let posthog_sync_interval = env::var("BELLA_POSTHOG_SYNC_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(300);
    let credential_cipher = bella_ingestion::credentials::CredentialCipher::from_base64(
        &required_env("BELLA_CREDENTIAL_ENCRYPTION_KEY")?,
    )?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let slack_client =
        SlackConfig::from_env()?.map(|config| SlackClient::new(client.clone(), config));

    let db = bella_db::connect(&database_url).await?;
    bella_db::run_migrations(&db).await?;
    let ingestor = bella_ingestion::openai::OpenAiIngestor::new(
        db.clone(),
        client.clone(),
        credential_cipher.clone(),
        openai_base_url,
    );
    let posthog_ingestor = PosthogIngestor::new(
        db.clone(),
        client,
        credential_cipher,
        posthog_ingestion_enabled,
    );
    let incident_delivery =
        incident_delivery::IncidentDeliveryWorker::new(db.clone(), slack_client);
    spawn_health_server();

    tracing::info!(
        poll_interval,
        posthog_ingestion_enabled,
        posthog_sync_interval,
        "bella-worker started"
    );
    loop {
        let account_ids = due_openai_accounts(&db).await?;
        for account_id in account_ids {
            match ingestor.sync_account(account_id).await {
                Ok(outcome) => tracing::info!(
                    sync_run_id = %outcome.sync_run_id,
                    provider_account_id = %outcome.provider_account_id,
                    usage_buckets = outcome.usage_buckets,
                    cost_snapshots = outcome.cost_snapshots,
                    "provider account synced"
                ),
                Err(error) => tracing::warn!(%error, %account_id, "provider account sync failed"),
            }
        }
        let integration_ids = posthog_ingestor
            .due_integrations(Duration::from_secs(posthog_sync_interval), 10)
            .await?;
        for integration_id in integration_ids {
            match posthog_ingestor.sync_integration(integration_id).await {
                Ok(outcome) => tracing::info!(
                    sync_run_id = %outcome.sync_run_id,
                    integration_id = %outcome.integration_id,
                    organization_id = %outcome.organization_id,
                    signals_seen = outcome.signals_seen,
                    signals_upserted = outcome.signals_upserted,
                    incident_candidates_created = outcome.incident_candidates_created,
                    "PostHog integration synced"
                ),
                Err(error) => {
                    tracing::warn!(%error, %integration_id, "PostHog integration sync failed")
                }
            }
        }
        if let Err(error) = incident_delivery.process_due().await {
            tracing::warn!(%error, "incident delivery processing failed");
        }
        tokio::time::sleep(Duration::from_secs(poll_interval)).await;
    }
}

fn spawn_health_server() {
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3001);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    tokio::spawn(async move {
        let app = Router::new().route("/health", get(|| async { "ok" }));
        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(%error, %address, "worker health server failed to bind");
                return;
            }
        };
        tracing::info!(%address, "worker health server listening");
        if let Err(error) = axum::serve(listener, app).await {
            tracing::warn!(%error, "worker health server stopped");
        }
    });
}

async fn due_openai_accounts(db: &bella_db::DbPool) -> anyhow::Result<Vec<Uuid>> {
    let rows = sqlx::query(
        "select id
         from provider_accounts
         where provider = 'openai'
           and status = 'verified'
           and (next_sync_at is null or next_sync_at <= now())
         order by coalesce(last_synced_at, '-infinity'::timestamptz), created_at
         limit 10",
    )
    .fetch_all(db)
    .await?;

    Ok(rows.iter().map(|row| row.get("id")).collect())
}

fn required_env(name: &str) -> anyhow::Result<String> {
    let value = env::var(name)?;
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be empty");
    }
    Ok(value)
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

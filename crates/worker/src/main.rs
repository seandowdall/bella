mod incident_delivery;

use std::{env, time::Duration};

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
    let credential_cipher = bella_ingestion::credentials::CredentialCipher::from_base64(
        &required_env("BELLA_CREDENTIAL_ENCRYPTION_KEY")?,
    )?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let db = bella_db::connect(&database_url).await?;
    bella_db::run_migrations(&db).await?;
    let ingestor = bella_ingestion::openai::OpenAiIngestor::new(
        db.clone(),
        client.clone(),
        credential_cipher.clone(),
        openai_base_url,
    );
    let incident_delivery = incident_delivery::IncidentDeliveryWorker::new(
        db.clone(),
        client.clone(),
        credential_cipher,
    );

    tracing::info!(poll_interval, "bella-worker started");
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
        if let Err(error) = incident_delivery.process_due().await {
            tracing::warn!(%error, "incident delivery processing failed");
        }
        tokio::time::sleep(Duration::from_secs(poll_interval)).await;
    }
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

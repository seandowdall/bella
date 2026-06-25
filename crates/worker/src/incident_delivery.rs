use anyhow::Context;
use bella_ingestion::credentials::CredentialCipher;
use bella_slack::{
    IncidentSlackReport, SlackBotToken, SlackClientError, post_incident_opened_with_client,
};
use chrono::{DateTime, Utc};
use reqwest::Client;
use sqlx::Row;
use uuid::Uuid;

use bella_db::DbPool;

const SLACK_INCIDENT_OPENED: &str = "slack.incident_opened";
const CLAIM_LIMIT: i64 = 10;
const MAX_ATTEMPTS: i32 = 5;
const CLAIM_DUE_JOBS_SQL: &str = "with candidates as (
	             select id
	             from incident_delivery_jobs
	             where delivery_type = 'slack.incident_opened'
	               and (
	                    (status = 'pending' and available_at <= now())
	                    or (status = 'processing' and locked_at < now() - interval '10 minutes')
	               )
	             order by available_at, created_at
	             limit $1
             for update skip locked
         )
         update incident_delivery_jobs job
         set status = 'processing',
             attempts = job.attempts + 1,
             locked_at = now(),
             updated_at = now()
         from candidates
         where job.id = candidates.id
         returning job.id, job.organization_id, job.incident_id, job.delivery_type, job.attempts";

pub struct IncidentDeliveryWorker {
    db: DbPool,
    client: Client,
    credential_cipher: CredentialCipher,
}

impl IncidentDeliveryWorker {
    pub fn new(db: DbPool, client: Client, credential_cipher: CredentialCipher) -> Self {
        Self {
            db,
            client,
            credential_cipher,
        }
    }

    pub async fn process_due(&self) -> anyhow::Result<()> {
        let jobs = claim_due_jobs(&self.db).await?;
        for job in jobs {
            if let Err(error) = self.process_job(&job).await {
                tracing::warn!(job_id = %job.id, delivery_type = %job.delivery_type, %error, "incident delivery failed");
                reschedule_job(&self.db, &job, &error.to_string()).await?;
            } else {
                mark_delivered(&self.db, job.id).await?;
            }
        }
        Ok(())
    }

    async fn process_job(&self, job: &DeliveryJob) -> anyhow::Result<()> {
        match job.delivery_type.as_str() {
            SLACK_INCIDENT_OPENED => self.deliver_slack_incident_opened(job).await,
            delivery_type => anyhow::bail!("unsupported incident delivery type: {delivery_type}"),
        }
    }

    async fn deliver_slack_incident_opened(&self, job: &DeliveryJob) -> anyhow::Result<()> {
        let incident = load_incident(&self.db, job.organization_id, job.incident_id).await?;
        if incident.slack_thread_ts.is_some() {
            return Ok(());
        }

        let delivery_target = load_slack_delivery_target(&self.db, job.organization_id).await?;
        let plaintext = self
            .credential_cipher
            .decrypt(
                &delivery_target.credential_ciphertext,
                &delivery_target.credential_nonce,
            )
            .context("could not decrypt Slack bot token")?;
        let bot_token = SlackBotToken::new(
            String::from_utf8(plaintext).context("Slack bot token is not UTF-8")?,
        )?;

        let message = match post_incident_opened_with_client(
            &self.client,
            &bot_token,
            &delivery_target.slack_channel_id,
            &IncidentSlackReport {
                severity: incident.severity,
                source: incident.source,
                status: incident.status,
                detected_at: incident.detected_at,
            },
        )
        .await
        {
            Ok(message) => message,
            Err(error) => {
                if matches!(error, SlackClientError::TokenRevoked) {
                    mark_slack_installation_needs_attention(
                        &self.db,
                        delivery_target.slack_installation_id,
                        "token_revoked",
                    )
                    .await?;
                }
                return Err(slack_error(error));
            }
        };

        let result = sqlx::query(
            "update incidents
             set slack_channel_id = $1,
                 slack_thread_ts = $2,
                 updated_at = now()
             where id = $3
               and organization_id = $4
               and slack_thread_ts is null",
        )
        .bind(message.channel_id)
        .bind(message.message_ts)
        .bind(job.incident_id)
        .bind(job.organization_id)
        .execute(&self.db)
        .await?;
        if result.rows_affected() != 1 {
            anyhow::bail!("incident Slack thread was updated concurrently");
        }
        Ok(())
    }
}

struct DeliveryJob {
    id: Uuid,
    organization_id: Uuid,
    incident_id: Uuid,
    delivery_type: String,
    attempts: i32,
}

struct IncidentForDelivery {
    severity: String,
    source: String,
    status: String,
    detected_at: DateTime<Utc>,
    slack_thread_ts: Option<String>,
}

struct SlackDeliveryTarget {
    slack_installation_id: Uuid,
    credential_ciphertext: Vec<u8>,
    credential_nonce: Vec<u8>,
    slack_channel_id: String,
}

async fn claim_due_jobs(db: &DbPool) -> anyhow::Result<Vec<DeliveryJob>> {
    let mut transaction = db.begin().await?;
    let rows = sqlx::query(CLAIM_DUE_JOBS_SQL)
        .bind(CLAIM_LIMIT)
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;

    Ok(rows
        .into_iter()
        .map(|row| DeliveryJob {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
            incident_id: row.get("incident_id"),
            delivery_type: row.get("delivery_type"),
            attempts: row.get("attempts"),
        })
        .collect())
}

async fn load_slack_delivery_target(
    db: &DbPool,
    organization_id: Uuid,
) -> anyhow::Result<SlackDeliveryTarget> {
    let row = sqlx::query(
        "select si.id as slack_installation_id,
                c.credential_ciphertext,
                c.credential_nonce,
                dt.slack_channel_id
         from slack_installations si
         join integration_credentials c
           on c.integration_id = si.integration_id
          and c.kind = 'bot_token'
         join slack_delivery_targets dt
           on dt.slack_installation_id = si.id
          and dt.organization_id = si.organization_id
          and dt.status = 'active'
         where si.organization_id = $1
           and si.status = 'connected'
         order by dt.confirmed_at desc nulls last,
                  dt.last_seen_at desc,
                  dt.created_at desc
         limit 1",
    )
    .bind(organization_id)
    .fetch_optional(db)
    .await?
    .context("no active Slack delivery target is configured")?;

    Ok(SlackDeliveryTarget {
        slack_installation_id: row.get("slack_installation_id"),
        credential_ciphertext: row.get("credential_ciphertext"),
        credential_nonce: row.get("credential_nonce"),
        slack_channel_id: row.get("slack_channel_id"),
    })
}

async fn load_incident(
    db: &DbPool,
    organization_id: Uuid,
    incident_id: Uuid,
) -> anyhow::Result<IncidentForDelivery> {
    let row = sqlx::query(
        "select severity, source, status, detected_at, slack_thread_ts
         from incidents
         where id = $1 and organization_id = $2",
    )
    .bind(incident_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await?
    .context("incident no longer exists")?;

    Ok(IncidentForDelivery {
        severity: row.get("severity"),
        source: row.get("source"),
        status: row.get("status"),
        detected_at: row.get("detected_at"),
        slack_thread_ts: row.get("slack_thread_ts"),
    })
}

async fn mark_delivered(db: &DbPool, job_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "update incident_delivery_jobs
         set status = 'delivered',
             delivered_at = now(),
             locked_at = null,
             last_error = null,
             updated_at = now()
         where id = $1 and status = 'processing'",
    )
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn reschedule_job(db: &DbPool, job: &DeliveryJob, error: &str) -> anyhow::Result<()> {
    let failed = job.attempts >= MAX_ATTEMPTS;
    let delay_seconds = retry_delay_seconds(job.attempts);
    let error = error.chars().take(1_000).collect::<String>();

    sqlx::query(
        "update incident_delivery_jobs
         set status = case when $2 then 'failed' else 'pending' end,
             available_at = case when $2 then available_at else now() + ($3 * interval '1 second') end,
             locked_at = null,
             last_error = $4,
             updated_at = now()
         where id = $1 and status = 'processing'",
    )
    .bind(job.id)
    .bind(failed)
    .bind(delay_seconds)
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

fn retry_delay_seconds(attempts: i32) -> i32 {
    30 * 2_i32.pow(attempts.saturating_sub(1).min(5) as u32)
}

fn slack_error(error: SlackClientError) -> anyhow::Error {
    match error {
        SlackClientError::ChannelArchived => anyhow::anyhow!("Slack channel is archived"),
        SlackClientError::ChannelNotFound => anyhow::anyhow!("Slack channel was not found"),
        SlackClientError::MissingScope => {
            anyhow::anyhow!("Slack installation is missing a required scope")
        }
        SlackClientError::NotInChannel => {
            anyhow::anyhow!("Slack bot is not in the configured channel")
        }
        SlackClientError::RateLimited { .. } => {
            anyhow::anyhow!("Slack rate limited the incident message")
        }
        SlackClientError::Rejected { .. } => anyhow::anyhow!("Slack rejected the incident message"),
        SlackClientError::TokenRevoked => anyhow::anyhow!("Slack bot token is revoked or invalid"),
        SlackClientError::Unavailable => anyhow::anyhow!("Slack is unavailable"),
    }
}

async fn mark_slack_installation_needs_attention(
    db: &DbPool,
    slack_installation_id: Uuid,
    reason: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "update slack_installations
         set status = 'needs_attention',
             status_reason = $2,
             updated_at = now()
         where id = $1",
    )
    .bind(slack_installation_id)
    .bind(reason)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CLAIM_DUE_JOBS_SQL, retry_delay_seconds};

    #[test]
    fn claim_query_uses_second_bind_for_limit() {
        assert!(CLAIM_DUE_JOBS_SQL.contains("delivery_type = 'slack.incident_opened'"));
        assert!(CLAIM_DUE_JOBS_SQL.contains("limit $1"));
    }

    #[test]
    fn retries_with_bounded_exponential_backoff() {
        assert_eq!(retry_delay_seconds(1), 30);
        assert_eq!(retry_delay_seconds(2), 60);
        assert_eq!(retry_delay_seconds(6), 960);
        assert_eq!(retry_delay_seconds(20), 960);
    }
}

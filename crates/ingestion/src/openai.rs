use std::time::Duration as StdDuration;

use bella_db::DbPool;
use chrono::{DateTime, Duration, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{SyncOutcome, credentials::CredentialCipher};

const PROVIDER: &str = "openai";
const USAGE_ENDPOINT: &str = "/v1/organization/usage/completions";
const COSTS_ENDPOINT: &str = "/v1/organization/costs";

#[derive(Clone)]
pub struct OpenAiIngestor {
    db: DbPool,
    client: Client,
    cipher: CredentialCipher,
    base_url: String,
}

impl OpenAiIngestor {
    pub fn new(db: DbPool, client: Client, cipher: CredentialCipher, base_url: String) -> Self {
        Self {
            db,
            client,
            cipher,
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    pub async fn sync_account(&self, provider_account_id: Uuid) -> anyhow::Result<SyncOutcome> {
        let account = self.load_account(provider_account_id).await?;
        if account.provider != PROVIDER {
            anyhow::bail!("provider account is not openai");
        }

        let secret = self.decrypt_secret(&account)?;
        let now = Utc::now();
        let checkpoint = self.load_checkpoint(provider_account_id).await?;
        let window_start = checkpoint
            .map(|value| value - Duration::days(3))
            .unwrap_or_else(|| now - Duration::days(30));
        let window_end = now - Duration::minutes(5);
        if window_start >= window_end {
            anyhow::bail!("sync window is empty");
        }

        let sync_run_id = Uuid::new_v4();
        sqlx::query(
            "insert into provider_sync_runs
             (id, provider_account_id, provider, status, window_start, window_end)
             values ($1, $2, $3, 'running', $4, $5)",
        )
        .bind(sync_run_id)
        .bind(provider_account_id)
        .bind(PROVIDER)
        .bind(window_start)
        .bind(window_end)
        .execute(&self.db)
        .await?;

        match self
            .sync_window(
                provider_account_id,
                sync_run_id,
                &secret,
                window_start,
                window_end,
            )
            .await
        {
            Ok((usage_buckets, cost_snapshots)) => {
                self.mark_success(provider_account_id, sync_run_id, window_end)
                    .await?;
                Ok(SyncOutcome {
                    sync_run_id,
                    provider_account_id,
                    provider: PROVIDER.to_owned(),
                    window_start,
                    window_end,
                    usage_buckets,
                    cost_snapshots,
                })
            }
            Err(error) => {
                let message = error.to_string();
                self.mark_failure(provider_account_id, sync_run_id, &message)
                    .await?;
                Err(error)
            }
        }
    }

    async fn sync_window(
        &self,
        provider_account_id: Uuid,
        sync_run_id: Uuid,
        secret: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> anyhow::Result<(usize, usize)> {
        let usage_pages = self
            .fetch_pages(secret, USAGE_ENDPOINT, window_start, window_end)
            .await?;
        let mut usage_buckets = 0;
        for (cursor, payload) in usage_pages {
            let request = RawPayloadRequest {
                provider_account_id,
                sync_run_id,
                endpoint: USAGE_ENDPOINT,
                window_start,
                window_end,
                page_cursor: cursor,
            };
            let raw_payload_id = self.store_raw_payload(&request, &payload).await?;
            usage_buckets += self
                .upsert_usage_buckets(provider_account_id, raw_payload_id, &payload)
                .await?;
        }

        let cost_pages = self
            .fetch_pages(secret, COSTS_ENDPOINT, window_start, window_end)
            .await?;
        let mut cost_snapshots = 0;
        for (cursor, payload) in cost_pages {
            let request = RawPayloadRequest {
                provider_account_id,
                sync_run_id,
                endpoint: COSTS_ENDPOINT,
                window_start,
                window_end,
                page_cursor: cursor,
            };
            let raw_payload_id = self.store_raw_payload(&request, &payload).await?;
            cost_snapshots += self
                .upsert_cost_snapshots(provider_account_id, raw_payload_id, &payload)
                .await?;
        }

        Ok((usage_buckets, cost_snapshots))
    }

    async fn fetch_pages(
        &self,
        secret: &str,
        endpoint: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> anyhow::Result<Vec<(String, Value)>> {
        let mut pages = Vec::new();
        let mut page_cursor: Option<String> = None;

        loop {
            let payload = self
                .fetch_page(
                    secret,
                    endpoint,
                    window_start,
                    window_end,
                    page_cursor.as_deref(),
                )
                .await?;
            let cursor_key = page_cursor.clone().unwrap_or_default();
            page_cursor = payload
                .get("next_page")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let has_more = payload
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(page_cursor.is_some());
            pages.push((cursor_key, payload));
            if !has_more || page_cursor.is_none() {
                break;
            }
        }

        Ok(pages)
    }

    async fn fetch_page(
        &self,
        secret: &str,
        endpoint: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        page_cursor: Option<&str>,
    ) -> anyhow::Result<Value> {
        let url = format!("{}{}", self.base_url, endpoint);
        let start_time = window_start.timestamp().to_string();
        let end_time = window_end.timestamp().to_string();

        for attempt in 0..5 {
            let mut request = self.client.get(&url).bearer_auth(secret).query(&[
                ("start_time", start_time.as_str()),
                ("end_time", end_time.as_str()),
                ("bucket_width", "1d"),
                ("limit", "100"),
            ]);
            if endpoint == USAGE_ENDPOINT {
                request = request.query(&[
                    ("group_by[]", "model"),
                    ("group_by[]", "project_id"),
                    ("group_by[]", "user_id"),
                    ("group_by[]", "api_key_id"),
                ]);
            }
            if let Some(page) = page_cursor {
                request = request.query(&[("page", page)]);
            }

            let response = request.send().await?;
            let status = response.status();
            if status.is_success() {
                return response.json::<Value>().await.map_err(Into::into);
            }
            if !should_retry(status) || attempt == 4 {
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!(
                    "OpenAI {endpoint} returned HTTP {}: {body}",
                    status.as_u16()
                );
            }

            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(StdDuration::from_secs)
                .unwrap_or_else(|| StdDuration::from_millis(250 * 2_u64.pow(attempt)));
            tokio::time::sleep(retry_after).await;
        }

        unreachable!("retry loop always returns or bails")
    }

    async fn store_raw_payload(
        &self,
        request: &RawPayloadRequest,
        payload: &Value,
    ) -> anyhow::Result<Uuid> {
        let payload_bytes = serde_json::to_vec(payload)?;
        let payload_hash = format!("{:x}", Sha256::digest(&payload_bytes));
        let id = Uuid::new_v4();
        let row = sqlx::query(
            "insert into provider_raw_payloads
             (id, provider_account_id, sync_run_id, provider, endpoint,
              request_window_start, request_window_end, page_cursor, payload_hash, payload)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             on conflict (provider_account_id, endpoint, request_window_start, request_window_end, page_cursor, payload_hash)
             do update set payload = excluded.payload
             returning id",
        )
        .bind(id)
        .bind(request.provider_account_id)
        .bind(request.sync_run_id)
        .bind(PROVIDER)
        .bind(request.endpoint)
        .bind(request.window_start)
        .bind(request.window_end)
        .bind(&request.page_cursor)
        .bind(payload_hash)
        .bind(payload)
        .fetch_one(&self.db)
        .await?;
        Ok(row.get("id"))
    }

    async fn upsert_usage_buckets(
        &self,
        provider_account_id: Uuid,
        raw_payload_id: Uuid,
        payload: &Value,
    ) -> anyhow::Result<usize> {
        let mut count = 0;
        for bucket in payload
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (bucket_start, bucket_end) = bucket_bounds(bucket)?;
            for result in bucket
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                sqlx::query(
                    "insert into usage_buckets
                     (id, provider_account_id, provider, bucket_start, bucket_end, model,
                      project_external_id, user_external_id, api_key_external_id, operation,
                      input_tokens, output_tokens, request_count, raw_payload_id)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                     on conflict (provider_account_id, bucket_start, bucket_end, model,
                                  project_external_id, user_external_id, api_key_external_id, operation)
                     do update set input_tokens = excluded.input_tokens,
                                   output_tokens = excluded.output_tokens,
                                   request_count = excluded.request_count,
                                   raw_payload_id = excluded.raw_payload_id,
                                   updated_at = now()",
                )
                .bind(Uuid::new_v4())
                .bind(provider_account_id)
                .bind(PROVIDER)
                .bind(bucket_start)
                .bind(bucket_end)
                .bind(text_dim(result, "model"))
                .bind(text_dim(result, "project_id"))
                .bind(text_dim(result, "user_id"))
                .bind(text_dim(result, "api_key_id"))
                .bind(text_dim(result, "operation"))
                .bind(int_dim(result, &["input_tokens"]))
                .bind(int_dim(result, &["output_tokens"]))
                .bind(int_dim(result, &["num_model_requests", "requests"]))
                .bind(raw_payload_id)
                .execute(&self.db)
                .await?;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn upsert_cost_snapshots(
        &self,
        provider_account_id: Uuid,
        raw_payload_id: Uuid,
        payload: &Value,
    ) -> anyhow::Result<usize> {
        let mut count = 0;
        for bucket in payload
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (bucket_start, bucket_end) = bucket_bounds(bucket)?;
            for result in bucket
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let amount = result.get("amount").unwrap_or(&Value::Null);
                let amount_micros = amount_to_micros(amount)?;
                let currency = amount
                    .get("currency")
                    .and_then(Value::as_str)
                    .unwrap_or("usd")
                    .to_lowercase();
                sqlx::query(
                    "insert into cost_snapshots
                     (id, provider_account_id, provider, bucket_start, bucket_end, line_item,
                      model, project_external_id, amount_micros, currency, raw_payload_id)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                     on conflict (provider_account_id, bucket_start, bucket_end, line_item,
                                  model, project_external_id, currency)
                     do update set amount_micros = excluded.amount_micros,
                                   raw_payload_id = excluded.raw_payload_id,
                                   updated_at = now()",
                )
                .bind(Uuid::new_v4())
                .bind(provider_account_id)
                .bind(PROVIDER)
                .bind(bucket_start)
                .bind(bucket_end)
                .bind(text_dim(result, "line_item"))
                .bind(text_dim(result, "model"))
                .bind(text_dim(result, "project_id"))
                .bind(amount_micros)
                .bind(currency)
                .bind(raw_payload_id)
                .execute(&self.db)
                .await?;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn load_account(&self, provider_account_id: Uuid) -> anyhow::Result<ProviderAccount> {
        let row = sqlx::query(
            "select id, provider, credential_ciphertext, credential_nonce
             from provider_accounts
             where id = $1 and status = 'verified'",
        )
        .bind(provider_account_id)
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("verified provider account not found"))?;
        Ok(ProviderAccount {
            provider: row.get("provider"),
            credential_ciphertext: row.get("credential_ciphertext"),
            credential_nonce: row.get("credential_nonce"),
        })
    }

    fn decrypt_secret(&self, account: &ProviderAccount) -> anyhow::Result<String> {
        let plaintext = self
            .cipher
            .decrypt(&account.credential_ciphertext, &account.credential_nonce)?;
        let credentials: Value = serde_json::from_slice(&plaintext)?;
        credentials
            .get("secret")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("provider credential secret is missing"))
    }

    async fn load_checkpoint(
        &self,
        provider_account_id: Uuid,
    ) -> anyhow::Result<Option<DateTime<Utc>>> {
        let row = sqlx::query(
            "select min(checkpoint_at) as checkpoint_at
             from provider_sync_checkpoints
             where provider_account_id = $1 and stream in ('usage', 'costs')",
        )
        .bind(provider_account_id)
        .fetch_one(&self.db)
        .await?;
        Ok(row.get("checkpoint_at"))
    }

    async fn mark_success(
        &self,
        provider_account_id: Uuid,
        sync_run_id: Uuid,
        checkpoint_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "insert into provider_sync_checkpoints (provider_account_id, stream, checkpoint_at)
             values ($1, 'usage', $2), ($1, 'costs', $2)
             on conflict (provider_account_id, stream)
             do update set checkpoint_at = excluded.checkpoint_at, updated_at = now()",
        )
        .bind(provider_account_id)
        .bind(checkpoint_at)
        .execute(&self.db)
        .await?;
        sqlx::query(
            "update provider_sync_runs
             set status = 'succeeded', finished_at = now()
             where id = $1",
        )
        .bind(sync_run_id)
        .execute(&self.db)
        .await?;
        sqlx::query(
            "update provider_accounts
             set last_synced_at = now(), next_sync_at = now() + interval '6 hours',
                 last_sync_error = null, updated_at = now()
             where id = $1",
        )
        .bind(provider_account_id)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn mark_failure(
        &self,
        provider_account_id: Uuid,
        sync_run_id: Uuid,
        error: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "update provider_sync_runs
             set status = 'failed', error = $2, finished_at = now()
             where id = $1",
        )
        .bind(sync_run_id)
        .bind(error)
        .execute(&self.db)
        .await?;
        sqlx::query(
            "update provider_accounts
             set last_sync_error = $2, next_sync_at = now() + interval '30 minutes', updated_at = now()
             where id = $1",
        )
        .bind(provider_account_id)
        .bind(error)
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

struct ProviderAccount {
    provider: String,
    credential_ciphertext: Vec<u8>,
    credential_nonce: Vec<u8>,
}

struct RawPayloadRequest {
    provider_account_id: Uuid,
    sync_run_id: Uuid,
    endpoint: &'static str,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    page_cursor: String,
}

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn bucket_bounds(bucket: &Value) -> anyhow::Result<(DateTime<Utc>, DateTime<Utc>)> {
    let start = bucket
        .get("start_time")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("OpenAI bucket missing start_time"))?;
    let end = bucket
        .get("end_time")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("OpenAI bucket missing end_time"))?;
    Ok((timestamp(start)?, timestamp(end)?))
}

fn timestamp(seconds: i64) -> anyhow::Result<DateTime<Utc>> {
    Utc.timestamp_opt(seconds, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid provider timestamp {seconds}"))
}

fn text_dim(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn int_dim(value: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_i64))
        .sum()
}

fn amount_to_micros(amount: &Value) -> anyhow::Result<i64> {
    let value = amount
        .get("value")
        .ok_or_else(|| anyhow::anyhow!("OpenAI cost result missing amount.value"))?;
    let decimal = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => anyhow::bail!("OpenAI amount.value must be a number or string"),
    };
    decimal_to_micros(&decimal)
}

fn decimal_to_micros(value: &str) -> anyhow::Result<i64> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let value = value.trim_start_matches('-');
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole_micros = whole.parse::<i64>()? * 1_000_000;
    let mut fraction = fraction.chars().take(6).collect::<String>();
    while fraction.len() < 6 {
        fraction.push('0');
    }
    let fraction_micros = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>()?
    };
    let micros = whole_micros + fraction_micros;
    Ok(if negative { -micros } else { micros })
}

#[cfg(test)]
mod tests {
    use super::decimal_to_micros;

    #[test]
    fn converts_decimal_money_to_micros() {
        assert_eq!(decimal_to_micros("0").unwrap(), 0);
        assert_eq!(decimal_to_micros("1").unwrap(), 1_000_000);
        assert_eq!(decimal_to_micros("1.23").unwrap(), 1_230_000);
        assert_eq!(decimal_to_micros("0.0000019").unwrap(), 1);
        assert_eq!(decimal_to_micros("-0.50").unwrap(), -500_000);
    }
}

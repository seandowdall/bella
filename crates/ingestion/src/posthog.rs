use std::time::Duration as StdDuration;

use bella_db::DbPool;
use chrono::{DateTime, Duration, Utc};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::credentials::CredentialCipher;

const SOURCE: &str = "posthog";

#[derive(Debug, Serialize)]
pub struct PosthogConnectionCheck {
    pub ok: bool,
    pub integration_id: Uuid,
    pub posthog_host: String,
    pub posthog_project_id: String,
    pub observed_rows: usize,
}

#[derive(Debug, Serialize)]
pub struct PosthogSyncOutcome {
    pub sync_run_id: Uuid,
    pub integration_id: Uuid,
    pub organization_id: Uuid,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub signals_seen: usize,
    pub signals_upserted: usize,
    pub incident_candidates_created: usize,
}

#[derive(Clone)]
pub struct PosthogIngestor {
    db: DbPool,
    client: Client,
    cipher: CredentialCipher,
    enabled: bool,
}

impl PosthogIngestor {
    pub fn new(db: DbPool, client: Client, cipher: CredentialCipher, enabled: bool) -> Self {
        Self {
            db,
            client,
            cipher,
            enabled,
        }
    }

    pub async fn check_organization(
        &self,
        organization_id: Uuid,
    ) -> anyhow::Result<PosthogConnectionCheck> {
        let integration = self
            .load_api_integration(ApiIntegrationLookup::Organization(organization_id))
            .await?;
        let rows = self
            .query_posthog(
                &integration,
                "select event, timestamp from events order by timestamp desc limit 1",
            )
            .await?;

        Ok(PosthogConnectionCheck {
            ok: true,
            integration_id: integration.id,
            posthog_host: integration.posthog_host,
            posthog_project_id: integration.posthog_project_id,
            observed_rows: rows.len(),
        })
    }

    pub async fn sync_organization(
        &self,
        organization_id: Uuid,
    ) -> anyhow::Result<PosthogSyncOutcome> {
        let integration = self
            .load_api_integration(ApiIntegrationLookup::Organization(organization_id))
            .await?;
        self.sync_integration(integration.id).await
    }

    pub async fn sync_integration(
        &self,
        integration_id: Uuid,
    ) -> anyhow::Result<PosthogSyncOutcome> {
        if !self.enabled {
            anyhow::bail!("PostHog production ingestion is disabled");
        }

        let integration = self
            .load_api_integration(ApiIntegrationLookup::Integration(integration_id))
            .await?;
        let window_end = Utc::now() - Duration::minutes(2);
        let checkpoint = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "select last_synced_at from posthog_sync_checkpoints where integration_id = $1",
        )
        .bind(integration.id)
        .fetch_optional(&self.db)
        .await?
        .flatten();
        let window_start = checkpoint
            .map(|value| value - Duration::minutes(15))
            .unwrap_or_else(|| window_end - Duration::hours(6));
        if window_start >= window_end {
            anyhow::bail!("PostHog sync window is empty");
        }

        let natural_key = format!(
            "posthog:{}:{}:{}",
            integration.posthog_project_id,
            window_start.timestamp(),
            window_end.timestamp()
        );
        let run_id = Uuid::new_v4();
        let run = sqlx::query(
            "insert into posthog_sync_runs
             (id, organization_id, integration_id, natural_key, status, posthog_host,
              posthog_project_id, window_start, window_end)
             values ($1, $2, $3, $4, 'running', $5, $6, $7, $8)
             on conflict (organization_id, integration_id, natural_key)
             do update set status = 'running', error = null, updated_at = now()
             returning id",
        )
        .bind(run_id)
        .bind(integration.organization_id)
        .bind(integration.id)
        .bind(&natural_key)
        .bind(&integration.posthog_host)
        .bind(&integration.posthog_project_id)
        .bind(window_start)
        .bind(window_end)
        .fetch_one(&self.db)
        .await?;
        let run_id: Uuid = run.get("id");

        match self
            .sync_window(&integration, window_start, window_end)
            .await
        {
            Ok(outcome) => {
                sqlx::query(
                    "update posthog_sync_runs
                     set status = 'succeeded',
                         signals_seen = $2,
                         signals_upserted = $3,
                         incident_candidates_created = $4,
                         updated_at = now()
                     where id = $1",
                )
                .bind(run_id)
                .bind(outcome.signals_seen as i32)
                .bind(outcome.signals_upserted as i32)
                .bind(outcome.incident_candidates_created as i32)
                .execute(&self.db)
                .await?;
                sqlx::query(
                    "insert into posthog_sync_checkpoints (integration_id, last_synced_at)
                     values ($1, $2)
                     on conflict (integration_id)
                     do update set last_synced_at = excluded.last_synced_at, updated_at = now()",
                )
                .bind(integration.id)
                .bind(window_end)
                .execute(&self.db)
                .await?;

                Ok(PosthogSyncOutcome {
                    sync_run_id: run_id,
                    integration_id: integration.id,
                    organization_id: integration.organization_id,
                    window_start,
                    window_end,
                    signals_seen: outcome.signals_seen,
                    signals_upserted: outcome.signals_upserted,
                    incident_candidates_created: outcome.incident_candidates_created,
                })
            }
            Err(error) => {
                let message = error.to_string();
                sqlx::query(
                    "update posthog_sync_runs
                     set status = 'failed', error = $2, updated_at = now()
                     where id = $1",
                )
                .bind(run_id)
                .bind(&message)
                .execute(&self.db)
                .await?;
                Err(error)
            }
        }
    }

    pub async fn due_integrations(
        &self,
        interval: StdDuration,
        limit: i64,
    ) -> anyhow::Result<Vec<Uuid>> {
        if !self.enabled {
            return Ok(Vec::new());
        }

        let interval_seconds = i64::try_from(interval.as_secs()).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            "select i.id
             from integrations i
             join integration_credentials c on c.integration_id = i.id and c.kind = 'api_token'
             left join posthog_sync_checkpoints checkpoint on checkpoint.integration_id = i.id
             where i.integration_type = 'posthog'
               and i.status = 'connected'
               and i.metadata ? 'posthog_host'
               and i.metadata ? 'posthog_project_id'
               and (
                    checkpoint.last_synced_at is null
                    or checkpoint.last_synced_at <= now() - ($1::text::interval)
               )
             order by coalesce(checkpoint.last_synced_at, '-infinity'::timestamptz), i.created_at
             limit $2",
        )
        .bind(format!("{interval_seconds} seconds"))
        .bind(limit)
        .fetch_all(&self.db)
        .await?;

        Ok(rows.iter().map(|row| row.get("id")).collect())
    }

    async fn sync_window(
        &self,
        integration: &PosthogApiIntegration,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> anyhow::Result<SyncWindowOutcome> {
        let query = posthog_signal_query(window_start, window_end);
        let rows = self.query_posthog(integration, &query).await?;
        let mut signals_upserted = 0;
        let mut incident_candidates_created = 0;

        for row in &rows {
            let payload = row_to_payload(row, integration);
            let normalized = normalize_signal(&payload);
            let (inserted_signal, created_incident) = self
                .upsert_normalized_signal(integration, &payload, &normalized)
                .await?;
            if inserted_signal {
                signals_upserted += 1;
            }
            if created_incident {
                incident_candidates_created += 1;
            }
        }

        Ok(SyncWindowOutcome {
            signals_seen: rows.len(),
            signals_upserted,
            incident_candidates_created,
        })
    }

    async fn upsert_normalized_signal(
        &self,
        integration: &PosthogApiIntegration,
        payload: &Value,
        normalized: &NormalizedPosthogSignal,
    ) -> anyhow::Result<(bool, bool)> {
        let mut transaction = self.db.begin().await?;
        let incident_id = Uuid::new_v4();
        let incident = sqlx::query(
            "insert into incidents
             (id, organization_id, title, status, severity, source, fingerprint, detected_at, metadata)
             values ($1, $2, $3, 'triggered', $4, 'posthog', $5, $6, $7)
             on conflict (organization_id, source, fingerprint) where resolved_at is null
             do update set title = excluded.title,
                           severity = excluded.severity,
                           updated_at = now(),
                           metadata = incidents.metadata
                               || excluded.metadata
                               || jsonb_build_object(
                                   'source_evidence_count',
                                   coalesce((incidents.metadata->>'source_evidence_count')::integer, 1) + 1
                               )
             returning id, (xmax = 0) as inserted",
        )
        .bind(incident_id)
        .bind(integration.organization_id)
        .bind(&normalized.title)
        .bind(&normalized.severity)
        .bind(&normalized.fingerprint)
        .bind(normalized.detected_at)
        .bind(serde_json::json!({
            "first_source": SOURCE,
            "last_signal_type": normalized.signal_type,
            "entity_key": normalized.entity_key,
            "fingerprint_seed": normalized.fingerprint_seed,
            "external_url": normalized.external_url,
            "confidence": normalized.confidence,
            "service_hint": normalized.service_hint,
            "component_hint": normalized.component_hint,
            "owner_hint": normalized.owner_hint,
            "source_evidence_count": 1,
        }))
        .fetch_one(&mut *transaction)
        .await?;
        let incident_id: Uuid = incident.get("id");
        let created_incident: bool = incident.get("inserted");

        let signal = sqlx::query(
            "insert into signals
             (id, organization_id, integration_id, incident_id, source, signal_type, source_event_id,
              fingerprint, title, severity, payload, received_at)
             values ($1, $2, $3, $4, 'posthog', $5, $6, $7, $8, $9, $10, now())
             on conflict (organization_id, source, source_event_id) where source_event_id is not null
             do update set incident_id = excluded.incident_id,
                           title = excluded.title,
                           severity = excluded.severity,
                           payload = excluded.payload
             returning id, (xmax = 0) as inserted",
        )
        .bind(Uuid::new_v4())
        .bind(integration.organization_id)
        .bind(integration.id)
        .bind(incident_id)
        .bind(&normalized.signal_type)
        .bind(&normalized.source_event_id)
        .bind(&normalized.fingerprint)
        .bind(&normalized.title)
        .bind(&normalized.severity)
        .bind(payload)
        .fetch_one(&mut *transaction)
        .await?;
        let signal_id: Uuid = signal.get("id");
        let inserted_signal: bool = signal.get("inserted");

        if inserted_signal {
            sqlx::query(
                "insert into incident_events
                 (id, organization_id, incident_id, event_type, title, metadata)
                 values ($1, $2, $3, 'signal.received', $4, $5)",
            )
            .bind(Uuid::new_v4())
            .bind(integration.organization_id)
            .bind(incident_id)
            .bind(format!("PostHog signal received: {}", normalized.title))
            .bind(serde_json::json!({
                "signal_id": signal_id,
                "signal_type": normalized.signal_type,
                "source_event_id": normalized.source_event_id,
                "external_url": normalized.external_url,
            }))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;

        Ok((inserted_signal, created_incident))
    }

    async fn load_api_integration(
        &self,
        lookup: ApiIntegrationLookup,
    ) -> anyhow::Result<PosthogApiIntegration> {
        let (filter, id) = match lookup {
            ApiIntegrationLookup::Organization(id) => ("i.organization_id = $1", id),
            ApiIntegrationLookup::Integration(id) => ("i.id = $1", id),
        };
        let row = sqlx::query(&format!(
            "select i.id, i.organization_id, i.metadata, c.credential_ciphertext, c.credential_nonce
             from integrations i
             join integration_credentials c on c.integration_id = i.id and c.kind = 'api_token'
             where {filter}
               and i.integration_type = 'posthog'
               and i.status <> 'disabled'
             order by i.updated_at desc
             limit 1"
        ))
        .bind(id)
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("PostHog API ingestion is not configured"))?;
        let metadata: Value = row.get("metadata");
        let posthog_host = string_at(&metadata, &["posthog_host"])
            .ok_or_else(|| anyhow::anyhow!("PostHog host is not configured"))?
            .trim_end_matches('/')
            .to_owned();
        let posthog_project_id = string_at(&metadata, &["posthog_project_id"])
            .ok_or_else(|| anyhow::anyhow!("PostHog project ID is not configured"))?
            .to_owned();
        let ciphertext: Vec<u8> = row.get("credential_ciphertext");
        let nonce: Vec<u8> = row.get("credential_nonce");
        let api_token = String::from_utf8(self.cipher.decrypt(&ciphertext, &nonce)?)?;

        Ok(PosthogApiIntegration {
            id: row.get("id"),
            organization_id: row.get("organization_id"),
            posthog_host,
            posthog_project_id,
            api_token,
        })
    }

    async fn query_posthog(
        &self,
        integration: &PosthogApiIntegration,
        query: &str,
    ) -> anyhow::Result<Vec<Value>> {
        let url = format!(
            "{}/api/projects/{}/query",
            integration.posthog_host, integration.posthog_project_id
        );
        for attempt in 0..4 {
            let response = self
                .client
                .post(&url)
                .bearer_auth(&integration.api_token)
                .json(&serde_json::json!({
                    "query": {
                        "kind": "HogQLQuery",
                        "query": query,
                    },
                }))
                .send()
                .await?;
            let status = response.status();
            if status.is_success() {
                let payload = response.json::<Value>().await?;
                return Ok(payload
                    .get("results")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default());
            }
            if !should_retry_posthog(status) || attempt == 3 {
                let _ = response.text().await;
                anyhow::bail!("PostHog query returned HTTP {}", status.as_u16());
            }
            tokio::time::sleep(StdDuration::from_millis(250 * 2_u64.pow(attempt))).await;
        }
        unreachable!("retry loop always returns or bails")
    }
}

enum ApiIntegrationLookup {
    Organization(Uuid),
    Integration(Uuid),
}

struct PosthogApiIntegration {
    id: Uuid,
    organization_id: Uuid,
    posthog_host: String,
    posthog_project_id: String,
    api_token: String,
}

struct NormalizedPosthogSignal {
    signal_type: String,
    source_event_id: String,
    fingerprint: String,
    fingerprint_seed: String,
    title: String,
    severity: String,
    detected_at: DateTime<Utc>,
    external_url: Option<String>,
    entity_key: String,
    confidence: String,
    service_hint: Option<String>,
    component_hint: Option<String>,
    owner_hint: Option<String>,
}

struct SyncWindowOutcome {
    signals_seen: usize,
    signals_upserted: usize,
    incident_candidates_created: usize,
}

fn posthog_signal_query(window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> String {
    format!(
        "select uuid, event, timestamp, created_at, distinct_id, properties
         from events
         where timestamp >= parseDateTimeBestEffort('{}')
           and timestamp < parseDateTimeBestEffort('{}')
           and (
                event = '$exception'
                or lower(event) like '%error%'
                or lower(event) like '%exception%'
                or lower(event) like '%alert%'
                or lower(event) like '%anomal%'
                or lower(event) like '%deploy%'
                or lower(event) like '%deployment%'
                or lower(event) like '%change%'
                or lower(event) like '%feature flag%'
                or properties.$exception_type is not null
                or properties.$exception_fingerprint is not null
                or properties.alert_id is not null
                or properties.anomaly_id is not null
                or properties.feature_flag is not null
           )
         order by timestamp asc, uuid asc
         limit 500",
        window_start.to_rfc3339(),
        window_end.to_rfc3339()
    )
}

fn row_to_payload(row: &Value, integration: &PosthogApiIntegration) -> Value {
    let values = row.as_array().cloned().unwrap_or_default();
    let event_uuid = values.first().and_then(Value::as_str).unwrap_or_default();
    let event_name = values
        .get(1)
        .and_then(Value::as_str)
        .unwrap_or("posthog_event");
    let timestamp = values.get(2).and_then(Value::as_str);
    let created_at = values.get(3).and_then(Value::as_str);
    let distinct_id = values.get(4).and_then(Value::as_str);
    let properties = values
        .get(5)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut payload = serde_json::json!({
        "uuid": event_uuid,
        "event": event_name,
        "timestamp": timestamp,
        "created_at": created_at,
        "distinct_id_hash": distinct_id.map(stable_hash),
        "properties": sanitize_properties(&properties),
        "posthog_project_id": integration.posthog_project_id,
    });
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "external_url".to_string(),
            Value::String(format!(
                "{}/project/{}/events/{}",
                integration.posthog_host, integration.posthog_project_id, event_uuid
            )),
        );
    }
    payload
}

pub fn sanitize_webhook_payload(payload: &Value) -> Value {
    match payload {
        Value::Object(object) => {
            let mut sanitized = Map::new();
            for (key, value) in object {
                if key == "distinct_id" {
                    if let Some(distinct_id) = value.as_str() {
                        sanitized.insert(
                            "distinct_id_hash".to_string(),
                            Value::String(stable_hash(distinct_id)),
                        );
                    }
                } else if is_sensitive_property_key(key) {
                    sanitized.insert(key.clone(), Value::String("[redacted]".to_string()));
                } else {
                    sanitized.insert(key.clone(), sanitize_webhook_payload(value));
                }
            }
            Value::Object(sanitized)
        }
        Value::Array(values) => Value::Array(values.iter().map(sanitize_webhook_payload).collect()),
        Value::String(value) => Value::String(truncate(value, 1_000)),
        _ => payload.clone(),
    }
}

fn normalize_signal(payload: &Value) -> NormalizedPosthogSignal {
    let event_name = string_at(payload, &["event"]).unwrap_or("posthog_event");
    let event_lower = event_name.to_lowercase();
    let signal_type = if event_name == "$exception" {
        "posthog.exception_event"
    } else if string_at(payload, &["issue", "id"]).is_some()
        || string_at(payload, &["issue", "name"]).is_some()
    {
        "posthog.error_issue"
    } else if event_lower.contains("alert") {
        "posthog.alert"
    } else if event_lower.contains("anomal") {
        "posthog.anomaly"
    } else if event_lower.contains("deploy") || event_lower.contains("change") {
        "posthog.change_marker"
    } else if event_lower.contains("feature flag")
        || string_at(payload, &["properties", "feature_flag"]).is_some()
    {
        "posthog.feature_flag"
    } else {
        "posthog.webhook"
    }
    .to_string();

    let payload_hash = payload_hash(payload);
    let source_event_id = first_string(
        payload,
        &[
            &["uuid"],
            &["event_id"],
            &["id"],
            &["issue", "id"],
            &["properties", "$exception_id"],
        ],
    )
    .map(ToOwned::to_owned)
    .unwrap_or_else(|| payload_hash.clone());
    let fingerprint_seed = first_string(
        payload,
        &[
            &["properties", "$exception_fingerprint"],
            &["properties", "$exception_type"],
            &["properties", "alert_id"],
            &["properties", "anomaly_id"],
            &["properties", "deployment"],
            &["properties", "feature_flag"],
            &["properties", "service"],
            &["properties", "component"],
            &["issue", "id"],
            &["issue", "fingerprint"],
        ],
    )
    .map(|value| truncate(value, 240))
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| source_event_id.clone());
    let title = first_string(
        payload,
        &[
            &["issue", "name"],
            &["issue", "title"],
            &["title"],
            &["name"],
            &["properties", "$exception_message"],
            &["properties", "$exception_type"],
            &["event"],
        ],
    )
    .map(|value| truncate(value, 240))
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| "PostHog error signal".to_string());
    let severity = first_string(
        payload,
        &[&["severity"], &["level"], &["properties", "level"]],
    )
    .map(normalize_severity)
    .unwrap_or_else(|| "unknown".to_string());
    let detected_at = first_string(
        payload,
        &[
            &["timestamp"],
            &["created_at"],
            &["issue", "first_seen"],
            &["properties", "timestamp"],
        ],
    )
    .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
    .map(|value| value.with_timezone(&Utc))
    .unwrap_or_else(Utc::now);
    let external_url = first_string(payload, &[&["external_url"], &["url"], &["issue", "url"]])
        .map(|value| truncate(value, 500));
    let service_hint =
        first_string(payload, &[&["properties", "service"]]).map(|value| truncate(value, 120));
    let component_hint =
        first_string(payload, &[&["properties", "component"]]).map(|value| truncate(value, 120));
    let owner_hint = first_string(
        payload,
        &[
            &["properties", "owner"],
            &["properties", "team"],
            &["properties", "repository"],
        ],
    )
    .map(|value| truncate(value, 120));
    let entity_key = first_string(
        payload,
        &[
            &["properties", "service"],
            &["properties", "component"],
            &["properties", "$current_url"],
            &["distinct_id_hash"],
        ],
    )
    .map(|value| truncate(value, 120))
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| fingerprint_seed.clone());
    let project_id = string_at(payload, &["posthog_project_id"]).unwrap_or("unknown_project");
    let window_start = detected_at
        .timestamp()
        .div_euclid(3600)
        .saturating_mul(3600);
    let fingerprint = stable_hash(&format!(
        "{SOURCE}:{project_id}:{signal_type}:{entity_key}:{fingerprint_seed}:{window_start}"
    ));
    let confidence = if matches!(
        signal_type.as_str(),
        "posthog.exception_event" | "posthog.error_issue" | "posthog.alert"
    ) {
        "high"
    } else {
        "medium"
    }
    .to_string();

    NormalizedPosthogSignal {
        signal_type,
        source_event_id: truncate(&source_event_id, 160),
        fingerprint,
        fingerprint_seed,
        title,
        severity,
        detected_at,
        external_url,
        entity_key,
        confidence,
        service_hint,
        component_hint,
        owner_hint,
    }
}

fn sanitize_properties(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    if is_sensitive_property_key(key) {
                        (key.clone(), Value::String("[redacted]".to_string()))
                    } else {
                        (key.clone(), sanitize_properties(value))
                    }
                })
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_properties).collect()),
        Value::String(value) => Value::String(truncate(value, 1_000)),
        _ => value.clone(),
    }
}

fn is_sensitive_property_key(key: &str) -> bool {
    let key = key.to_lowercase();
    [
        "email", "name", "phone", "address", "ip", "token", "secret", "password", "distinct",
        "person", "user", "session",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn first_string<'a>(payload: &'a Value, paths: &[&[&str]]) -> Option<&'a str> {
    paths.iter().find_map(|path| string_at(payload, path))
}

fn string_at<'a>(payload: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = payload;
    for segment in path {
        current = current.get(*segment)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn payload_hash(payload: &Value) -> String {
    stable_hash(&payload.to_string())
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn truncate(value: &str, max_len: usize) -> String {
    value.trim().chars().take(max_len).collect()
}

fn normalize_severity(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "critical" | "fatal" | "panic" => "critical",
        "high" | "error" => "high",
        "medium" | "warning" | "warn" => "medium",
        "low" => "low",
        "info" | "debug" => "info",
        _ => "unknown",
    }
    .to_string()
}

fn should_retry_posthog(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;

    #[test]
    fn posthog_signal_query_includes_dogfood_signal_families() {
        let window_start = Utc.with_ymd_and_hms(2026, 6, 26, 10, 0, 0).unwrap();
        let window_end = Utc.with_ymd_and_hms(2026, 6, 26, 11, 0, 0).unwrap();
        let query = posthog_signal_query(window_start, window_end);

        for expected in [
            "event = '$exception'",
            "like '%error%'",
            "like '%alert%'",
            "like '%anomal%'",
            "like '%deploy%'",
            "like '%change%'",
            "like '%feature flag%'",
            "properties.alert_id",
            "properties.anomaly_id",
            "properties.feature_flag",
        ] {
            assert!(query.contains(expected), "query should include {expected}");
        }
    }

    #[test]
    fn row_to_payload_hashes_distinct_id_and_redacts_sensitive_properties() {
        let integration = test_integration();
        let row = json!([
            "evt_1",
            "$exception",
            "2026-06-26T10:15:00Z",
            "2026-06-26T10:15:01Z",
            "person@example.com",
            {
                "$exception_type": "TypeError",
                "service": "checkout",
                "user_email": "person@example.com",
                "nested": {
                    "session_id": "abc123",
                    "component": "cart"
                }
            }
        ]);

        let payload = row_to_payload(&row, &integration);

        assert!(payload.get("distinct_id").is_none());
        assert_eq!(
            payload.get("distinct_id_hash").and_then(Value::as_str),
            Some(stable_hash("person@example.com").as_str())
        );
        assert_eq!(
            string_at(&payload, &["properties", "user_email"]),
            Some("[redacted]")
        );
        assert_eq!(
            string_at(&payload, &["properties", "nested", "session_id"]),
            Some("[redacted]")
        );
        assert_eq!(
            string_at(&payload, &["properties", "nested", "component"]),
            Some("cart")
        );
    }

    #[test]
    fn webhook_payload_sanitizer_hashes_distinct_id_and_redacts_sensitive_fields() {
        let payload = json!({
            "uuid": "evt_1",
            "event": "$exception",
            "distinct_id": "person@example.com",
            "properties": {
                "$exception_type": "TypeError",
                "$exception_message": "Cannot read properties of undefined",
                "email": "person@example.com",
                "session_id": "abc123",
                "service": "checkout"
            },
            "person": {
                "name": "Jane Example"
            }
        });

        let sanitized = sanitize_webhook_payload(&payload);

        assert!(sanitized.get("distinct_id").is_none());
        assert_eq!(
            sanitized.get("distinct_id_hash").and_then(Value::as_str),
            Some(stable_hash("person@example.com").as_str())
        );
        assert_eq!(
            string_at(&sanitized, &["properties", "email"]),
            Some("[redacted]")
        );
        assert_eq!(
            string_at(&sanitized, &["properties", "session_id"]),
            Some("[redacted]")
        );
        assert_eq!(
            string_at(&sanitized, &["properties", "service"]),
            Some("checkout")
        );
        assert_eq!(string_at(&sanitized, &["person"]), Some("[redacted]"));
    }

    #[test]
    fn normalizes_exception_candidates_with_stable_hourly_fingerprints() {
        let payload = json!({
            "uuid": "evt_1",
            "event": "$exception",
            "timestamp": "2026-06-26T10:15:00Z",
            "posthog_project_id": "12345",
            "external_url": "https://us.posthog.com/project/12345/events/evt_1",
            "properties": {
                "$exception_type": "TypeError",
                "$exception_message": "Cannot read properties of undefined",
                "$exception_fingerprint": "type-error-cart",
                "service": "checkout",
                "component": "cart",
                "owner": "growth-eng",
                "level": "error"
            }
        });
        let same_candidate_payload = json!({
            "uuid": "evt_2",
            "event": "$exception",
            "timestamp": "2026-06-26T10:45:00Z",
            "posthog_project_id": "12345",
            "properties": {
                "$exception_type": "TypeError",
                "$exception_message": "Cannot read properties of undefined",
                "$exception_fingerprint": "type-error-cart",
                "service": "checkout",
                "component": "cart",
                "owner": "growth-eng",
                "level": "error"
            }
        });
        let next_window_payload = json!({
            "uuid": "evt_3",
            "event": "$exception",
            "timestamp": "2026-06-26T11:01:00Z",
            "posthog_project_id": "12345",
            "properties": {
                "$exception_fingerprint": "type-error-cart",
                "service": "checkout"
            }
        });

        let normalized = normalize_signal(&payload);
        let same_candidate = normalize_signal(&same_candidate_payload);
        let next_window = normalize_signal(&next_window_payload);

        assert_eq!(normalized.signal_type, "posthog.exception_event");
        assert_eq!(normalized.source_event_id, "evt_1");
        assert_eq!(normalized.severity, "high");
        assert_eq!(normalized.entity_key, "checkout");
        assert_eq!(normalized.fingerprint_seed, "type-error-cart");
        assert_eq!(normalized.confidence, "high");
        assert_eq!(normalized.service_hint.as_deref(), Some("checkout"));
        assert_eq!(normalized.component_hint.as_deref(), Some("cart"));
        assert_eq!(normalized.owner_hint.as_deref(), Some("growth-eng"));
        assert_eq!(normalized.fingerprint, same_candidate.fingerprint);
        assert_ne!(normalized.fingerprint, next_window.fingerprint);
    }

    #[test]
    fn classifies_posthog_alert_anomaly_change_and_feature_flag_signals() {
        let cases = [
            ("Alert fired", "posthog.alert"),
            ("Product anomaly detected", "posthog.anomaly"),
            ("Deployment finished", "posthog.change_marker"),
            ("Feature flag updated", "posthog.feature_flag"),
        ];

        for (event, signal_type) in cases {
            let payload = json!({
                "uuid": format!("evt-{event}"),
                "event": event,
                "timestamp": "2026-06-26T10:15:00Z",
                "posthog_project_id": "12345",
                "properties": {
                    "service": "web",
                    "level": "warning"
                }
            });

            let normalized = normalize_signal(&payload);

            assert_eq!(normalized.signal_type, signal_type);
            assert_eq!(normalized.severity, "medium");
        }
    }

    fn test_integration() -> PosthogApiIntegration {
        PosthogApiIntegration {
            id: Uuid::nil(),
            organization_id: Uuid::nil(),
            posthog_host: "https://us.posthog.com".to_string(),
            posthog_project_id: "12345".to_string(),
            api_token: "phx_test".to_string(),
        }
    }
}

# OpenAI Ingestion

Bella imports OpenAI usage and cost data through OpenAI organization APIs. This
path is the provider-reported billing layer: it is useful for cost visibility,
historical import, and reconciliation, but it is not true real-time tracing.

## Credential

Connect OpenAI with an admin credential that can read organization usage and
cost endpoints. Bella validates the credential before storing it.

The credential is encrypted with `BELLA_CREDENTIAL_ENCRYPTION_KEY` and is never
written to the local CLI credential file.

## Endpoints

Bella currently calls:

```text
GET /v1/organization/usage/completions
GET /v1/organization/costs
```

The adapter stores raw provider payloads for debugging and replay, then writes
normalized rows into:

```text
usage_buckets
cost_snapshots
provider_raw_payloads
provider_sync_runs
provider_sync_checkpoints
```

Costs are stored as integer micros, not floating-point dollars.

## Sync Cadence

After a verified OpenAI account is connected, Bella schedules it for immediate
background sync by setting `next_sync_at = now()`.

The worker polls due OpenAI accounts every `BELLA_WORKER_POLL_SECONDS` seconds
and imports data for each due account. The default worker poll interval is 60
seconds.

After a successful OpenAI sync, Bella schedules the next sync 15 minutes later.
After a failed sync, Bella retries 30 minutes later.

Each successful sync uses a rolling correction window:

```text
window_start = checkpoint_at - 3 days
window_end = now - 5 minutes
```

The overlap lets Bella absorb late provider corrections without duplicating rows.
Imports are idempotent through stable natural keys on usage buckets, cost
snapshots, and raw payloads.

## Freshness Limits

OpenAI provider data should be treated as provider-reported billing/usage data,
not live request telemetry.

Expected limitations:

- Data can be delayed by the provider.
- Current buckets can be incomplete.
- Prior buckets can be corrected later.
- Data is bucketed, not per-request.
- App-level context such as feature, trace ID, prompt version, or customer is not
  available unless Bella also receives SDK/event telemetry.

For real-time observability, Bella will need SDK/event ingestion. Provider
polling remains the billing truth and reconciliation layer.

## Worker Requirement

Self-hosted deployments must run the worker in addition to the API and web app.

```sh
just api
just worker
just web
```

Without the worker, connected provider accounts will validate and save, but
automatic scheduled imports will not run. The `Sync now` API/UI/CLI action is a
manual recovery and debugging control, not the primary product loop.

Manual sync from the CLI:

```sh
just cli providers sync \
  --organization <organization-id> \
  --account <provider-account-id>
```

## Sandbox

Run a local OpenAI-compatible mock for development:

```sh
just sandbox
just api-sandbox
just web
```

Use any non-empty fake key, for example:

```text
sandbox-openai-admin-key
```

The sandbox base URL is injected with:

```text
OPENAI_BASE_URL=http://127.0.0.1:4010/openai
```

Available sandbox scenarios include:

```sh
just sandbox happy-path
just sandbox pagination
just sandbox rate-limit-once
just sandbox server-error-once
just sandbox duplicate-buckets
just sandbox malformed-usage
just sandbox malformed-costs
just sandbox missing-optional-dimensions
```

## Adding Future Providers

Future provider adapters should follow the same shape:

```text
fetch provider pages
store raw payloads
normalize usage/cost rows
upsert by natural keys
advance checkpoints only after full success
persist actionable sync errors on failure
```

The dashboard and reporting APIs should continue to read normalized tables rather
than provider-specific payloads.

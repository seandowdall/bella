# PostHog Live Incident Dogfood

Bella supports two PostHog ingestion paths for incident dogfooding:

- inbound webhooks for alert/error notifications
- read-only API sync for recent production signals

The read-only sync uses PostHog's private `/api/projects/:project_id/query`
endpoint with a HogQL query. The Events API is intentionally not used because
PostHog marks it deprecated for export-like workflows; Bella keeps the first
dogfood sync scoped to a small rolling window.

## Scope

The production sync currently reads recent events that can become incident
evidence:

- `$exception` events
- events whose name contains `error`, `exception`, `alert`, `anomal`,
  `deploy`, `deployment`, `change`, or `feature flag`
- events with `$exception_type`, `$exception_fingerprint`, `alert_id`,
  `anomaly_id`, or `feature_flag` properties

Bella stores normalized incident evidence, source IDs, timestamps, PostHog
backlinks, and a scrubbed event payload. `distinct_id` is stored as a stable
hash, and property keys that look like emails, names, IPs, tokens, secrets,
users, people, sessions, or addresses are redacted before persistence. Do not
configure queries that copy prompts, completions, API keys, or raw user PII into
Bella. Prefer stable external IDs, service/component properties, and PostHog
links.

## Credentials

Create a PostHog personal API key with `query:read` access to the production
project used for Bella dogfooding.

Configure Bella through the Integrations page:

1. Open Bella at `http://127.0.0.1:5173/integrations`.
2. Enter the PostHog host, project ID, and read-only API token.
3. Save the connection.
4. Use **Check connection** for a non-mutating PostHog query.
5. Use **Sync now** to ingest the current bounded window.

Or configure it through the CLI:

```sh
printf '%s' "$POSTHOG_API_TOKEN" | just cli integrations posthog connect \
  --organization <organization-id> \
  --name "PostHog Production" \
  --host https://us.posthog.com \
  --project-id <posthog-project-id> \
  --api-token-stdin
```

For EU Cloud, use `https://eu.posthog.com`. For self-hosted PostHog, use the
self-hosted app origin and add that exact HTTPS origin to:

```text
BELLA_ALLOWED_POSTHOG_ORIGINS=https://self-hosted-posthog.example.com
```

The same command also rotates Bella's inbound webhook secret and prints the
webhook URL/auth header. Store the secret immediately; Bella only shows it once.
Only organization owners and admins can connect PostHog, check the API
connection, or run manual syncs.

## Non-mutating Check

Verify the API connection without writing incidents:

```sh
just cli integrations posthog check --organization <organization-id>
```

This runs a one-row query against PostHog and returns the configured host,
project ID, and observed row count.

## Sync

Run a bounded sync:

```sh
just cli integrations posthog sync --organization <organization-id>
```

The first sync reads a six-hour window ending two minutes ago. Later syncs start
from the saved checkpoint with a fifteen-minute overlap so late-arriving events
are retried. Sync runs are stored with deterministic natural keys and repeated
runs are idempotent at the signal level.

Each normalized signal is deduped by `(organization, source, source_event_id)`
and grouped into an open incident candidate by `(organization, source,
fingerprint)`.

The background worker also syncs configured PostHog integrations automatically:

```sh
just worker
```

The worker considers due PostHog integrations on each
`BELLA_WORKER_POLL_SECONDS` loop. The default PostHog sync interval is five
minutes. Override it with:

```text
BELLA_POSTHOG_SYNC_INTERVAL_SECONDS=300
```

For container deploys that use the shared Bella image, run the worker with:

```text
BELLA_PROCESS=bella-worker
```

The worker serves `GET /health` on `PORT` so platforms such as Railway can
health-check the worker process separately from the API.

## Incident Lifecycle

PostHog signals create open incident candidates with `triggered` status. From
the incident detail page or API, responders can move a candidate through:

```text
acknowledged
investigating
mitigated
resolved
follow_up
```

Setting an incident to `resolved` records `resolved_at`; moving it back to any
non-resolved state clears `resolved_at`. Each status change appends an
`incident.status_changed` timeline event with the previous status, next status,
and actor user ID.

## Kill Switch

Disable production PostHog ingestion without deleting credentials:

```text
BELLA_POSTHOG_INGESTION_ENABLED=false
```

When disabled, inbound PostHog webhooks and manual/worker PostHog syncs stop
with a service-unavailable response. Connection checks remain available so the
stored configuration can still be inspected and repaired.

## Security Notes

PostHog API sync only accepts configured HTTPS origins. Bella allows PostHog
Cloud origins by default and rejects localhost, private IPs, link-local IPs, and
unconfigured hosts in production.

Send webhook secrets with `Authorization: Bearer`, `X-Bella-Webhook-Secret`, or
`X-PostHog-Webhook-Secret`. Query-string webhook secrets are rejected because
URLs are commonly captured by logs and proxies.

Use per-integration webhook secrets generated by Bella. A global
`POSTHOG_WEBHOOK_SECRET` is disabled by default outside local development; set
`BELLA_ALLOW_GLOBAL_POSTHOG_WEBHOOK_SECRET=true` only for a deliberate
compatibility window.

## PostHog-Side Setup Needed

Someone with access to the production PostHog project must provide:

- the PostHog app host
- the project ID
- a personal API key scoped to `query:read`
- confirmation that the scoped event families above are acceptable for dogfood
  privacy boundaries

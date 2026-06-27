# Deploy Slack Cloud to QA

This runbook enables Bella's Slack Cloud flow in an isolated QA environment.
It covers the repository configuration needed after `feat/slack-cloud` is
deployed. Use a separate Slack app, database, encryption key, and credentials
for production.

See [Incident delivery architecture](../architecture/incident-delivery.md) for
the rationale, guarantees, and trade-offs behind the PostgreSQL queue and
Railway-hosted worker.

## Supported QA Flow

The current implementation supports:

1. An organization owner or admin selects **Install Bella in Slack**.
2. Bella creates a short-lived, single-use OAuth state bound to that user,
   organization, and browser.
3. Slack redirects to Bella after authorization.
4. Bella encrypts the workspace bot token and stores the installation.
5. A user runs `/invite @Bella QA` in a channel.
6. Slack sends a signed `member_joined_channel` event and Bella records the
   channel as a delivery target.

Each Slack workspace can be connected to only one Bella organization for a
given Slack app. Slack events identify the workspace and app but do not carry a
Bella organization ID, so Bella rejects attempts to connect the same workspace
to another organization instead of guessing tenant ownership.
7. A valid PostHog webhook creates an incident and durable delivery job.
8. The worker posts the incident root message and stores its Slack thread
   timestamp.

Bella does not currently send a confirmation message immediately after the
channel invite or post investigation follow-ups into the thread.

## 1. Establish Public QA URLs

Choose one immutable HTTPS web origin and one immutable HTTPS API origin. This
guide uses:

```text
Web: https://app.qa.example.com
API: https://api.qa.example.com
```

The following routes must be publicly reachable:

```text
GET  https://api.qa.example.com/health
GET  https://app.qa.example.com/api/v1/slack/oauth/callback
POST https://api.qa.example.com/v1/slack/events
POST https://api.qa.example.com/v1/organizations/:organization_id/webhooks/posthog
```

The OAuth callback uses the Vercel `/api` rewrite so the HTTP-only browser nonce
set when installation starts is available when Slack returns. Do not put that
route behind browser authentication. Slack request signatures protect the
Events API route, OAuth state and the browser nonce protect the callback, and
organization-specific secrets protect PostHog webhooks.

## 2. Deploy the QA Services

QA requires four independently persistent components:

- PostgreSQL
- `bella-api`
- `bella-worker`
- `apps/web`

The root `Dockerfile` and `railway.toml` build and run `bella-api`. Create a
separate Railway service from the same commit using:

```text
Config file: /railway.worker.toml
```

That configuration builds `Dockerfile.worker` and runs the release
`bella-worker` binary. The worker must stay running; the API queues Slack
deliveries but does not send queued incident messages itself.

Both Rust services run embedded SQL migrations at startup. Deploy the API
before accepting traffic and confirm all migrations, including
`20260624000000_slack_cloud.sql`, have completed.

## 3. Configure Shared Service Secrets

Set these on both `bella-api` and `bella-worker`:

```env
DATABASE_URL=postgres://...
BELLA_CREDENTIAL_ENCRYPTION_KEY=...
```

Generate the encryption key once:

```sh
openssl rand -base64 32
```

The API encrypts Slack bot tokens and the worker decrypts them. The values must
match exactly and must remain stable across deployments. Store them in the QA
secret manager, never in Git or image layers.

The worker also needs:

```env
BELLA_WORKER_POLL_SECONDS=10
```

Ten seconds is useful for QA feedback. Production can use a longer interval.

## 4. Create the QA Slack App

Copy:

```text
deploy/slack/cloud-app-manifest.example.yaml
```

Replace `app.qa.example.com` and `api.qa.example.com` with the exact public QA
web and API hostnames. Import the resulting manifest into a new Slack app
attached to the QA workspace.

Keep these settings:

```text
Bot scopes: chat:write, channels:read, groups:read
OAuth redirect: https://app.qa.example.com/api/v1/slack/oauth/callback
Events URL: https://api.qa.example.com/v1/slack/events
Bot events: app_uninstalled, member_joined_channel, tokens_revoked
Socket Mode: disabled
Token rotation: disabled
```

Token rotation must remain disabled until Bella stores and refreshes Slack
refresh tokens.

When Slack verifies the Events API URL, the QA API must already be deployed
with `SLACK_SIGNING_SECRET`. A failed verification usually means the URL is not
public, TLS is invalid, the signing secret is wrong, or the request reached a
proxy route that did not preserve the request body.

## 5. Configure the QA API

Get the client ID, client secret, and signing secret from the QA Slack app.
Store them as API service secrets:

```env
BELLA_API_BIND_ADDR=0.0.0.0:3000
BELLA_PUBLIC_API_URL=https://api.qa.example.com
BELLA_WEB_URL=https://app.qa.example.com
BELLA_ALLOWED_ORIGINS=https://app.qa.example.com
BELLA_SECURE_COOKIES=true

SLACK_CLIENT_ID=...
SLACK_CLIENT_SECRET=...
SLACK_SIGNING_SECRET=...
SLACK_REDIRECT_URI=https://app.qa.example.com/api/v1/slack/oauth/callback
```

All four `SLACK_*` values are an all-or-nothing group. The API rejects partial
configuration. `SLACK_REDIRECT_URI` must exactly match the Slack app setting,
including its scheme, hostname, path, and trailing-slash behavior.

Do not set the self-hosted singleton variables for the Cloud path:

```text
BELLA_SLACK_BOT_TOKEN
BELLA_SLACK_DEFAULT_CHANNEL_ID
BELLA_SLACK_ORGANIZATION_ID
```

## 6. Configure the QA Web Build

Set these before building `apps/web`:

```env
NEXT_PUBLIC_BELLA_API_BASE_URL=/api
NEXT_PUBLIC_BELLA_PUBLIC_API_URL=https://api.qa.example.com
```

Configure the web host to proxy `/api/:path*` to the QA API with the `/api`
prefix removed. The existing Next.js rewrite performs this when
`BELLA_API_ORIGIN` is available to the Next.js server:

```env
BELLA_API_ORIGIN=https://api.qa.example.com
```

`NEXT_PUBLIC_*` values are public build-time configuration. Slack secrets must
never be set on the web service or exposed with a `NEXT_PUBLIC_` prefix.

## 7. Validate the Deployment

Check health and TLS:

```sh
curl --fail --show-error https://api.qa.example.com/health
curl --fail --show-error https://app.qa.example.com/
```

Confirm the API and worker are using the same database and encryption key.
Check API logs for migration or Slack configuration errors without logging the
secret values.

Then test the user flow:

1. Log into the QA web app.
2. Select a QA organization where the user is an owner or admin.
3. Open **Integrations** and select **Install Bella in Slack**.
4. Approve installation in the QA Slack workspace.
5. Confirm Bella returns to Integrations and shows the workspace as connected.
6. In a QA Slack channel, run `/invite @Bella QA`.
7. Refresh Integrations and confirm the channel appears.
8. Connect PostHog, save the generated secret, and configure a PostHog HTTP
   webhook.
9. Trigger a test incident.
10. Confirm the incident appears in Bella and a root message appears in Slack.

## 8. Inspect QA State

Use read-only queries and avoid selecting encrypted credential columns:

```sql
select slack_team_name, status, status_reason, installed_at
from slack_installations
order by installed_at desc;

select slack_channel_name, slack_channel_id, status, discovered_by
from slack_delivery_targets
order by created_at desc;

select delivery_type, status, attempts, left(coalesce(last_error, ''), 200)
from incident_delivery_jobs
order by created_at desc
limit 20;

select incident_id, slack_channel_id, slack_thread_ts
from incident_slack_threads
order by created_at desc
limit 20;
```

## 9. Failure Recovery

- **Slack Cloud installation is not configured:** set all four `SLACK_*`
  variables on the API and redeploy it.
- **redirect_uri did not match:** make the Slack OAuth redirect and
  `SLACK_REDIRECT_URI` identical.
- **Slack cannot verify the Events URL:** confirm public routing, raw request
  body preservation, signing secret, and API logs.
- **Workspace connects but no channel appears:** confirm
  `member_joined_channel` is subscribed and reinstall the app if scopes changed.
- **Incident exists but no Slack message appears:** confirm the worker is
  running and inspect `incident_delivery_jobs`.
- **Token cannot be decrypted:** restore the encryption key used when the
  installation was created, or reinstall Slack after intentionally rotating
  the key.

## 10. QA Exit Criteria

QA is ready for production hardening only when:

- OAuth succeeds from a clean browser session.
- Repeated OAuth callbacks cannot reuse the same state.
- `/invite` discovers both a public and a private test channel.
- Duplicate Slack events do not duplicate delivery targets.
- Duplicate PostHog deliveries do not duplicate incidents or Slack roots.
- A stopped worker resumes pending jobs after restart.
- Uninstalling the app marks the installation unavailable.
- No Slack tokens or signing secrets appear in API responses, browser state,
  deployment logs, or error tracking.

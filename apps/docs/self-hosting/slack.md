# Configure Slack for Self-Hosting

Bella's initial self-hosted Slack integration sends incident reports and
investigation updates to a channel you choose. It is outbound-only: Bella does
not read Slack history, respond to Slack messages, or require a public Slack
callback URL.

Each self-hosted operator creates and owns their own Slack app. Bella provides
a reusable manifest so setup is consistent and does not require manually
configuring scopes.

Bella Cloud will use a different setup path. Cloud users should not create a
Slack app, copy bot tokens, or paste channel IDs into Bella. The goal is a
single guided installation from the Bella dashboard:

1. The user logs into Bella and opens **Integrations**.
2. The user selects **Install Bella**.
3. Bella redirects the user to Slack's app installation flow.
4. The user approves the Bella Slack app for their workspace.
5. Slack redirects back to Bella and Bella marks the workspace as connected.
6. The user invites Bella to the incident channel:

   ```text
   /invite @Bella
   ```

7. Bella detects the invited channel, sends a confirmation message, and uses
   that channel for future incident threads.

This keeps the customer-facing setup to Slack authorization plus a normal
channel invite. It also keeps production Slack credentials out of the dashboard,
logs, docs, and repository.

## Bella Cloud Slack Plan

The cloud integration should be implemented as a separate credential model from
the self-hosted environment variables described below.

### Product Flow

- **Install from Bella:** Organization owners and admins can start the Slack
  install flow from the Bella web app.
- **Authorize in Slack:** Slack handles workspace selection and administrator
  approval.
- **Return to Bella:** Bella records the connected workspace and shows the next
  action.
- **Invite Bella:** The user invites Bella to each incident channel where it
  should post.
- **Confirm delivery:** Bella posts a short confirmation message in newly
  connected channels.
- **Connect PostHog:** The user creates or rotates the PostHog webhook secret in
  Bella and configures it in PostHog.
- **Create incident threads:** New PostHog incidents create a root Slack message
  and Bella adds follow-up context in the Slack thread as investigation jobs
  complete.

### Backend Shape

- Store one Slack installation per Bella organization and Slack workspace.
- Store Slack workspace metadata, granted scopes, bot identity, installation
  status, and the Bella user who installed it.
- Store Slack bot credentials encrypted at rest. Do not expose credentials
  through API responses, UI state, logs, CLI output, or documentation.
- Store delivery targets separately from installations. A delivery target is a
  Slack channel Bella has been invited to and is allowed to post in.
- Store incident Slack threads separately from incidents so future versions can
  support more than one target channel per incident.
- Replace the singleton environment-based Slack client in cloud paths with a
  per-installation client loaded for each delivery job.
- Keep delivery jobs durable and retryable. Failed Slack sends should update job
  state without dropping the incident or losing the thread mapping.

### Slack App Configuration

The first cloud app should request the minimum bot permissions needed for this
workflow:

```text
chat:write
channels:read
groups:read
```

Do not request channel history or broad posting permissions for the initial
cloud release. Requiring `/invite @Bella` is an intentional consent boundary:
Bella only posts where the workspace has explicitly added the bot.

The cloud app needs:

- OAuth redirect handling for installation.
- Signed Slack request verification for inbound Slack callbacks.
- Event handling for app uninstall or token revocation.
- Channel discovery when Bella is invited to a channel, with a fallback refresh
  path from the dashboard.

Cloud deployments must provide the Slack app client ID, client secret, signing
secret, and redirect URI through the deployment secret manager. These values are
an all-or-nothing configuration group: Bella should either start with Slack
Cloud disabled or with the complete Slack app configuration. The redirect URI
must be HTTPS outside local development and must not include query strings or
fragments.

### Delivery Behavior

When PostHog sends a valid webhook:

1. Bella normalizes the signal and upserts the open incident by organization,
   source, and fingerprint.
2. Bella stores the raw signal and an incident event.
3. If this is a new incident, Bella enqueues a Slack root-message delivery job.
4. The worker loads active Slack delivery targets for the organization.
5. The worker posts the root message to each active target and stores the Slack
   channel ID and thread timestamp.
6. Later investigation jobs post evidence, summaries, and handoff context into
   the stored thread.

The first production version can post one root message plus deterministic status
updates. Richer agent-generated context can be added behind the same thread job
model once the investigation pipeline is ready.

### Implementation Order

1. Fix the current incident delivery worker claim query before building on it.
2. Add database tables for Slack installations, delivery targets, OAuth state,
   inbound Slack event idempotency, and incident thread mappings.
3. Add Slack OAuth start and callback API routes.
4. Add Slack event ingestion with signature verification and idempotent event
   handling.
5. Add cloud Slack integration status to the web app.
6. Update the worker to resolve Slack credentials and targets from the database.
7. Add test-message support for database-backed cloud installations.
8. Add PostHog-to-Slack end-to-end tests using fake Slack responses.
9. Keep the self-hosted environment-based path working until there is a clear
   migration path for self-hosted operators.

### Security Notes

- Treat Slack OAuth state, inbound Slack callbacks, and PostHog webhooks as
  untrusted input.
- Verify Slack callbacks before parsing business actions.
- Use short-lived install state and bind it to the Bella organization and user
  that initiated installation.
- Encrypt stored Slack credentials with Bella's existing credential encryption
  path.
- Never include raw Slack credentials in structured logs, error responses,
  screenshots, docs, or client-rendered data.
- Store external event IDs and delivery dedupe keys so Slack retries and Bella
  worker retries are idempotent.
- Mark installations as needing attention when Slack reports revocation,
  uninstall, missing scope, channel removal, or posting failures that require
  user action.

## What You Need

- A running Bella API, worker, and Postgres database.
- A Slack workspace where you can create and install apps.
- A Bella organization ID for the organization that should deliver incidents.
- Permission to invite a bot to the incident channel.

The API and worker must use the same Slack environment configuration.

## 1. Find the Bella Organization ID

Open **Integrations** in Bella and copy the PostHog webhook URL. The UUID after
`/organizations/` is the organization ID:

```text
https://bella.example.com/api/v1/organizations/<organization-id>/webhooks/posthog
```

Use that value for `BELLA_SLACK_ORGANIZATION_ID`. The initial self-hosted setup
deliberately permits a bot to deliver incidents for one Bella organization only.

## 2. Create the Slack App from Bella's Manifest

1. Open [Slack API - Your Apps](https://api.slack.com/apps).
2. Select **Create New App**.
3. Select **From an app manifest**.
4. Pick the Slack workspace where Bella will be installed.
5. Paste [`deploy/slack/app-manifest.yaml`](../../../deploy/slack/app-manifest.yaml).
6. Review the configuration and select **Create**.

The manifest creates a `Bella` bot with only these bot scopes:

```text
chat:write
channels:read
groups:read
```

It intentionally does not enable event subscriptions, interactive components,
slash commands, Socket Mode, or Slack OAuth redirects.

## 3. Install the App and Invite the Bot

1. In the Slack app configuration, open **OAuth & Permissions**.
2. Select **Install to Workspace** and approve the listed bot scopes.
3. Copy the **Bot User OAuth Token**. It begins with `xoxb-`.
4. In Slack, create or choose an incident channel, such as `#incidents`.
5. Invite the bot:

   ```text
   /invite @Bella
   ```

For a private channel, the bot must be invited before it can post there.

## 4. Find the Slack Channel ID

Open the selected channel, select its name, open **About**, and copy the
channel ID from the bottom of the details panel. It normally begins with `C`.

## 5. Configure Bella

Set these values in the environment for **both** the Bella API and worker:

```env
BELLA_SLACK_BOT_TOKEN=xoxb-...
BELLA_SLACK_DEFAULT_CHANNEL_ID=C0123456789
BELLA_SLACK_ORGANIZATION_ID=<organization-id>
```

Keep the bot token in your deployment secret manager or private `.env` file.
Never add it to `app-manifest.yaml`, commit it to source control, or expose it
to the dashboard.

Restart both processes after changing configuration:

```sh
just api
just worker
```

For local development, use separate terminals and optionally set:

```env
BELLA_WORKER_POLL_SECONDS=5
```

## 6. Send a Test Message

In Bella, open **Settings > Integrations** and select **Send test message**.
Only Bella organization owners and admins can use this action.

Slack should receive:

```text
Bella Slack integration is connected.
```

## 7. Connect PostHog and Test an Incident

Open Bella **Integrations** and select **Connect PostHog**. Copy the generated
webhook URL and secret into a PostHog HTTP webhook destination. PostHog must be
able to reach Bella's public API URL; `127.0.0.1` only works for local manual
testing.

To verify the complete path before configuring PostHog, send a PostHog-shaped
signal to the generated webhook URL. Paste the secret only when prompted:

```sh
read -s BELLA_WEBHOOK_SECRET
EVENT_ID="$(uuidgen | tr '[:upper:]' '[:lower:]')"

curl -i -X POST "<bella-webhook-url>" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $BELLA_WEBHOOK_SECRET" \
  -d "{
    \"event\": \"\$exception\",
    \"uuid\": \"$EVENT_ID\",
    \"properties\": {
      \"\$exception_type\": \"CheckoutError\",
      \"\$exception_fingerprint\": \"$EVENT_ID\",
      \"level\": \"error\"
    }
  }"

unset BELLA_WEBHOOK_SECRET
```

Expected result:

```text
HTTP/1.1 201 Created
```

Bella creates an incident, records a durable Slack delivery job, and the
worker posts a root incident message to the configured channel. The initial
message intentionally omits raw exception text; open the Bella incident for
full details.

Bella limits new Slack incident roots to 20 per organization in five minutes
to reduce notification flooding if a webhook secret is compromised.

## Troubleshooting

### `Slack integration is not configured`

Set all three variables and restart the API and worker:

```text
BELLA_SLACK_BOT_TOKEN
BELLA_SLACK_DEFAULT_CHANNEL_ID
BELLA_SLACK_ORGANIZATION_ID
```

### The bot is not in the channel

Invite `@Bella` to the selected channel, then send the test message again.

### A Bella incident exists but no Slack message arrives

Confirm the worker is running and that `BELLA_SLACK_ORGANIZATION_ID` matches
the incident's organization. Check worker logs for incident delivery failures.

### PostHog cannot reach Bella

Deploy Bella behind a public HTTPS URL, or use a temporary HTTPS tunnel during
development. Configure Bella's generated webhook URL in PostHog, not an
internal container hostname.

## Current Limits

This is the self-hosted outbound Slack MVP. The following are planned but not
available yet:

- Guided token entry and channel selection in the Bella dashboard.
- Per-organization encrypted Slack installations stored in Bella.
- Slack Cloud OAuth installation.
- Slack Cloud channel discovery from `/invite @Bella`.
- Slack thread actions, commands, and richer inbound events.

The current environment-based setup is deliberately simple and deterministic
for one self-hosted organization. Cloud OAuth will use a separate,
per-organization credential model.

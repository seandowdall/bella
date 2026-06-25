# Configure Slack for Self-Hosting

Bella's initial self-hosted Slack integration sends incident reports and
investigation updates to a channel you choose. It is outbound-only: Bella does
not read Slack history, respond to Slack messages, or require a public Slack
callback URL.

Each self-hosted operator creates and owns their own Slack app. Bella provides
a reusable manifest so setup is consistent and does not require manually
configuring scopes.

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
- Slack thread actions, commands, and inbound events.
- Bella Cloud OAuth installation.

The current environment-based setup is deliberately simple and deterministic
for one self-hosted organization. Cloud OAuth will use a separate,
per-organization credential model.

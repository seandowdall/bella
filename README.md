# Bella

Open source AI cost visibility platform.

## Structure

```text
crates/core      Shared domain types
crates/api       Axum HTTP API
crates/db        Postgres connection, migrations, and query helpers
crates/cli       Local command-line client
crates/ingestion Provider usage and cost ingestion
crates/mcp       MCP server
crates/sandbox   Local provider sandbox
crates/worker    Scheduled ingestion and incident delivery worker
apps/web         Next.js dashboard for app.bellalabs.ai
apps/site        Next.js landing page for bellalabs.ai
apps/docs        Contributor and self-hosting documentation
packages/openapi API contract placeholder
packages/bella-* TypeScript SDK packages for Bella usage events
packages/*       Shared package placeholders
```

## Local Postgres

```sh
docker compose up -d postgres
```

The default API database URL is:

```text
postgres://bella:bella@127.0.0.1:5432/bella
```

## Local Development

```sh
just dev
```

Useful commands:

```sh
just docker       # start Docker services
just pgweb        # start pgweb
just api          # run the Axum API
just web          # run the Next.js dashboard
just site         # run the public landing page
just cli --help   # run the Bella CLI
just verify       # fmt, check, clippy, test
just stop         # stop Docker services
bun run build:sdks     # build TypeScript SDK packages
bun run typecheck:sdks # typecheck TypeScript SDK packages
```

Health check:

```sh
curl http://127.0.0.1:3000/health
```

## GitHub OAuth

GitHub OAuth is required for dashboard and CLI login. For local development,
create an OAuth app with:

```text
Homepage URL: http://127.0.0.1:5173
Authorization callback URL: http://127.0.0.1:3000/v1/auth/github/callback
```

Copy `.env.example` to `.env`, then set:

```text
GITHUB_OAUTH_CLIENT_ID=...
GITHUB_OAUTH_CLIENT_SECRET=...
BELLA_CREDENTIAL_ENCRYPTION_KEY=...
```

Generate the provider credential encryption key once with:

```sh
openssl rand -base64 32
```

Keep this key stable and secret. Rotating it requires re-encrypting stored
provider credentials.

Run the API and web app:

```sh
just api
just web
```

Run the background worker to import provider usage, sync configured PostHog
signals, and deliver incident notifications on a schedule:

```sh
just worker
```

The dashboard uses an HTTP-only session cookie. The CLI uses the same GitHub
OAuth app through a browser handoff and stores its API token in
`~/.config/bella/credentials.json` with owner-only permissions:

```sh
just cli login
just cli whoami
just cli logout
```

Full setup guides:

- [Bella CLI beta](apps/docs/cli-beta.md)
- [Contributor OAuth setup](apps/docs/contributors/github-oauth.md)
- [Self-hosted OAuth setup](apps/docs/self-hosting/github-oauth.md)
- [GitHub App integration](apps/docs/self-hosting/github-app.md)
- [Self-hosted Slack incident delivery](apps/docs/self-hosting/slack.md)
- [Slack Cloud QA deployment](apps/docs/deployment/slack-cloud-qa.md)
- [OpenAI ingestion](apps/docs/ingestion/openai.md)
- [PostHog live incident dogfood](apps/docs/ingestion/posthog-dogfood.md)

## SDKs

Bella includes TypeScript SDK packages for recording LLM usage events through
the API:

- `@bella/core`: shared transport, event types, and API errors.
- `@bella/server`: Node/server SDK for wrapping LLM calls and recording usage.
- `@bella/web`: browser SDK for identity/context and lightweight telemetry.

The server SDK is the production dogfood path for TypeScript services. It fails
open by default so Bella ingestion outages do not break customer LLM calls, and
it does not send prompts, completions, provider API keys, or raw error messages
unless explicitly configured by the caller.

For hosted dogfooding, configure server-side services with:

```text
BELLA_API_KEY=...
BELLA_API_URL=https://api.example.com
BELLA_ORGANIZATION_ID=...
BELLA_PROVIDER_ACCOUNT_ID=...
BELLA_PROVIDER=openai
BELLA_SDK_FAIL_OPEN=true
BELLA_SDK_CAPTURE_ERROR_MESSAGE=false
```

See [SDK package docs](packages/README.md) for quickstart examples.

## Organizations

Every user receives a default organization and workspace on first login.
Additional organizations can be created from the dashboard or CLI:

```sh
just cli organizations list
just cli organizations create --name "Acme AI"
just cli --json organizations list
```

Provider connections are available from the dashboard and CLI. When your user
belongs to one organization, the account list selects it automatically:

```sh
# Supported provider types:
just cli providers catalog

# Accounts connected in the web UI or CLI:
just cli providers accounts
just cli providers accounts --organization <organization-id>

printf '%s' "$PROVIDER_API_KEY" | just cli providers connect \
  --organization <organization-id> \
  --workspace <workspace-id> \
  --provider mistral \
  --name production \
  --secret-stdin
just cli providers disconnect \
  --organization <organization-id> \
  --account <provider-account-id>
```

All provider commands support the global `--json` flag. Provider secrets are
sent to the API for encrypted storage and are never written to the local Bella
CLI credential file.

Bella automatically validates OpenAI and Anthropic admin credentials against
their organization usage APIs. Mistral and DeepSeek credentials are validated
against read-only model-list endpoints. Other provider types remain explicitly
`saved_unverified` until a provider-specific validator is implemented.

See the [multi-tenant architecture](apps/docs/architecture/multi-tenancy.md)
for tenant boundaries, roles, natural keys, and idempotency behavior.

## License

MIT

# Bella Cloud Deployment

Bella Cloud should use the same application contract as self-hosted
installations, but run on managed infrastructure where that reduces operational
risk.

## Public URL Contract

Serve the dashboard and API through one HTTPS origin:

```text
Dashboard: https://app.bellalabs.ai/
API:       https://app.bellalabs.ai/api/
```

The reverse proxy or platform router must strip `/api` before forwarding API
requests:

```text
/api/health -> bella-api:3000/health
/api/v1/... -> bella-api:3000/v1/...
/*          -> bella-web
```

This keeps browser cookies same-origin and gives self-hosted and hosted
deployments the same external shape.

## Recommended Hosted Shape

Start with managed services for state and secrets:

```text
Managed edge or routing layer
  -> bella-web container
  -> bella-api container
      -> managed Postgres
```

Use the checked-in Dockerfiles as the portable artifact:

```sh
docker build -f Dockerfile.api -t bella-api .
docker build -f Dockerfile.web -t bella-web .
```

The same images should run on a VM, Render, Fly, ECS, or another container
runtime. The hosted deployment should prefer managed Postgres over a database on
the same VM as the application.

## Required Environment

API service:

```env
DATABASE_URL=postgres://...
BELLA_PUBLIC_API_URL=https://app.bellalabs.ai/api
BELLA_WEB_URL=https://app.bellalabs.ai
BELLA_SECURE_COOKIES=true
BELLA_CREDENTIAL_ENCRYPTION_KEY=base64-encoded-32-byte-key
GITHUB_OAUTH_CLIENT_ID=...
GITHUB_OAUTH_CLIENT_SECRET=...
```

Runtime binding:

```env
# Docker/self-hosted default
BELLA_API_BIND_ADDR=0.0.0.0:3000

# Managed platforms may set PORT instead. BELLA_API_BIND_ADDR takes precedence.
PORT=3000
```

Web service:

```env
NEXT_PUBLIC_BELLA_API_BASE_URL=/api
BELLA_INTERNAL_API_URL=http://bella-api:3000
```

Only `NEXT_PUBLIC_BELLA_API_BASE_URL` is safe to expose to the browser. GitHub
OAuth secrets, database credentials, and provider credentials must never be set
as `NEXT_PUBLIC_` variables.

## Secrets

Store these in the hosting platform's secret manager:

- `DATABASE_URL`
- `BELLA_CREDENTIAL_ENCRYPTION_KEY`
- `GITHUB_OAUTH_CLIENT_SECRET`
- Provider credentials submitted through Bella

Generate `BELLA_CREDENTIAL_ENCRYPTION_KEY` once and keep it stable:

```sh
openssl rand -base64 32
```

Rotating this key later requires re-encrypting stored provider credentials.

## GitHub OAuth

Create a GitHub OAuth app for each environment.

Production:

```text
Homepage URL: https://app.bellalabs.ai
Authorization callback URL: https://app.bellalabs.ai/api/v1/auth/github/callback
```

The callback must match `BELLA_PUBLIC_API_URL` plus
`/v1/auth/github/callback`.

## Database

Use managed Postgres for Bella Cloud. The first production choice should be one
of:

- Render Postgres when the app also runs on Render.
- Neon Postgres when database branching, restore workflows, or provider
  independence matter more.
- AWS RDS or another cloud-native Postgres when deploying into an existing cloud
  account.

The application user should have only the privileges needed by the app and
migrations. Keep backups, restore windows, and database region documented in the
deployment record.

## Deployment Checklist

- HTTPS is enabled for the public domain.
- `BELLA_SECURE_COOKIES=true`.
- `BELLA_PUBLIC_API_URL` is externally reachable and uses `/api`.
- `BELLA_WEB_URL` is the exact dashboard origin with no trailing path.
- The GitHub OAuth callback exactly matches the public API callback URL.
- Secrets are stored in the platform secret manager.
- No GitHub, database, or provider secrets are exposed as `NEXT_PUBLIC_`
  variables.
- API health check is configured as `/health` internally or `/api/health`
  externally.
- Postgres backups and restore expectations are recorded.
- Deploys are non-interactive and reproducible from the Dockerfiles.

## Smoke Test

Run the HTTP smoke test after deploy:

```sh
deploy/smoke-test.sh https://app.bellalabs.ai
```

Then validate authenticated flows:

```sh
bella --api-base-url https://app.bellalabs.ai/api login
bella --api-base-url https://app.bellalabs.ai/api whoami
bella --api-base-url https://app.bellalabs.ai/api organizations list
bella --api-base-url https://app.bellalabs.ai/api providers catalog
bella --api-base-url https://app.bellalabs.ai/api logout
```

# Bella Cloud Deployment

Bella Cloud v1 runs on a self-managed Hetzner VPS. The hosted deployment should
stay close to the self-hosted shape while making our production operations
explicit and repeatable.

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

## Hosted Shape

Bella Cloud v1 uses Docker Compose on a Hetzner VPS:

```text
Hetzner VPS
  -> Caddy reverse proxy / TLS
  -> bella-web container
  -> bella-api container
  -> Postgres container with persistent volume
```

Use the checked-in Dockerfiles as the portable artifact:

```sh
docker build -f Dockerfile.api -t bella-api .
docker build -f Dockerfile.web -t bella-web .
```

The same images can later run on Render, Fly, ECS, or another container runtime.
For now, the production runbook is [Hetzner VPS Deployment](hetzner.md).

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

Store these only in the deployment environment file on the server:

- `DATABASE_URL`
- `BELLA_CREDENTIAL_ENCRYPTION_KEY`
- `GITHUB_OAUTH_CLIENT_SECRET`
- Provider credentials submitted through Bella

The server copy of `deploy/hetzner/.env` must not be committed and should be
readable only by the deployment user.

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

Bella Cloud v1 uses the Postgres service in `deploy/hetzner/docker-compose.yml`
with a persistent Docker volume. Because this is self-managed, production is not
ready until backup and restore are configured.

Minimum requirements:

- Postgres is not exposed publicly.
- The data volume survives container restarts and image rebuilds.
- Daily backups run successfully.
- Backups are copied off the VPS.
- Restore is tested against staging before production launch.

## Deployment Checklist

- HTTPS is enabled for the public domain.
- DNS points `app.bellalabs.ai` to the Hetzner VPS.
- `BELLA_SECURE_COOKIES=true`.
- `BELLA_PUBLIC_API_URL` is externally reachable and uses `/api`.
- `BELLA_WEB_URL` is the exact dashboard origin with no trailing path.
- The GitHub OAuth callback exactly matches the public API callback URL.
- Secrets are stored only in the server `.env` file with restricted
  permissions.
- No GitHub, database, or provider secrets are exposed as `NEXT_PUBLIC_`
  variables.
- API health check is configured as `/health` internally or `/api/health`
  externally.
- Postgres backups, off-server copy, and restore expectations are recorded.
- Firewall allows only SSH, HTTP, and HTTPS.
- Caddy is the only public entrypoint for web and API traffic.
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

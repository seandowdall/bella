# Hetzner VPS Deployment

Bella Cloud v1 runs on a self-managed Hetzner VPS. This keeps the hosted
deployment close to the self-hosted path while making the operational
responsibilities explicit.

```text
Hetzner VPS
  -> Caddy reverse proxy / TLS
  -> bella-web container
  -> bella-api container
  -> Postgres container with persistent volume
```

## Public Contract

```text
Dashboard: https://app.bellalabs.ai/
API:       https://app.bellalabs.ai/api/
```

Caddy strips `/api` before forwarding API requests:

```text
/api/health -> bella-api:3000/health
/api/v1/... -> bella-api:3000/v1/...
/*          -> bella-web:5173
```

## Server Setup

1. Create a Hetzner VPS with Debian or Ubuntu LTS.
2. Add DNS records:

```text
app.bellalabs.ai A    <server-ipv4>
app.bellalabs.ai AAAA <server-ipv6> # if enabled
```

3. SSH with keys only. Disable password login after initial access.
4. Install Docker Engine and the Docker Compose plugin.
5. Allow only SSH, HTTP, and HTTPS at the firewall:

```sh
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw enable
```

Do not expose Postgres or application container ports directly to the public
internet.

## Environment

Copy the template and fill in production values:

```sh
cp deploy/hetzner/.env.example deploy/hetzner/.env
```

Generate the credential encryption key once:

```sh
openssl rand -base64 32
```

Required values:

```env
BELLA_DOMAIN=app.bellalabs.ai
ACME_EMAIL=ops@bellalabs.ai

POSTGRES_DB=bella
POSTGRES_USER=bella
POSTGRES_PASSWORD=...
DATABASE_URL=postgres://bella:...@postgres:5432/bella

BELLA_PUBLIC_API_URL=https://app.bellalabs.ai/api
BELLA_WEB_URL=https://app.bellalabs.ai
BELLA_SECURE_COOKIES=true
BELLA_API_BIND_ADDR=0.0.0.0:3000
BELLA_INTERNAL_API_URL=http://bella-api:3000

BELLA_CREDENTIAL_ENCRYPTION_KEY=...
GITHUB_OAUTH_CLIENT_ID=...
GITHUB_OAUTH_CLIENT_SECRET=...
```

Keep `deploy/hetzner/.env` on the server only. Do not commit it.

## GitHub OAuth

Create a production GitHub OAuth app:

```text
Homepage URL: https://app.bellalabs.ai
Authorization callback URL: https://app.bellalabs.ai/api/v1/auth/github/callback
```

The callback must equal `BELLA_PUBLIC_API_URL` plus
`/v1/auth/github/callback`.

## Deploy

From the repository root:

```sh
docker compose --env-file deploy/hetzner/.env \
  -f deploy/hetzner/docker-compose.yml \
  up -d --build
```

Check service state:

```sh
docker compose --env-file deploy/hetzner/.env \
  -f deploy/hetzner/docker-compose.yml \
  ps
```

View logs:

```sh
docker compose --env-file deploy/hetzner/.env \
  -f deploy/hetzner/docker-compose.yml \
  logs -f caddy bella-api bella-web
```

The same operations are available through `just`:

```sh
just hetzner-config
just hetzner-up
just hetzner-logs
```

## Smoke Tests

Run the HTTP smoke test:

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

Also validate provider connection once provider ingestion work is ready:

```sh
printf '%s' "$PROVIDER_API_KEY" | bella --api-base-url https://app.bellalabs.ai/api providers connect \
  --organization <organization-id> \
  --workspace <workspace-id> \
  --provider <provider-id> \
  --name production \
  --secret-stdin
```

## Backups

Create an on-server backup:

```sh
deploy/hetzner/backup-postgres.sh
```

or:

```sh
just hetzner-backup
```

Backups are written to:

```text
backups/hetzner/bella-postgres-YYYYMMDDTHHMMSSZ.sql.gz
```

Production backups must also be copied off the VPS, for example to a Hetzner
Storage Box, S3, or Backblaze B2. A backup that exists only on the VPS does not
protect against server loss.

Recommended schedule:

- At least daily database backups.
- Off-server copy after every backup.
- Alert when a backup fails.
- Monthly restore drill into staging.

Restore requires an explicit confirmation flag:

```sh
BELLA_CONFIRM_RESTORE=yes deploy/hetzner/restore-postgres.sh backups/hetzner/bella-postgres-YYYYMMDDTHHMMSSZ.sql.gz
```

Run restore drills against staging first. Restoring production is a downtime
operation unless a separate restore target is prepared.

## Security Checklist

- SSH keys only; password SSH disabled.
- Root SSH login disabled after a deploy user exists.
- Firewall allows only `22`, `80`, and `443`.
- Postgres is reachable only on the private Docker network.
- `bella-api` and `bella-web` are reachable only through Caddy.
- `BELLA_SECURE_COOKIES=true` in production.
- `deploy/hetzner/.env` is readable only by the deployment user.
- GitHub OAuth secret, Postgres password, and credential encryption key are not
  committed or exposed as `NEXT_PUBLIC_` variables.
- Docker daemon is not exposed over TCP.
- OS security updates are applied regularly.
- Docker images are rebuilt and redeployed when base images need security
  updates.

## Monitoring

Minimum production monitoring:

- Uptime check for `https://app.bellalabs.ai/`.
- Uptime check for `https://app.bellalabs.ai/api/health`.
- Disk usage alert for the VPS.
- Alert on failed Postgres backup.
- Log review path for Caddy, API, and web containers.
- Container restart visibility.

## Scaling Path

One VPS is acceptable for Bella Cloud v1, but it has a clear ceiling. Move off
the single-server shape when any of these become true:

- Postgres disk, CPU, or memory competes with app traffic.
- Backup or restore windows become too long.
- Uptime requirements exceed what one VPS can provide.
- App traffic requires multiple API/web instances.

Next step after one VPS:

```text
Load balancer / edge proxy
  -> app VPS 1: bella-web + bella-api
  -> app VPS 2: bella-web + bella-api
  -> separate Postgres host or managed Postgres
```

Separating Postgres is the first scaling move because it removes customer data
from the disposable app server layer.

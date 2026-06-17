# Hetzner VPS Deployment

Bella Cloud v1 runs on a self-managed Hetzner VPS. This keeps the hosted
deployment close to the self-hosted path while making the operational
responsibilities explicit.

This runbook defines the deployment path before anything is hosted. Treat
`app.bellalabs.ai` as the intended Bella Cloud production domain, not evidence
that the service is already live.

```text
Hetzner VPS
  -> Caddy reverse proxy / TLS
  -> bella-web container
  -> bella-api container
  -> Postgres container with persistent volume
```

## Public Contract

```text
Dashboard: https://<bella-domain>/
API:       https://<bella-domain>/api/
```

For Bella Cloud production, the intended domain is `app.bellalabs.ai`. This
contract is not considered live until the VPS exists, DNS points at it, and the
smoke tests pass.

Caddy strips `/api` before forwarding API requests:

```text
/api/health -> bella-api:3000/health
/api/v1/... -> bella-api:3000/v1/...
/*          -> bella-web:5173
```

## Server Setup

1. Choose the production domain. Bella Cloud intends to use
   `app.bellalabs.ai`.
2. Create a Hetzner VPS with Debian or Ubuntu LTS.
3. Add DNS records after the VPS has a public IP:

```text
<bella-domain> A    <server-ipv4>
<bella-domain> AAAA <server-ipv6> # if enabled
```

4. SSH with keys only. Disable password login after initial access.
5. Install Docker Engine and the Docker Compose plugin.
6. Allow only SSH, HTTP, and HTTPS at the firewall:

```sh
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw enable
```

Do not expose Postgres or application container ports directly to the public
internet.

## Before Provisioning

Have these ready before creating the production VPS:

- Domain decision and DNS access for the chosen hostname.
- GitHub OAuth app ownership and callback URL.
- A non-shared email address for Let's Encrypt notices.
- A generated Postgres password.
- A generated `BELLA_CREDENTIAL_ENCRYPTION_KEY`.
- A backup destination outside the VPS, such as a Hetzner Storage Box, S3, or
  Backblaze B2.
- An uptime and disk-alert destination.

Nothing in this repository should contain real values for those secrets. Keep
only `deploy/hetzner/.env.example` in Git.

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
BELLA_DOMAIN=<bella-domain>
ACME_EMAIL=<tls-notice-email>

POSTGRES_DB=bella
POSTGRES_USER=bella
POSTGRES_PASSWORD=...
DATABASE_URL=postgres://bella:...@postgres:5432/bella

BELLA_PUBLIC_API_URL=https://<bella-domain>/api
BELLA_WEB_URL=https://<bella-domain>
BELLA_SECURE_COOKIES=true
BELLA_API_BIND_ADDR=0.0.0.0:3000
BELLA_INTERNAL_API_URL=http://bella-api:3000

BELLA_CREDENTIAL_ENCRYPTION_KEY=...
GITHUB_OAUTH_CLIENT_ID=...
GITHUB_OAUTH_CLIENT_SECRET=...
```

Keep `deploy/hetzner/.env` on the server only. Do not commit it.

Validate the file before deploying:

```sh
just hetzner-preflight
```

The preflight fails when required values are missing, placeholders are still in
use, production URLs are not HTTPS, secure cookies are disabled, or the
credential encryption key is not a base64-encoded 32-byte key.

## GitHub OAuth

Create a production GitHub OAuth app:

```text
Homepage URL: https://<bella-domain>
Authorization callback URL: https://<bella-domain>/api/v1/auth/github/callback
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
just hetzner-preflight
just hetzner-up
just hetzner-logs
```

`just hetzner-up` runs preflight first. Use `just hetzner-config` when you only
want to render the Compose file for debugging.

## Smoke Tests

Run the HTTP smoke test:

```sh
deploy/smoke-test.sh https://<bella-domain>
```

Then validate authenticated flows:

```sh
bella --api-base-url https://<bella-domain>/api login
bella --api-base-url https://<bella-domain>/api whoami
bella --api-base-url https://<bella-domain>/api organizations list
bella --api-base-url https://<bella-domain>/api providers catalog
bella --api-base-url https://<bella-domain>/api logout
```

Also validate provider connection once provider ingestion work is ready:

```sh
printf '%s' "$PROVIDER_API_KEY" | bella --api-base-url https://<bella-domain>/api providers connect \
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

Example cron entry for the deployment user:

```cron
17 2 * * * cd /opt/bella && deploy/hetzner/backup-postgres.sh && rclone sync backups/hetzner remote:bella-postgres
```

Replace the `rclone` target with the actual off-server destination. Do not treat
the cron example as complete until the sync command has been tested and failure
alerts are configured.

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

- Uptime check for `https://<bella-domain>/`.
- Uptime check for `https://<bella-domain>/api/health`.
- Disk usage alert for the VPS.
- Alert on failed Postgres backup.
- Log review path for Caddy, API, and web containers.
- Container restart visibility.

## Ready To Deploy

BEL-5 is ready for the real Hetzner deploy when all of these are true:

- `deploy/hetzner/.env` exists only on the server.
- `just hetzner-preflight` passes on the server.
- `just hetzner-config` renders the expected Compose file.
- DNS points the chosen hostname at the VPS.
- GitHub OAuth callback matches `https://<bella-domain>/api/v1/auth/github/callback`.
- A backup command writes a dump and copies it off-server.
- `deploy/hetzner/restore-postgres.sh` has been tested against a non-production
  target.
- Uptime, disk, and backup-failure alerts have destinations.

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

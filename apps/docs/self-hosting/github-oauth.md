# Configure GitHub OAuth for Self-Hosting

Every self-hosted Bella installation must use an OAuth app owned by its
operator. Bella does not ship a shared production GitHub client secret.

The OAuth app controls who GitHub identifies. Bella remains responsible for
application sessions, organizations, team membership, and authorization.

## Recommended Deployment

Serve the dashboard and API through one HTTPS origin:

```text
Dashboard: https://bella.example.com/
API:       https://bella.example.com/api/
```

Configure the reverse proxy to remove the `/api` prefix before forwarding
requests to `bella-api`. This same-origin layout avoids cross-origin cookie and
CORS configuration.

## Create the GitHub OAuth App

In the GitHub account or organization that operates Bella, open
**Settings > Developer settings > OAuth Apps** and create an OAuth app.

For the example deployment, use:

```text
Application name: Bella
Homepage URL: https://bella.example.com
Authorization callback URL: https://bella.example.com/api/v1/auth/github/callback
```

Replace `bella.example.com` with the public Bella hostname. The callback must
match the public API URL exactly, including the scheme and `/api` prefix.

The CLI uses this same callback through its browser login flow, so it does not
need a separate GitHub OAuth app.

## Configure the API

Set these values in the API service environment:

```env
GITHUB_OAUTH_CLIENT_ID=your_client_id
GITHUB_OAUTH_CLIENT_SECRET=your_client_secret
BELLA_PUBLIC_API_URL=https://bella.example.com/api
BELLA_WEB_URL=https://bella.example.com
BELLA_ALLOWED_ORIGINS=https://bella.example.com
BELLA_SECURE_COOKIES=true
BELLA_ALLOWED_GITHUB_EMAILS=seandowdall22@gmail.com,tadhg.jamesdowdall@gmail.com
BELLA_API_BIND_ADDR=0.0.0.0:3000
DATABASE_URL=postgres://...
```

Requirements:

- Store `GITHUB_OAUTH_CLIENT_SECRET` in the deployment secret manager.
- Never put the client secret in the web image or a `VITE_` variable.
- Use HTTPS and `BELLA_SECURE_COOKIES=true`.
- Keep `BELLA_PUBLIC_API_URL` externally reachable. Do not use the container's
  internal hostname.
- Set `BELLA_WEB_URL` to the exact dashboard origin without a trailing path.
- Set `BELLA_ALLOWED_ORIGINS` to the exact browser origins allowed to call the
  API with credentials. Use a comma-separated list only when one API serves
  multiple trusted dashboard origins.
- Set `BELLA_ALLOWED_GITHUB_EMAILS` to a comma-separated list when you want to
  restrict login to specific GitHub accounts by primary verified email.

## Configure the Dashboard

Build the dashboard with:

```env
NEXT_PUBLIC_BELLA_API_BASE_URL=/api
```

This value is public and is embedded in the browser bundle. It is an API path,
not a secret.

Configure the reverse proxy so:

```text
/api/v1/... -> bella-api:3000/v1/...
/*           -> Bella dashboard static files
```

The callback and CLI success routes must reach the API and dashboard
respectively:

```text
/api/v1/auth/github/callback -> API
/auth/cli/success            -> dashboard index.html
```

Because the dashboard is a single-page application, configure static hosting
to fall back to `index.html` for unknown dashboard routes.

## Validate the Installation

1. Open `https://bella.example.com`.
2. Log in with GitHub and confirm the dashboard shows the GitHub account.
3. Run the CLI against the public API:

```sh
bella --api-base-url https://bella.example.com/api login
bella --api-base-url https://bella.example.com/api whoami
```

4. Log out and confirm the CLI token is revoked:

```sh
bella --api-base-url https://bella.example.com/api logout
```

## Secret Rotation

Generate a new client secret in GitHub, update the API secret, and restart the
API. Existing Bella web sessions and CLI tokens remain valid because Bella
stores its own sessions; the GitHub secret is used only during new OAuth
exchanges.

After confirming new logins work, remove the old GitHub client secret.

## Multiple Environments

Use a separate GitHub OAuth app for each environment, such as production and
staging. Each app can then have the exact callback URL for that environment,
and rotating one environment's secret will not affect the others.

Production and QA should also use separate values for:

- Postgres databases and database users
- `GITHUB_OAUTH_CLIENT_ID` and `GITHUB_OAUTH_CLIENT_SECRET`
- `BELLA_PUBLIC_API_URL`, `BELLA_WEB_URL`, and `BELLA_ALLOWED_ORIGINS`
- `BELLA_CREDENTIAL_ENCRYPTION_KEY`
- Provider credentials and webhook secrets

For example, if the production dashboard is `https://app.bella.md` and QA is
`https://app.qa.bella.md`, configure each API with only its own trusted origin:

```env
# production API
BELLA_PUBLIC_API_URL=https://api.bella.md
BELLA_WEB_URL=https://app.bella.md
BELLA_ALLOWED_ORIGINS=https://app.bella.md

# QA API
BELLA_PUBLIC_API_URL=https://api.qa.bella.md
BELLA_WEB_URL=https://app.qa.bella.md
BELLA_ALLOWED_ORIGINS=https://app.qa.bella.md
```

The API rejects cookie-authenticated state-changing browser requests unless the
request comes from one of these trusted origins. Bearer-token requests from the
CLI and SDK do not depend on browser cookies and are not subject to this browser
origin check.

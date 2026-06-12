# GitHub OAuth for Contributors

Bella requires GitHub OAuth for dashboard and CLI login. Each contributor
should create a GitHub OAuth app for local development rather than sharing a
client secret.

## Create a Local OAuth App

In GitHub, open **Settings > Developer settings > OAuth Apps**, create a new
OAuth app, and use:

```text
Application name: Bella Local
Homepage URL: http://127.0.0.1:5173
Authorization callback URL: http://127.0.0.1:3000/v1/auth/github/callback
```

Use `127.0.0.1` consistently. GitHub treats `localhost` and `127.0.0.1` as
different callback hosts.

## Configure Bella

Create your local environment file:

```sh
cp .env.example .env
```

Add the client ID and generated client secret:

```env
GITHUB_OAUTH_CLIENT_ID=your_client_id
GITHUB_OAUTH_CLIENT_SECRET=your_client_secret
```

Keep the secret in `.env`. Do not commit it and do not expose it through a
`VITE_` environment variable. The API is the only component that exchanges the
GitHub authorization code.

The remaining local defaults should be:

```env
BELLA_PUBLIC_API_URL=http://127.0.0.1:3000
BELLA_WEB_URL=http://127.0.0.1:5173
BELLA_SECURE_COOKIES=false
```

## Run and Test

Start the API and dashboard in separate terminals:

```sh
just api
just web
```

Open `http://127.0.0.1:5173` and select **Log in with GitHub**.

The Vite development server proxies `/api` to the API on port `3000`. The
dashboard receives an HTTP-only session cookie; it never receives the GitHub
client secret or GitHub access token.

Test CLI login with the same OAuth app:

```sh
just cli login
just cli whoami
just cli logout
```

CLI login opens the GitHub authorization page and polls Bella for completion.
The resulting Bella API token is stored in
`~/.config/bella/credentials.json` with owner-only permissions.

## Troubleshooting

### Callback URL mismatch

Confirm that the GitHub OAuth app callback is exactly:

```text
http://127.0.0.1:3000/v1/auth/github/callback
```

Also confirm that `BELLA_PUBLIC_API_URL` is
`http://127.0.0.1:3000`.

### Address already in use

Find the process listening on the API port:

```sh
lsof -nP -iTCP:3000 -sTCP:LISTEN
```

Stop it with `kill <PID>`, then run `just api` again.

### Login succeeds but the dashboard is logged out

Use `http://127.0.0.1:5173`, not a mixture of `localhost` and `127.0.0.1`.
Cookies are scoped by host.

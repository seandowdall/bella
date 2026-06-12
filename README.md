# Bella

Open source AI cost visibility platform.

## Structure

```text
crates/core      Shared domain types
crates/api       Axum HTTP API
crates/db        Postgres connection, migrations, and query helpers
crates/cli       Local command-line client
crates/mcp       MCP server placeholder
crates/worker    Async job worker placeholder
apps/web         Vite/React dashboard
apps/docs        Contributor and self-hosting documentation
packages/openapi API contract placeholder
packages/*       Shared package placeholders
sdks             Client SDK placeholders
examples         Example integration placeholders
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
just web          # run the Vite dashboard
just cli --help   # run the Bella CLI
just verify       # fmt, check, clippy, test
just stop         # stop Docker services
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
```

Run the API and web app:

```sh
just api
just web
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

- [Contributor OAuth setup](apps/docs/contributors/github-oauth.md)
- [Self-hosted OAuth setup](apps/docs/self-hosting/github-oauth.md)

## License

MIT

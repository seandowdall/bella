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
apps/docs        Docs placeholder
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

See the [initial setup guide](apps/docs/README.md#initial-setup) to configure your local environment.

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

## License

MIT

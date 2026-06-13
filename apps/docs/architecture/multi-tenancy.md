# Multi-Tenant Architecture

Bella is multi-tenant by default, including self-hosted installations.
Organizations are the top-level tenant and workspaces partition provider
accounts, usage, and cost data within an organization.

```text
User
  -> Organization membership (owner, admin, or member)
    -> Organization
      -> Workspace
        -> Provider accounts
          -> Usage and cost data
```

## First Login

After GitHub login, Bella idempotently ensures that the user belongs to at
least one organization. A new user receives:

- An organization derived from their GitHub login
- An `owner` membership
- A workspace named `Default` with the stable slug `default`

Concurrent or repeated login callbacks cannot create duplicate default
organizations because onboarding is serialized per user in Postgres.

## Creating Organizations

Authenticated users can create additional organizations through:

```http
POST /v1/organizations
Idempotency-Key: stable-client-generated-key
Content-Type: application/json

{
  "name": "Acme AI"
}
```

Creation transactionally adds the organization, the caller's `owner`
membership, and its default workspace. Retrying the same request with the same
idempotency key returns the existing organization. Reusing the key with a
different request is rejected.

The CLI exposes the same API:

```sh
bella organizations list
bella organizations create --name "Acme AI"
bella organizations create \
  --name "Acme AI" \
  --idempotency-key deployment-acme-ai
```

All commands support `--json`.

## Authorization Boundary

Possessing an organization UUID or slug never grants access. API handlers must
join through `organization_memberships` using the authenticated user ID before
reading or mutating tenant data.

Future records should carry tenant scope directly or through a required
workspace relationship:

```text
organizations
  -> workspaces
    -> provider_accounts
      -> usage_events
      -> cost_snapshots
```

Stable natural keys should be unique inside their owning scope:

- Organization slug: globally unique
- Workspace slug: unique per organization
- Provider account key: unique per workspace and provider
- Usage event key: unique per provider account and provider event ID

PostgreSQL row-level security can later provide defense in depth, but API
authorization remains mandatory.

## Client Architecture

The dashboard, CLI, and future MCP server call the same Axum routes:

```text
Dashboard --\
CLI ---------> Axum API -> Postgres
MCP --------/
```

Clients do not write directly to Postgres. This keeps tenant authorization,
validation, audit behavior, and idempotency consistent.

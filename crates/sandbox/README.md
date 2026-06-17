# Bella Sandbox

`bella-sandbox` serves deterministic mock provider APIs for local ingestion
development. It keeps real provider identities such as `openai`; adapters are
pointed at sandbox URLs with provider base URL overrides.

## Run

```sh
cargo run -p bella-sandbox
```

The default bind address is `127.0.0.1:4010`. The OpenAI mock base URL is:

```text
http://127.0.0.1:4010/openai
```

Run Bella against the mock OpenAI API with:

```sh
OPENAI_BASE_URL=http://127.0.0.1:4010/openai cargo run -p bella-worker
```

or:

```sh
OPENAI_BASE_URL=http://127.0.0.1:4010/openai cargo run -p bella-api
```

## Scenarios

Select a scenario with `--scenario` or `BELLA_SANDBOX_SCENARIO`:

```sh
cargo run -p bella-sandbox -- --scenario happy-path
cargo run -p bella-sandbox -- --scenario pagination
cargo run -p bella-sandbox -- --scenario rate-limit-once
cargo run -p bella-sandbox -- --scenario server-error-once
cargo run -p bella-sandbox -- --scenario duplicate-buckets
cargo run -p bella-sandbox -- --scenario malformed-usage
cargo run -p bella-sandbox -- --scenario malformed-costs
cargo run -p bella-sandbox -- --scenario missing-optional-dimensions
```

Current scenarios:

- `happy-path`: one usage page and one cost page.
- `pagination`: usage and cost endpoints return a second page.
- `rate-limit-once`: each OpenAI endpoint returns one `429 Retry-After` before
  succeeding.
- `server-error-once`: each OpenAI endpoint returns one `500` before succeeding.
- `duplicate-buckets`: usage and cost responses include duplicate buckets to test
  idempotent imports.
- `malformed-usage`: usage response is missing required bucket timestamps.
- `malformed-costs`: costs response has an invalid `amount.value` shape.
- `missing-optional-dimensions`: usage/cost rows omit optional model, project,
  user, and API key dimensions.

## OpenAI Routes

```text
GET /openai/v1/organization/usage/completions
GET /openai/v1/organization/costs
```

These routes accept the same query parameters the adapter sends, including
`start_time`, `end_time`, `bucket_width`, `limit`, `page`, and `group_by[]`.
The current fixtures are deterministic and intentionally small so ingestion
tests can assert exact normalized usage and cost rows.

## Future Providers

Add future providers under provider-prefixed paths rather than creating fake
provider IDs:

```text
/anthropic/...
/mistral/...
```

The product data should continue to use real provider IDs like `openai` and
`anthropic`.

# Bella Go SDK Contract

This document defines the Go SDK contract from the existing TypeScript SDK. The
TypeScript packages remain the source of truth for the MVP API shape:

- `@bella/core`: usage event types, event IDs, transport, and API errors.
- `@bella/server`: server-side LLM call wrapper behavior.
- `@bella/web`: browser telemetry. The Go SDK does not implement a browser SDK.

The Go SDK covers server-side usage recording only.

## Concept Mapping

| TypeScript | Go | Notes |
| --- | --- | --- |
| `BellaClient` | `Client` | Low-level SDK ingestion client. |
| `createBellaClient(options)` | `NewClient(options)` | Validates API key and organization ID. |
| `BellaClientOptions.apiKey` | `ClientOptions.APIKey` | Sent as `Authorization: Bearer <token>`. |
| `BellaClientOptions.baseUrl` | `ClientOptions.BaseURL` | Bella API base URL. Defaults to `http://127.0.0.1:3000`. Trailing slashes are trimmed. |
| `BellaClientOptions.organizationId` | `ClientOptions.OrganizationID` | Used in the SDK ingestion path. |
| `BellaClientOptions.fetch` | `ClientOptions.HTTPClient` | Allows caller-controlled HTTP behavior. |
| `BellaApiError` | `APIError` | Returned for non-2xx Bella API responses. |
| `recordUsageEvent(event)` | `RecordUsageEvent(ctx, event)` | Posts one usage event to Bella. |
| `createEventId(prefix)` | `CreateEventID(prefix)` | Creates SDK event IDs such as `llm_<random>`. |
| `BellaServer` | `Server` | Server-side LLM usage wrapper. |
| `createBellaServer(options)` | `NewServer(options)` | Builds a server wrapper from explicit options. |
| `createBellaServerFromEnv()` | `NewServerFromEnv()` | Builds from Bella environment variables. |
| `trackLlmCall(options)` | `TrackLlmCall(ctx, server, options)` | Wraps one provider call and records usage. |
| `BellaServerOptions.defaultProviderAccountId` | `ServerOptions.DefaultProviderAccountID` | Default provider account for wrapped calls. |
| `BellaServerOptions.defaultProvider` | `ServerOptions.DefaultProvider` | Default provider name for wrapped calls. |
| `BellaServerOptions.failOpen` | `ServerOptions.FailOpen` | Defaults to true. |
| `BellaServerOptions.captureErrorMessage` | `ServerOptions.CaptureErrorMessage` / `ErrorMessageFromError` | Defaults to false. |
| `BellaServerOptions.onIngestionError` | `ServerOptions.OnIngestionError` | Called when Bella ingestion fails. |
| `TrackLlmCallOptions.call` | `TrackLlmCallOptions.Call` | The provider call to execute. |
| `TrackLlmCallOptions.usage` | `TrackLlmCallOptions.UsageFromResult` | Caller-supplied token extractor. |
| `TrackLlmCallOptions.cost` | `TrackLlmCallOptions.CostFromResult` | Caller-supplied cost extractor. |

## Wire Payload

`RecordUsageEvent` sends the same JSON field names as `@bella/core`:

| TypeScript field | Go field | JSON field |
| --- | --- | --- |
| `eventId` | `UsageEvent.EventID` | `event_id` |
| `providerAccountId` | `UsageEvent.ProviderAccountID` | `provider_account_id` |
| `provider` | `UsageEvent.Provider` | `provider` |
| `model` | `UsageEvent.Model` | `model` |
| `operation` | `UsageEvent.Operation` | `operation` |
| `status` | `UsageEvent.Status` | `status` |
| `startedAt` | `UsageEvent.StartedAt` | `started_at` |
| `endedAt` | `UsageEvent.EndedAt` | `ended_at` |
| `usage.inputTokens` | `Usage.InputTokens` | `usage.input_tokens` |
| `usage.outputTokens` | `Usage.OutputTokens` | `usage.output_tokens` |
| `usage.totalTokens` | `Usage.TotalTokens` | `usage.total_tokens` |
| `cost.amountMicros` | `Cost.AmountMicros` | `cost.amount_micros` |
| `cost.currency` | `Cost.Currency` | `cost.currency` |
| `metadata` | `UsageEvent.Metadata` | `metadata` |
| `errorMessage` | `UsageEvent.ErrorMessage` | `error_message` |

The request is:

```text
POST /v1/organizations/{organization_id}/sdk/usage-events
Authorization: Bearer <BELLA_API_KEY>
Content-Type: application/json
```

Successful responses decode as:

```json
{
  "event_id": "llm_...",
  "accepted": true
}
```

## Server Wrapper Semantics

`TrackLlmCall` follows the TypeScript server SDK behavior:

1. Resolve `ProviderAccountID` and `Provider` from per-call options first, then
   server defaults.
2. Create an event ID with `CreateEventID("llm")` when the caller does not pass
   one.
3. Capture `StartedAt`.
4. Run the provider call.
5. On success, record a `succeeded` event with timing, model, operation, usage,
   cost, and metadata.
6. Return the provider result unchanged.
7. On provider error, record a `failed` event and return the original provider
   error.
8. If Bella ingestion fails after the provider call, return the provider result
   or provider error when fail-open is enabled.

The default operation is `llm.call`.

## Configuration

Explicit configuration uses:

| Go option | Environment variable | Meaning |
| --- | --- | --- |
| `ClientOptions.APIKey` | `BELLA_API_KEY` | Bella API token. |
| `ClientOptions.BaseURL` | `BELLA_API_URL` / `BELLA_PUBLIC_API_URL` | Bella API base URL. |
| `ClientOptions.OrganizationID` | `BELLA_ORGANIZATION_ID` | Bella organization ID. |
| `ServerOptions.DefaultProviderAccountID` | `BELLA_PROVIDER_ACCOUNT_ID` | Default Bella provider account ID. |
| `ServerOptions.DefaultProvider` | `BELLA_PROVIDER` | Default provider name. Defaults to `openai` from env setup. |
| `ServerOptions.FailOpen` | `BELLA_SDK_FAIL_OPEN` | Defaults to true. Set `false` to fail closed. |
| `ServerOptions.CaptureErrorMessage` | `BELLA_SDK_CAPTURE_ERROR_MESSAGE` | Defaults to false. Set `true` to capture `error.Error()`. |
| `ServerOptions.ErrorMessageFromError` | none | Caller-provided redacted error extractor. |
| `ServerOptions.OnIngestionError` | none | Callback for Bella ingestion errors. |

`NewServerFromEnv` returns `(nil, false, nil)` when `BELLA_API_KEY` or
`BELLA_ORGANIZATION_ID` is missing. This lets applications call their provider
directly when Bella is not configured.

## Privacy Defaults

The Go SDK must not capture these by default:

- prompts
- completions
- provider API keys
- provider request bodies
- provider response bodies
- raw error messages

Only usage metadata should be sent: provider account, provider, model,
operation, status, timestamps, token counts, optional cost, optional metadata,
and optional redacted error summaries.

Raw error messages are omitted unless `CaptureErrorMessage` is true or the
caller provides `ErrorMessageFromError`.

## Event IDs and Retries

Bella deduplicates SDK ingestion by organization and event ID. Retrying the same
usage event must reuse the same `EventID`.

`CreateEventID("llm")` is appropriate for a new provider call. If the caller is
retrying a previously attempted Bella ingestion request for the same provider
call, the caller should persist and reuse the original event ID.

The SDK does not automatically retry ingestion yet. This avoids hiding retry
policy decisions from the application and keeps event ID reuse explicit.

## Intentional Go Deviations

The Go SDK intentionally differs from TypeScript in a few places:

- `TrackLlmCall` is a package-level generic function, not a method. Go does not
  support methods with independent type parameters.
- Go does not dynamically inspect arbitrary provider result objects. Callers
  provide `ModelFromResult`, `UsageFromResult`, and `CostFromResult` functions
  when they want extracted metadata.
- `NewServerFromEnv` returns `(server, ok, error)` instead of `undefined`, which
  is idiomatic Go.
- `FailOpen` is `*bool` in `ServerOptions` so callers can distinguish omitted
  from explicitly false.
- `ClientOptions.HTTPClient` replaces the TypeScript custom `fetch` hook.

These deviations should preserve the same public concepts and ingestion
semantics while fitting Go conventions.

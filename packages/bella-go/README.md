# Bella Go SDK

Server-side Go SDK for recording Bella LLM usage events.

The SDK wraps an LLM call, records timing, status, model, provider, token usage
when supplied by the caller, and returns the provider result unchanged. It does
not send prompts, completions, provider API keys, or raw error messages by
default.

See [CONTRACT.md](CONTRACT.md) for the Go SDK contract mapped from the
TypeScript SDK.

## Install

```sh
go get github.com/seandowdall/bella/packages/bella-go
```

## Quickstart

`BaseURL` is the Bella API URL. For local development use
`http://127.0.0.1:3000`; for hosted or self-hosted deployments use the public
Bella API endpoint for that deployment.

```go
package main

import (
	"context"
	"log"

	bella "github.com/seandowdall/bella/packages/bella-go"
)

func main() {
	ctx := context.Background()

	server, err := bella.NewServer(bella.ServerOptions{
		ClientOptions: bella.ClientOptions{
			APIKey:         "bella_api_key",
			OrganizationID: "organization_id",
			BaseURL:        "http://127.0.0.1:3000",
		},
		DefaultProviderAccountID: "provider_account_id",
		DefaultProvider:          "openai",
	})
	if err != nil {
		log.Fatal(err)
	}

	result, err := bella.TrackLlmCall(ctx, server, bella.TrackLlmCallOptions[string]{
		Model:     "gpt-4.1-mini",
		Operation: "chat.completions.create",
		Call: func(ctx context.Context) (string, error) {
			return callProvider(ctx)
		},
	})
	if err != nil {
		log.Fatal(err)
	}

	log.Println(result)
}

func callProvider(context.Context) (string, error) {
	return "provider result", nil
}
```

## Environment Setup

```go
server, ok, err := bella.NewServerFromEnv()
if err != nil {
	log.Fatal(err)
}
if !ok {
	// Bella is not configured. Call the provider directly.
}
```

Supported environment variables:

```text
BELLA_API_KEY=...
BELLA_API_URL=https://api.example.com
BELLA_ORGANIZATION_ID=...
BELLA_PROVIDER_ACCOUNT_ID=...
BELLA_PROVIDER=openai
BELLA_SDK_FAIL_OPEN=true
BELLA_SDK_CAPTURE_ERROR_MESSAGE=false
```

`BELLA_API_URL` falls back to `BELLA_PUBLIC_API_URL`. `BELLA_PROVIDER` defaults
to `openai`.

For multi-provider services, use environment defaults for the most common
provider or omit provider defaults and pass `Provider` and `ProviderAccountID`
on each wrapped call.

## Fail-Open Behavior

`TrackLlmCall` fails open by default. If the provider call succeeds but Bella
ingestion fails, the provider result is still returned.

Set `FailOpen` to false when constructing the server to make ingestion failures
return an error:

```go
failOpen := false
server, err := bella.NewServer(bella.ServerOptions{
	ClientOptions: bella.ClientOptions{
		APIKey:         "bella_api_key",
		OrganizationID: "organization_id",
	},
	DefaultProviderAccountID: "provider_account_id",
	DefaultProvider:          "openai",
	FailOpen:                 &failOpen,
})
```

Raw error messages are not recorded by default. Set `CaptureErrorMessage` to
true, or provide `ErrorMessageFromError`, when you want to send a redacted error
summary.

## Usage and Cost

The Go SDK does not inspect provider-specific response bodies by default. Supply
small extractor functions when you want to record tokens or cost:

```go
type ProviderResult struct {
	Text         string
	InputTokens  int64
	OutputTokens int64
	CostMicros   int64
}

result, err := bella.TrackLlmCall(ctx, server, bella.TrackLlmCallOptions[ProviderResult]{
	Model:     "gpt-4.1-mini",
	Operation: "chat.completions.create",
	Call: func(ctx context.Context) (ProviderResult, error) {
		return callProvider(ctx)
	},
	UsageFromResult: func(result ProviderResult) *bella.Usage {
		totalTokens := result.InputTokens + result.OutputTokens
		return &bella.Usage{
			InputTokens:  &result.InputTokens,
			OutputTokens: &result.OutputTokens,
			TotalTokens:  &totalTokens,
		}
	},
	CostFromResult: func(result ProviderResult) *bella.Cost {
		return &bella.Cost{
			AmountMicros: result.CostMicros,
			Currency:     "usd",
		}
	},
})
```

## Multi-Provider Calls

Defaults are convenient for a single-provider service. For services that use
more than one provider, pass provider attribution per call:

```go
openAIResult, err := bella.TrackLlmCall(ctx, server, bella.TrackLlmCallOptions[OpenAIResult]{
	ProviderAccountID: "openai_provider_account_id",
	Provider:          "openai",
	Model:             "gpt-4.1-mini",
	Operation:         "chat.completions.create",
	Call: func(ctx context.Context) (OpenAIResult, error) {
		return callOpenAI(ctx)
	},
})
```

```go
anthropicResult, err := bella.TrackLlmCall(ctx, server, bella.TrackLlmCallOptions[AnthropicResult]{
	ProviderAccountID: "anthropic_provider_account_id",
	Provider:          "anthropic",
	Model:             "claude-3-5-sonnet",
	Operation:         "messages.create",
	Call: func(ctx context.Context) (AnthropicResult, error) {
		return callAnthropic(ctx)
	},
})
```

## Direct Event Recording

```go
inputTokens := int64(10)
outputTokens := int64(20)
totalTokens := inputTokens + outputTokens

_, err := client.RecordUsageEvent(ctx, bella.UsageEvent{
	EventID:           bella.CreateEventID("llm"),
	ProviderAccountID: "provider_account_id",
	Provider:          "openai",
	Model:             "gpt-4.1-mini",
	Operation:         "chat.completions.create",
	Status:            bella.UsageStatusSucceeded,
	StartedAt:         startedAt,
	EndedAt:           endedAt,
	Usage: &bella.Usage{
		InputTokens:  &inputTokens,
		OutputTokens: &outputTokens,
		TotalTokens:  &totalTokens,
	},
	Cost: &bella.Cost{
		AmountMicros: 12345,
		Currency:     "usd",
	},
	Metadata: map[string]any{
		"service": "worker",
	},
})
```

Use stable event IDs when retrying the same usage event so Bella can deduplicate
ingestion by organization and event ID.

## Runnable Example

See [examples/openai/main.go](examples/openai/main.go) for a minimal runnable
example with environment-based setup and usage extraction.

# Bella SDK Packages

The SDKs live in this monorepo for the MVP, but each package is structured as a publishable package.

- `@bella/core`: shared transport, event types, and errors.
- `@bella/server`: Node/server SDK for wrapping LLM calls and recording usage.
- `@bella/web`: browser SDK for identity/context and lightweight telemetry.
- `github.com/seandowdall/bella/packages/bella-go`: Go server SDK for wrapping LLM calls and recording usage. See the [Go SDK README](bella-go/README.md) and [minimal Go example](bella-go/examples/openai/main.go).

## Server MVP

```ts
import OpenAI from "openai";
import { createBellaServer, createBellaServerFromEnv } from "@bella/server";

const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });
const bella = createBellaServer({
  apiKey: process.env.BELLA_API_KEY!,
  baseUrl: process.env.BELLA_API_URL,
  organizationId: process.env.BELLA_ORGANIZATION_ID!,
  defaultProviderAccountId: process.env.BELLA_PROVIDER_ACCOUNT_ID!,
  defaultProvider: "openai",
  onIngestionError: (error) => console.warn("Bella ingestion failed", error),
});

const completion = await bella.trackLlmCall({
  model: "gpt-4.1-mini",
  operation: "chat.completions.create",
  call: () =>
    openai.chat.completions.create({
      model: "gpt-4.1-mini",
      messages: [{ role: "user", content: "Hello" }],
    }),
});
```

For hosted dogfooding, prefer environment-based initialization:

```ts
import { createBellaServerFromEnv } from "@bella/server";

const bella = createBellaServerFromEnv();

const result = bella
  ? await bella.trackLlmCall({
      model: "gpt-4.1-mini",
      operation: "chat.completions.create",
      call: () => openai.chat.completions.create(request),
    })
  : await openai.chat.completions.create(request);
```

The server SDK records timing, status, model, provider, and token usage when the provider response includes a standard `usage` object. It does not send prompts, completions, provider API keys, or raw error messages by default.

`trackLlmCall` fails open by default: if Bella ingestion is unavailable after a provider call succeeds, the provider result is still returned. Set `failOpen: false` if you want ingestion failures to throw.

## Hosted Dogfood

For Bella's hosted environment, configure the server-side app or worker that makes LLM calls with:

```sh
BELLA_API_KEY=...
BELLA_API_URL=https://api.your-bella-host.example
BELLA_ORGANIZATION_ID=...
BELLA_PROVIDER_ACCOUNT_ID=...
BELLA_PROVIDER=openai
# Optional. Defaults to true so Bella outages do not break production LLM calls.
BELLA_SDK_FAIL_OPEN=true
# Optional. Defaults to false to avoid leaking provider/client error details.
BELLA_SDK_CAPTURE_ERROR_MESSAGE=false
```

Use a Bella API token that belongs to the Bella team organization. The provider account id should point at the provider account you want dogfood usage attributed to.

## Web MVP

```ts
import { createBellaWeb } from "@bella/web";

const bella = createBellaWeb({
  apiKey: import.meta.env.VITE_BELLA_PUBLIC_TOKEN,
  baseUrl: import.meta.env.VITE_BELLA_API_URL,
  organizationId: import.meta.env.VITE_BELLA_ORGANIZATION_ID,
  providerAccountId: import.meta.env.VITE_BELLA_PROVIDER_ACCOUNT_ID,
  provider: "openai",
});

bella.identify({ userId: "user_123", sessionId: "session_abc" });
await bella.capture("page_view", { route: "/dashboard" });
```

Do not put privileged Bella API tokens in browser code. The web package is ready for a future public/client token flow; for the MVP, prefer server-side ingestion for production LLM usage.

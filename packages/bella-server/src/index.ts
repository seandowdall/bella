export {
  BellaApiError,
  BellaClient,
  createBellaClient,
  createEventId,
  type BellaClientOptions,
  type BellaCost,
  type BellaUsage,
  type BellaUsageEvent,
} from "@bella/core";

import {
  BellaClient,
  createEventId,
  type BellaClientOptions,
  type BellaCost,
  type BellaUsage,
  type BellaUsageEvent,
} from "@bella/core";

export interface BellaServerOptions extends BellaClientOptions {
  defaultProviderAccountId?: string;
  defaultProvider?: string;
  failOpen?: boolean;
  onIngestionError?: (error: unknown, event: BellaUsageEvent) => void;
}

export interface TrackLlmCallOptions<T> {
  providerAccountId?: string;
  provider?: string;
  model?: string;
  operation?: string;
  eventId?: string;
  metadata?: Record<string, unknown>;
  call: () => Promise<T>;
  usage?: (result: T) => BellaUsage | undefined;
  cost?: (result: T) => BellaCost | undefined;
}

export class BellaServer extends BellaClient {
  private readonly defaultProviderAccountId?: string;
  private readonly defaultProvider?: string;
  private readonly failOpen: boolean;
  private readonly onIngestionError?: (error: unknown, event: BellaUsageEvent) => void;

  constructor(options: BellaServerOptions) {
    super(options);
    this.defaultProviderAccountId = options.defaultProviderAccountId;
    this.defaultProvider = options.defaultProvider;
    this.failOpen = options.failOpen ?? true;
    this.onIngestionError = options.onIngestionError;
  }

  async trackLlmCall<T>(options: TrackLlmCallOptions<T>): Promise<T> {
    const providerAccountId = options.providerAccountId ?? this.defaultProviderAccountId;
    const provider = options.provider ?? this.defaultProvider;
    if (!providerAccountId) {
      throw new Error("Bella providerAccountId is required");
    }
    if (!provider) {
      throw new Error("Bella provider is required");
    }

    const eventId = options.eventId ?? createEventId("llm");
    const startedAt = new Date();
    try {
      const result = await options.call();
      await this.safeRecordUsageEvent({
        eventId,
        providerAccountId,
        provider,
        model: options.model ?? modelFromResult(result),
        operation: options.operation ?? "llm.call",
        status: "succeeded",
        startedAt,
        endedAt: new Date(),
        usage: options.usage?.(result) ?? usageFromResult(result),
        cost: options.cost?.(result),
        metadata: options.metadata,
      });
      return result;
    } catch (error) {
      await this.safeRecordUsageEvent({
        eventId,
        providerAccountId,
        provider,
        model: options.model,
        operation: options.operation ?? "llm.call",
        status: "failed",
        startedAt,
        endedAt: new Date(),
        metadata: options.metadata,
        errorMessage: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  }

  private async safeRecordUsageEvent(event: BellaUsageEvent): Promise<void> {
    try {
      await this.recordUsageEvent(event);
    } catch (error) {
      this.onIngestionError?.(error, event);
      if (!this.failOpen) {
        throw error;
      }
    }
  }
}

export function createBellaServer(options: BellaServerOptions): BellaServer {
  return new BellaServer(options);
}

export function createBellaServerFromEnv(env: NodeEnv = readProcessEnv()): BellaServer | undefined {
  const apiKey = env.BELLA_API_KEY;
  const organizationId = env.BELLA_ORGANIZATION_ID;
  if (!apiKey || !organizationId) {
    return undefined;
  }

  return createBellaServer({
    apiKey,
    baseUrl: env.BELLA_API_URL ?? env.BELLA_PUBLIC_API_URL,
    organizationId,
    defaultProviderAccountId: env.BELLA_PROVIDER_ACCOUNT_ID,
    defaultProvider: env.BELLA_PROVIDER ?? "openai",
    failOpen: env.BELLA_SDK_FAIL_OPEN !== "false",
  });
}

function usageFromResult(value: unknown): BellaUsage | undefined {
  if (!isRecord(value) || !isRecord(value.usage)) {
    return undefined;
  }
  return {
    inputTokens: numberField(value.usage, "input_tokens") ?? numberField(value.usage, "prompt_tokens"),
    outputTokens: numberField(value.usage, "output_tokens") ?? numberField(value.usage, "completion_tokens"),
    totalTokens: numberField(value.usage, "total_tokens"),
  };
}

function modelFromResult(value: unknown): string | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const model = value.model;
  return typeof model === "string" ? model : undefined;
}

function numberField(value: Record<string, unknown>, key: string): number | undefined {
  const field = value[key];
  return typeof field === "number" && Number.isFinite(field) ? field : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

interface NodeEnv {
  BELLA_API_KEY?: string;
  BELLA_API_URL?: string;
  BELLA_PUBLIC_API_URL?: string;
  BELLA_ORGANIZATION_ID?: string;
  BELLA_PROVIDER_ACCOUNT_ID?: string;
  BELLA_PROVIDER?: string;
  BELLA_SDK_FAIL_OPEN?: string;
}

function readProcessEnv(): NodeEnv {
  const processLike = globalThis as typeof globalThis & { process?: { env?: NodeEnv } };
  return processLike.process?.env ?? {};
}

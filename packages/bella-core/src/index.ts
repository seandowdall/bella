export type BellaFetch = typeof fetch;

export type BellaUsageStatus = "succeeded" | "failed";

export interface BellaUsage {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
}

export interface BellaCost {
  amountMicros: number;
  currency?: string;
}

export interface BellaUsageEvent {
  eventId: string;
  providerAccountId: string;
  provider: string;
  model?: string;
  operation?: string;
  status: BellaUsageStatus;
  startedAt: Date | string;
  endedAt: Date | string;
  usage?: BellaUsage;
  cost?: BellaCost;
  metadata?: Record<string, unknown>;
  errorMessage?: string;
}

export interface BellaClientOptions {
  apiKey: string;
  baseUrl?: string;
  organizationId: string;
  fetch?: BellaFetch;
}

export interface BellaUsageEventResponse {
  event_id: string;
  accepted: boolean;
}

export class BellaApiError extends Error {
  readonly status: number;
  readonly body: string;

  constructor(status: number, body: string) {
    super(`Bella API request failed with HTTP ${status}${body ? `: ${body}` : ""}`);
    this.name = "BellaApiError";
    this.status = status;
    this.body = body;
  }
}

export class BellaClient {
  readonly baseUrl: string;
  readonly organizationId: string;
  private readonly apiKey: string;
  private readonly fetchImpl: BellaFetch;

  constructor(options: BellaClientOptions) {
    if (!options.apiKey) {
      throw new Error("Bella apiKey is required");
    }
    if (!options.organizationId) {
      throw new Error("Bella organizationId is required");
    }
    this.apiKey = options.apiKey;
    this.organizationId = options.organizationId;
    this.baseUrl = (options.baseUrl ?? "http://127.0.0.1:3000").replace(/\/+$/, "");
    this.fetchImpl = options.fetch ?? globalThis.fetch;
    if (!this.fetchImpl) {
      throw new Error("Bella requires a fetch implementation");
    }
  }

  async recordUsageEvent(event: BellaUsageEvent): Promise<BellaUsageEventResponse> {
    const response = await this.fetchImpl(
      `${this.baseUrl}/v1/organizations/${this.organizationId}/sdk/usage-events`,
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${this.apiKey}`,
          "content-type": "application/json",
        },
        body: JSON.stringify(toWireUsageEvent(event)),
      },
    );

    if (!response.ok) {
      throw new BellaApiError(response.status, await response.text());
    }
    return (await response.json()) as BellaUsageEventResponse;
  }
}

export function createBellaClient(options: BellaClientOptions): BellaClient {
  return new BellaClient(options);
}

export function createEventId(prefix = "evt"): string {
  const random = globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
  return `${prefix}_${random}`;
}

function toWireUsageEvent(event: BellaUsageEvent): Record<string, unknown> {
  return {
    event_id: event.eventId,
    provider_account_id: event.providerAccountId,
    provider: event.provider,
    model: event.model,
    operation: event.operation,
    status: event.status,
    started_at: toIso(event.startedAt),
    ended_at: toIso(event.endedAt),
    usage: event.usage
      ? {
          input_tokens: event.usage.inputTokens,
          output_tokens: event.usage.outputTokens,
          total_tokens: event.usage.totalTokens,
        }
      : undefined,
    cost: event.cost
      ? {
          amount_micros: event.cost.amountMicros,
          currency: event.cost.currency,
        }
      : undefined,
    metadata: event.metadata,
    error_message: event.errorMessage,
  };
}

function toIso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : value;
}

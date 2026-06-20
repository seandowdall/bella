export {
  BellaApiError,
  BellaClient,
  createBellaClient,
  createEventId,
  type BellaClientOptions,
  type BellaUsageEvent,
} from "@bella/core";

import { BellaClient, createEventId, type BellaClientOptions } from "@bella/core";

export interface BellaWebOptions extends BellaClientOptions {
  providerAccountId?: string;
  provider?: string;
}

export interface BellaIdentity {
  userId?: string;
  sessionId?: string;
  accountId?: string;
  properties?: Record<string, unknown>;
}

export class BellaWeb extends BellaClient {
  private identity: BellaIdentity = {};
  private readonly defaultProviderAccountId?: string;
  private readonly defaultProvider?: string;

  constructor(options: BellaWebOptions) {
    super(options);
    this.defaultProviderAccountId = options.providerAccountId;
    this.defaultProvider = options.provider;
  }

  identify(identity: BellaIdentity): void {
    this.identity = { ...this.identity, ...identity };
  }

  async capture(name: string, properties: Record<string, unknown> = {}): Promise<void> {
    const providerAccountId = this.defaultProviderAccountId;
    const provider = this.defaultProvider;
    if (!providerAccountId || !provider) {
      return;
    }

    const now = new Date();
    await this.recordUsageEvent({
      eventId: createEventId("web"),
      providerAccountId,
      provider,
      operation: `web.${name}`,
      status: "succeeded",
      startedAt: now,
      endedAt: now,
      metadata: {
        ...properties,
        bella_identity: this.identity,
        url: globalThis.location?.href,
      },
    });
  }
}

export function createBellaWeb(options: BellaWebOptions): BellaWeb {
  return new BellaWeb(options);
}

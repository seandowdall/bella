"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { ArrowRightIcon, ClockIcon } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { IntegrationIcon } from "@/components/integration-icon";
import { getIntegrations } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type { Integration } from "@/lib/dashboard-types";

type IntegrationCatalogItem = {
  id: string;
  name: string;
  description: string;
  href?: string;
  capabilities: string[];
  plannedSummary?: string;
  state: "available" | "planned";
};

const catalog: IntegrationCatalogItem[] = [
  {
    id: "posthog",
    name: "PostHog",
    description: "Create Bella incidents from alerts, exception events, and production signals.",
    href: "/integrations/posthog",
    capabilities: ["Webhooks", "API ingestion", "Incident candidates"],
    state: "available",
  },
  {
    id: "sentry",
    name: "Sentry",
    description: "Ingest issue regressions, releases, and error spikes from Sentry projects.",
    capabilities: ["Webhooks", "Errors", "Releases"],
    plannedSummary: "Planned ingestion for issue regressions, release markers, and error-volume alerts.",
    state: "planned",
  },
  {
    id: "linear",
    name: "Linear",
    description: "Create, link, and sync incident follow-up issues with engineering workflows.",
    capabilities: ["Issues", "Backlinks", "Status sync"],
    plannedSummary: "Planned workflow sync for follow-up issues, ownership, and incident backlinks.",
    state: "planned",
  },
  {
    id: "slack",
    name: "Slack",
    description: "Route incident updates and capture operational signals from Slack channels.",
    capabilities: ["Notifications", "Channels", "Commands"],
    plannedSummary: "Planned incident updates, channel routing, and command-driven triage.",
    state: "planned",
  },
];

export default function IntegrationsPage() {
  const { selectedOrganizationId } = useAuth();
  const [integrations, setIntegrations] = useState<Integration[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!selectedOrganizationId) return;
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setError("");
      try {
        const nextIntegrations = await getIntegrations(selectedOrganizationId);
        if (!cancelled) setIntegrations(nextIntegrations);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Could not load integrations.");
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [selectedOrganizationId]);

  const integrationsByType = useMemo(
    () => new Map(integrations.map((integration) => [integration.integration_type, integration])),
    [integrations],
  );

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-semibold tracking-tight">Integrations</h1>
        <p className="text-sm text-muted-foreground">
          Connect operational systems that can create Bella incidents and enrich investigations.
        </p>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {loading ? (
        <div className="grid gap-4 lg:grid-cols-2">
          {catalog.map((item) => (
            <Skeleton key={item.id} className="h-56 rounded-xl" />
          ))}
        </div>
      ) : (
        <div className="grid gap-4 lg:grid-cols-2">
          {catalog.map((item) => (
            <IntegrationCard
              key={item.id}
              item={item}
              integration={integrationsByType.get(item.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function IntegrationCard({
  item,
  integration,
}: {
  item: IntegrationCatalogItem;
  integration: Integration | undefined;
}) {
  const configured = Boolean(integration);
  const apiConfigured = Boolean(integration?.api_token_fingerprint);
  const webhookConfigured = Boolean(integration?.credential_fingerprint);
  const needsAttention = item.id === "posthog" && configured && (!apiConfigured || !webhookConfigured);
  const statusLabel = item.state === "planned"
    ? "Planned"
    : needsAttention
      ? "Needs setup"
      : configured
        ? "Connected"
        : "Not connected";
  const actionLabel = configured ? (needsAttention ? "Review setup" : "Configure") : "Connect";

  return (
    <Card className="flex min-h-56 flex-col">
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3">
            <IntegrationIcon integration={item.id} name={item.name} />
            <div className="flex flex-col gap-1">
              <CardTitle>{item.name}</CardTitle>
              <CardDescription>{item.description}</CardDescription>
            </div>
          </div>
          <Badge variant={needsAttention ? "destructive" : configured ? "secondary" : "outline"}>
            {statusLabel}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col justify-between gap-5">
        <div className="flex flex-wrap gap-2">
          {item.capabilities.map((capability) => (
            <Badge key={capability} variant="outline">
              {capability}
            </Badge>
          ))}
        </div>

        {item.id === "posthog" ? (
          <div className="grid gap-2 text-sm text-muted-foreground sm:grid-cols-2">
            <div>Webhook {webhookConfigured ? "configured" : "not configured"}</div>
            <div>API sync {apiConfigured ? "configured" : "needs token"}</div>
            <div className="sm:col-span-2">Last updated {formatDateTime(integration?.updated_at)}</div>
          </div>
        ) : (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <ClockIcon />
            {item.plannedSummary}
          </div>
        )}

        <div>
          {item.href ? (
            <Button asChild variant={configured ? "outline" : "default"}>
              <Link href={item.href}>
                {actionLabel}
                <ArrowRightIcon data-icon="inline-end" />
              </Link>
            </Button>
          ) : (
            <Button variant="outline" disabled>
              Coming soon
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function formatDateTime(value: string | undefined) {
  if (!value) return "never";
  return new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

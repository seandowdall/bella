"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { ArrowRightIcon, CheckIcon, ClockIcon, CopyIcon, ExternalLinkIcon } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { IntegrationIcon } from "@/components/integration-icon";
import { createSlackInstallUrl, getIntegrations, getSlackStatus } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type { Integration, SlackStatus } from "@/lib/dashboard-types";

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
    id: "slack",
    name: "Slack",
    description: "Send incidents to the Slack channels where your team works.",
    capabilities: ["Notifications", "Channels"],
    state: "available",
  },
  {
    id: "github",
    name: "GitHub",
    description: "Install Bella as a GitHub App so agents can read repo context and open PRs.",
    href: "/integrations/github",
    capabilities: ["Repositories", "Pull requests", "Agent context"],
    state: "available",
  },
  {
    id: "sentry",
    name: "Sentry",
    description: "Ingest issue regressions, releases, and error spikes from Sentry projects.",
    capabilities: ["Webhooks", "Errors", "Releases"],
    plannedSummary:
      "Planned ingestion for issue regressions, release markers, and error-volume alerts.",
    state: "planned",
  },
  {
    id: "linear",
    name: "Linear",
    description: "Create, link, and sync incident follow-up issues with engineering workflows.",
    capabilities: ["Issues", "Backlinks", "Status sync"],
    plannedSummary:
      "Planned workflow sync for follow-up issues, ownership, and incident backlinks.",
    state: "planned",
  },
];

export default function IntegrationsPage() {
  const { selectedOrganizationId } = useAuth();
  const [integrations, setIntegrations] = useState<Integration[]>([]);
  const [slackStatus, setSlackStatus] = useState<SlackStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [installingSlack, setInstallingSlack] = useState(false);
  const [error, setError] = useState("");
  const [oauthError, setOauthError] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const url = new URL(window.location.href);
    const slackResult = url.searchParams.get("slack");
    if (slackResult === "workspace_conflict") {
      setOauthError("This Slack workspace is already connected to another Bella organization.");
      url.searchParams.delete("slack");
      window.history.replaceState({}, "", url);
    }
  }, []);

  useEffect(() => {
    if (!selectedOrganizationId) return;
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setError("");
      try {
        const [nextIntegrations, nextSlackStatus] = await Promise.all([
          getIntegrations(selectedOrganizationId),
          getSlackStatus(selectedOrganizationId),
        ]);
        if (!cancelled) {
          setIntegrations(nextIntegrations);
          setSlackStatus(nextSlackStatus);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Could not load integrations.");
        }
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
  const workspaceConnected = slackStatus?.workspace?.status === "connected";

  const installSlack = async () => {
    if (!selectedOrganizationId) return;
    setInstallingSlack(true);
    setError("");
    setOauthError("");
    try {
      const result = await createSlackInstallUrl({
        organizationId: selectedOrganizationId,
        returnTo: window.location.href,
      });
      window.location.assign(result.install_url);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not start Slack install.");
      setInstallingSlack(false);
    }
  };

  const copyInvite = async () => {
    await navigator.clipboard.writeText("/invite @Bella");
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-semibold tracking-tight">Integrations</h1>
        <p className="text-sm text-muted-foreground">
          Connect operational systems that can create Bella incidents and enrich investigations.
        </p>
      </div>

      {(oauthError || error) && (
        <Alert variant="destructive">
          <AlertDescription>{oauthError || error}</AlertDescription>
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
          {catalog.map((item) =>
            item.id === "slack" ? (
              <SlackCard
                key={item.id}
                item={item}
                connected={workspaceConnected}
                workspaceName={slackStatus?.workspace?.team_name}
                installing={installingSlack}
                copied={copied}
                onInstall={() => void installSlack()}
                onCopy={() => void copyInvite()}
              />
            ) : (
              <IntegrationCard
                key={item.id}
                item={item}
                integration={integrationsByType.get(item.id)}
              />
            ),
          )}
        </div>
      )}
    </div>
  );
}

function SlackCard({
  item,
  connected,
  workspaceName,
  installing,
  copied,
  onInstall,
  onCopy,
}: {
  item: IntegrationCatalogItem;
  connected: boolean;
  workspaceName: string | undefined;
  installing: boolean;
  copied: boolean;
  onInstall: () => void;
  onCopy: () => void;
}) {
  return (
    <Card className="flex min-h-56 flex-col">
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3">
            <IntegrationIcon integration="slack" name="Slack" />
            <div className="flex flex-col gap-1">
              <CardTitle>{item.name}</CardTitle>
              <CardDescription>{item.description}</CardDescription>
            </div>
          </div>
          <Badge variant={connected ? "secondary" : "outline"}>
            {connected ? "Connected" : "Not connected"}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col justify-between gap-5">
        {connected ? (
          <div className="flex flex-col gap-4">
            <div>
              <p className="text-sm font-medium">{workspaceName ?? "Slack workspace"}</p>
              <p className="text-sm text-muted-foreground">
                Bella is installed. Reconnect to use a different workspace.
              </p>
            </div>
            <Field>
              <FieldLabel htmlFor="slack-invite-command">Add Bella to a channel</FieldLabel>
              <InputGroup>
                <InputGroupInput
                  id="slack-invite-command"
                  value="/invite @Bella"
                  readOnly
                  className="font-mono"
                />
                <Tooltip>
                  <TooltipTrigger asChild>
                    <InputGroupAddon
                      align="inline-end"
                      aria-label="Copy Slack invite command"
                      onClick={onCopy}
                    >
                      {copied ? <CheckIcon /> : <CopyIcon />}
                    </InputGroupAddon>
                  </TooltipTrigger>
                  <TooltipContent>{copied ? "Copied" : "Copy command"}</TooltipContent>
                </Tooltip>
              </InputGroup>
              <FieldDescription>
                Run this command in each channel that should receive incidents.
              </FieldDescription>
            </Field>
          </div>
        ) : (
          <div className="flex flex-wrap gap-2">
            {item.capabilities.map((capability) => (
              <Badge key={capability} variant="outline">
                {capability}
              </Badge>
            ))}
          </div>
        )}
        <div>
          <Button type="button" variant={connected ? "outline" : "default"} onClick={onInstall}>
            {installing ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <ExternalLinkIcon data-icon="inline-start" />
            )}
            {connected ? "Reconnect" : "Install Slack"}
          </Button>
        </div>
      </CardContent>
    </Card>
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
  const needsAttention =
    item.id === "posthog" && configured && (!apiConfigured || !webhookConfigured);
  const statusLabel =
    item.state === "planned"
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
            <div className="sm:col-span-2">
              Last updated {formatDateTime(integration?.updated_at)}
            </div>
          </div>
        ) : item.id === "github" ? (
          <div className="grid gap-2 text-sm text-muted-foreground sm:grid-cols-2">
            <div>App {configured ? "installed" : "not installed"}</div>
            <div>PR access {configured ? "available" : "needs install"}</div>
            <div className="sm:col-span-2">
              Last updated {formatDateTime(integration?.updated_at)}
            </div>
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

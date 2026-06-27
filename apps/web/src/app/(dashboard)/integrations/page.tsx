"use client";

import { useEffect, useMemo, useState } from "react";
import {
  CheckIcon,
  CopyIcon,
  ExternalLinkIcon,
  PlayIcon,
  RotateCcwIcon,
  ShieldCheckIcon,
} from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import {
  checkPosthogIntegration,
  connectPosthogIntegration,
  getIntegrations,
  syncPosthogIntegration,
} from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type {
  Integration,
  PosthogConnection,
  PosthogConnectionCheck,
  PosthogSyncOutcome,
} from "@/lib/dashboard-types";

const publicApiUrl = process.env.NEXT_PUBLIC_BELLA_PUBLIC_API_URL ?? "http://127.0.0.1:3000";

export default function IntegrationsPage() {
  const { selectedOrganizationId } = useAuth();
  const [integrations, setIntegrations] = useState<Integration[]>([]);
  const [connection, setConnection] = useState<PosthogConnection | null>(null);
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [checking, setChecking] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState("");
  const [posthogHost, setPosthogHost] = useState("https://us.posthog.com");
  const [posthogProjectId, setPosthogProjectId] = useState("");
  const [posthogApiToken, setPosthogApiToken] = useState("");
  const [checkResult, setCheckResult] = useState<PosthogConnectionCheck | null>(null);
  const [syncResult, setSyncResult] = useState<PosthogSyncOutcome | null>(null);

  const posthog = integrations.find((integration) => integration.integration_type === "posthog");
  const apiConfigured = Boolean(posthog?.api_token_fingerprint);
  const webhookUrl = useMemo(() => {
    if (!selectedOrganizationId) return "";
    return `${publicApiUrl.replace(/\/$/, "")}/v1/organizations/${selectedOrganizationId}/webhooks/posthog`;
  }, [selectedOrganizationId]);

  useEffect(() => {
    if (!selectedOrganizationId) return;
    let cancelled = false;
    const load = async () => {
      setError("");
      try {
        const nextIntegrations = await getIntegrations(selectedOrganizationId);
        if (!cancelled) setIntegrations(nextIntegrations);
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

  useEffect(() => {
    if (!posthog) return;
    const savedHost = stringMetadata(posthog.metadata, "posthog_host");
    const savedProjectId = stringMetadata(posthog.metadata, "posthog_project_id");
    if (savedHost) setPosthogHost(savedHost);
    if (savedProjectId) setPosthogProjectId(savedProjectId);
  }, [posthog]);

  const connectPosthog = async () => {
    if (!selectedOrganizationId) return;
    setConnecting(true);
    setError("");
    setCheckResult(null);
    setSyncResult(null);
    try {
      const nextConnection = await connectPosthogIntegration({
        organizationId: selectedOrganizationId,
        displayName: posthog?.display_name ?? "PostHog",
        posthogHost,
        posthogProjectId,
        apiToken: posthogApiToken,
      });
      setConnection(nextConnection);
      setPosthogApiToken("");
      setIntegrations((current) => [
        ...current.filter((item) => item.id !== nextConnection.integration.id),
        nextConnection.integration,
      ]);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not connect PostHog.");
    } finally {
      setConnecting(false);
    }
  };

  const checkPosthog = async () => {
    if (!selectedOrganizationId) return;
    setChecking(true);
    setError("");
    setCheckResult(null);
    try {
      setCheckResult(await checkPosthogIntegration(selectedOrganizationId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not verify PostHog API access.");
    } finally {
      setChecking(false);
    }
  };

  const syncPosthog = async () => {
    if (!selectedOrganizationId) return;
    setSyncing(true);
    setError("");
    setSyncResult(null);
    try {
      setSyncResult(await syncPosthogIntegration(selectedOrganizationId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not sync PostHog signals.");
    } finally {
      setSyncing(false);
    }
  };

  const copy = async (label: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(label);
    window.setTimeout(() => setCopied(""), 1600);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-semibold tracking-tight">Integrations</h1>
        <p className="text-muted-foreground text-sm">
          Connect operational systems that can create Bella incidents.
        </p>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div className="flex flex-col gap-1">
              <CardTitle>PostHog</CardTitle>
              <CardDescription>
                Receive error tracking alerts, exception events, and product signals as Bella
                incidents.
              </CardDescription>
            </div>
            <Badge variant={posthog ? "secondary" : "outline"}>
              {posthog ? "Connected" : "Not connected"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          {loading ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner />
              Loading integrations
            </div>
          ) : (
            <>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="posthog-webhook-url">Webhook URL</FieldLabel>
                  <div className="flex gap-2">
                    <Input id="posthog-webhook-url" value={webhookUrl} readOnly />
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => void copy("url", webhookUrl)}
                    >
                      {copied === "url" ? (
                        <CheckIcon data-icon="inline-start" />
                      ) : (
                        <CopyIcon data-icon="inline-start" />
                      )}
                      Copy
                    </Button>
                  </div>
                  <FieldDescription>
                    Set this as the HTTP webhook destination in PostHog. For deployed self-hosting,
                    set NEXT_PUBLIC_BELLA_PUBLIC_API_URL to your public API origin.
                  </FieldDescription>
                </Field>

                <Field>
                  <FieldLabel htmlFor="posthog-secret">Webhook secret</FieldLabel>
                  <div className="flex gap-2">
                    <Input
                      id="posthog-secret"
                      value={
                        connection?.webhook_secret ??
                        (posthog?.credential_fingerprint
                          ? `Saved, fingerprint ${posthog.credential_fingerprint}`
                          : "Connect PostHog to generate a secret")
                      }
                      readOnly
                    />
                    {connection?.webhook_secret && (
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => void copy("secret", connection.webhook_secret)}
                      >
                        {copied === "secret" ? (
                          <CheckIcon data-icon="inline-start" />
                        ) : (
                          <CopyIcon data-icon="inline-start" />
                        )}
                        Copy
                      </Button>
                    )}
                  </div>
                  <FieldDescription>
                    Send this as Authorization: Bearer, X-Bella-Webhook-Secret, or
                    X-PostHog-Webhook-Secret. The full secret is shown only when generated or
                    rotated.
                  </FieldDescription>
                </Field>
              </FieldGroup>

              <Separator />

              <div className="flex flex-col gap-4">
                <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-sm font-medium">Production API ingestion</h2>
                    <p className="text-sm text-muted-foreground">
                      Pull bounded read-only PostHog signals into incident candidates.
                    </p>
                  </div>
                  <Badge variant={apiConfigured ? "secondary" : "outline"}>
                    {apiConfigured ? "API token saved" : "API token missing"}
                  </Badge>
                </div>

                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor="posthog-host">PostHog host</FieldLabel>
                    <Input
                      id="posthog-host"
                      value={posthogHost}
                      onChange={(event) => setPosthogHost(event.target.value)}
                      placeholder="https://us.posthog.com"
                    />
                    <FieldDescription>
                      Use the app origin for US Cloud, EU Cloud, or your self-hosted PostHog.
                    </FieldDescription>
                  </Field>

                  <Field>
                    <FieldLabel htmlFor="posthog-project-id">Project ID</FieldLabel>
                    <Input
                      id="posthog-project-id"
                      value={posthogProjectId}
                      onChange={(event) => setPosthogProjectId(event.target.value)}
                      placeholder="12345"
                    />
                  </Field>

                  <Field>
                    <FieldLabel htmlFor="posthog-api-token">API token</FieldLabel>
                    <Input
                      id="posthog-api-token"
                      type="password"
                      value={posthogApiToken}
                      onChange={(event) => setPosthogApiToken(event.target.value)}
                      placeholder={
                        posthog?.api_token_fingerprint
                          ? `Saved, fingerprint ${posthog.api_token_fingerprint}`
                          : "PostHog personal API key"
                      }
                    />
                    <FieldDescription>
                      Requires PostHog `query:read`. Leave blank to keep the saved token.
                    </FieldDescription>
                  </Field>
                </FieldGroup>

                <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
                  <Button type="button" onClick={() => void connectPosthog()} disabled={connecting}>
                    {connecting ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <RotateCcwIcon data-icon="inline-start" />
                    )}
                    Save and rotate secret
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void checkPosthog()}
                    disabled={checking || !apiConfigured}
                  >
                    {checking ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <ShieldCheckIcon data-icon="inline-start" />
                    )}
                    Check connection
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void syncPosthog()}
                    disabled={syncing || !apiConfigured}
                  >
                    {syncing ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <PlayIcon data-icon="inline-start" />
                    )}
                    Sync now
                  </Button>
                </div>

                {(checkResult || syncResult) && (
                  <div className="rounded-lg bg-muted p-4 text-sm">
                    {checkResult && (
                      <p>
                        Connection verified for project {checkResult.posthog_project_id}; observed{" "}
                        {checkResult.observed_rows} recent row
                        {checkResult.observed_rows === 1 ? "" : "s"}.
                      </p>
                    )}
                    {syncResult && (
                      <p>
                        Sync {syncResult.sync_run_id} saw {syncResult.signals_seen} signal
                        {syncResult.signals_seen === 1 ? "" : "s"}, upserted{" "}
                        {syncResult.signals_upserted}, and created{" "}
                        {syncResult.incident_candidates_created} candidate
                        {syncResult.incident_candidates_created === 1 ? "" : "s"}.
                      </p>
                    )}
                  </div>
                )}
              </div>
            </>
          )}
        </CardContent>
        <CardFooter className="justify-between gap-3">
          <Button asChild variant="ghost" size="sm">
            <a
              href="https://posthog.com/docs/error-tracking/alerts"
              target="_blank"
              rel="noreferrer"
            >
              <ExternalLinkIcon data-icon="inline-start" />
              PostHog alert docs
            </a>
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}

function stringMetadata(metadata: Record<string, unknown>, key: string) {
  const value = metadata[key];
  return typeof value === "string" ? value : "";
}

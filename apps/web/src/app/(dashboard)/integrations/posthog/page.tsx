"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  ArrowLeftIcon,
  CheckIcon,
  CircleIcon,
  CopyIcon,
  ExternalLinkIcon,
  PlayIcon,
  RotateCcwIcon,
  SaveIcon,
  ShieldCheckIcon,
  Trash2Icon,
} from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { IntegrationIcon } from "@/components/integration-icon";
import {
  checkPosthogIntegration,
  deletePosthogIntegration,
  getIntegrations,
  rotatePosthogWebhookSecret,
  savePosthogSettings,
  syncPosthogIntegration,
} from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type {
  Integration,
  PosthogConnectionCheck,
  PosthogSecretRotation,
  PosthogSyncOutcome,
} from "@/lib/dashboard-types";

const publicApiUrl = process.env.NEXT_PUBLIC_BELLA_PUBLIC_API_URL ?? "http://127.0.0.1:3000";

export default function PosthogIntegrationPage() {
  const router = useRouter();
  const { selectedOrganizationId } = useAuth();
  const [posthog, setPosthog] = useState<Integration | null>(null);
  const [rotation, setRotation] = useState<PosthogSecretRotation | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [checking, setChecking] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [confirmRotateOpen, setConfirmRotateOpen] = useState(false);
  const [confirmDisconnectOpen, setConfirmDisconnectOpen] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [copied, setCopied] = useState("");
  const [posthogHost, setPosthogHost] = useState("https://us.posthog.com");
  const [posthogProjectId, setPosthogProjectId] = useState("");
  const [posthogApiToken, setPosthogApiToken] = useState("");
  const [checkResult, setCheckResult] = useState<PosthogConnectionCheck | null>(null);
  const [syncResult, setSyncResult] = useState<PosthogSyncOutcome | null>(null);

  const apiConfigured = Boolean(posthog?.api_token_fingerprint);
  const webhookConfigured = Boolean(posthog?.credential_fingerprint);
  const integrationConfigured = apiConfigured && webhookConfigured;
  const primaryStatus = integrationConfigured
    ? "Connected"
    : posthog
      ? "Needs setup"
      : "Not connected";
  const nextAction = !webhookConfigured
    ? "Generate a webhook secret"
    : !apiConfigured
      ? "Add a read-only API token"
      : "Run a sync or check access";
  const webhookUrl = useMemo(() => {
    if (!selectedOrganizationId) return "";
    return `${publicApiUrl.replace(/\/$/, "")}/v1/organizations/${selectedOrganizationId}/webhooks/posthog`;
  }, [selectedOrganizationId]);

  useEffect(() => {
    if (!selectedOrganizationId) return;
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setError("");
      try {
        const nextIntegrations = await getIntegrations(selectedOrganizationId);
        const nextPosthog =
          nextIntegrations.find((integration) => integration.integration_type === "posthog") ??
          null;
        if (!cancelled) {
          const savedHost = nextPosthog ? stringMetadata(nextPosthog.metadata, "posthog_host") : "";
          const savedProjectId = nextPosthog
            ? stringMetadata(nextPosthog.metadata, "posthog_project_id")
            : "";
          if (savedHost) setPosthogHost(savedHost);
          if (savedProjectId) setPosthogProjectId(savedProjectId);
          setPosthog(nextPosthog);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Could not load PostHog.");
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [selectedOrganizationId]);

  const saveSettings = async () => {
    if (!selectedOrganizationId) return;
    setSaving(true);
    setError("");
    setNotice("");
    setCheckResult(null);
    setSyncResult(null);
    try {
      const integration = await savePosthogSettings({
        organizationId: selectedOrganizationId,
        displayName: posthog?.display_name ?? "PostHog",
        posthogHost,
        posthogProjectId,
        apiToken: posthogApiToken,
      });
      setPosthog(integration);
      setPosthogApiToken("");
      setNotice("PostHog API settings saved. Webhook secret was not changed.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save PostHog settings.");
    } finally {
      setSaving(false);
    }
  };

  const rotateSecret = async () => {
    if (!selectedOrganizationId) return;
    setRotating(true);
    setError("");
    setNotice("");
    try {
      const nextRotation = await rotatePosthogWebhookSecret(selectedOrganizationId);
      setRotation(nextRotation);
      setPosthog(nextRotation.integration);
      setNotice("Webhook secret generated. Copy it now; Bella only shows it once.");
      setConfirmRotateOpen(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not rotate PostHog webhook secret.");
    } finally {
      setRotating(false);
    }
  };

  const disconnectPosthog = async () => {
    if (!selectedOrganizationId) return;
    setDisconnecting(true);
    setError("");
    setNotice("");
    try {
      await deletePosthogIntegration(selectedOrganizationId);
      setConfirmDisconnectOpen(false);
      router.push("/integrations");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not disconnect PostHog.");
    } finally {
      setDisconnecting(false);
    }
  };

  const checkPosthog = async () => {
    if (!selectedOrganizationId) return;
    setChecking(true);
    setError("");
    setNotice("");
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
    setNotice("");
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
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-4">
        <Button asChild variant="ghost" size="sm" className="w-fit">
          <Link href="/integrations">
            <ArrowLeftIcon data-icon="inline-start" />
            Integrations
          </Link>
        </Button>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex items-start gap-3">
            <IntegrationIcon integration="posthog" name="PostHog" />
            <div className="flex flex-col gap-1">
              <h1 className="text-2xl font-semibold tracking-tight">PostHog</h1>
              <p className="text-sm text-muted-foreground">
                Receive alert webhooks and optionally pull read-only production signals.
              </p>
            </div>
          </div>
          <Badge variant={integrationConfigured ? "secondary" : "outline"}>{primaryStatus}</Badge>
        </div>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
      {notice && (
        <Alert>
          <AlertDescription>{notice}</AlertDescription>
        </Alert>
      )}

      {loading ? (
        <Card>
          <CardContent className="flex items-center gap-2 py-10 text-sm text-muted-foreground">
            <Spinner />
            Loading PostHog integration
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <div className="flex flex-col gap-4">
            <Card>
              <CardHeader>
                <CardTitle>Setup progress</CardTitle>
                <CardDescription>
                  Bella can track generated credentials and API checks. Adding the webhook URL in
                  PostHog is a manual step unless an agent completes it for you.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                <SetupStep done={webhookConfigured} label="Webhook secret generated" />
                <SetupStep done={false} label="Webhook URL added in PostHog" note="External" />
                <SetupStep done={apiConfigured} label="Read-only API token saved" />
                <SetupStep done={Boolean(checkResult)} label="API access checked" />
                <SetupStep done={Boolean(syncResult)} label="Initial sync run" />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Alert webhooks</CardTitle>
                <CardDescription>Paste these values into a PostHog alert webhook.</CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-4">
                <KeyValueRow
                  label="Webhook URL"
                  value={webhookUrl}
                  action={
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
                  }
                />
                <KeyValueRow
                  label="Webhook secret"
                  value={
                    rotation?.webhook_secret ??
                    (posthog?.credential_fingerprint
                      ? `Saved · ${posthog.credential_fingerprint}`
                      : "Not generated")
                  }
                  action={
                    rotation?.webhook_secret ? (
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => void copy("secret", rotation.webhook_secret)}
                      >
                        {copied === "secret" ? (
                          <CheckIcon data-icon="inline-start" />
                        ) : (
                          <CopyIcon data-icon="inline-start" />
                        )}
                        Copy
                      </Button>
                    ) : !webhookConfigured ? (
                      <Button type="button" onClick={() => setConfirmRotateOpen(true)}>
                        <RotateCcwIcon data-icon="inline-start" />
                        Generate
                      </Button>
                    ) : null
                  }
                />
              </CardContent>
              <CardFooter>
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

            <Card>
              <CardHeader>
                <CardTitle>Signal sync</CardTitle>
                <CardDescription>Import recent production signals from PostHog.</CardDescription>
              </CardHeader>
              <CardContent>
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor="posthog-host">Host</FieldLabel>
                    <Input
                      id="posthog-host"
                      value={posthogHost}
                      onChange={(event) => setPosthogHost(event.target.value)}
                      placeholder="https://us.posthog.com"
                    />
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
                          ? `Saved · ${posthog.api_token_fingerprint}`
                          : "PostHog personal API key"
                      }
                      autoComplete="off"
                    />
                    <FieldDescription>
                      Requires `query:read`. Leave blank to keep saved token.
                    </FieldDescription>
                  </Field>
                </FieldGroup>

                <Separator className="my-5" />

                <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
                  <Button type="button" onClick={() => void saveSettings()} disabled={saving}>
                    {saving ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <SaveIcon data-icon="inline-start" />
                    )}
                    Save changes
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
                    Check access
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
              </CardContent>
            </Card>
          </div>

          <div className="flex flex-col gap-4">
            <Card>
              <CardHeader>
                <CardTitle>{primaryStatus}</CardTitle>
                <CardDescription>Next: {nextAction}</CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                <StatusRow
                  label="Alert webhooks"
                  value={webhookConfigured ? "Configured" : "Missing secret"}
                />
                <StatusRow
                  label="Signal sync"
                  value={apiConfigured ? "Configured" : "Needs token"}
                />
                <StatusRow label="Last updated" value={formatDateTime(posthog?.updated_at)} />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Recent activity</CardTitle>
                <CardDescription>Operational checks from this session.</CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                <StatusRow
                  label="API check"
                  value={
                    checkResult
                      ? `${checkResult.observed_rows} recent row${checkResult.observed_rows === 1 ? "" : "s"}`
                      : "Not checked"
                  }
                />
                <StatusRow
                  label="Sync"
                  value={
                    syncResult
                      ? `${syncResult.signals_seen} seen · ${syncResult.signals_upserted} upserted`
                      : "Not run"
                  }
                />
                <StatusRow
                  label="Candidates"
                  value={syncResult ? String(syncResult.incident_candidates_created) : "Not run"}
                />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Advanced</CardTitle>
                <CardDescription>Credential rotation and removal.</CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                <Button type="button" variant="outline" onClick={() => setConfirmRotateOpen(true)}>
                  <RotateCcwIcon data-icon="inline-start" />
                  {webhookConfigured ? "Rotate webhook secret" : "Generate webhook secret"}
                </Button>
                {posthog && (
                  <Button
                    type="button"
                    variant="destructive"
                    onClick={() => setConfirmDisconnectOpen(true)}
                  >
                    <Trash2Icon data-icon="inline-start" />
                    Disconnect PostHog
                  </Button>
                )}
              </CardContent>
            </Card>
          </div>
        </div>
      )}

      <AlertDialog open={confirmRotateOpen} onOpenChange={setConfirmRotateOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {webhookConfigured
                ? "Rotate PostHog webhook secret?"
                : "Generate PostHog webhook secret?"}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {webhookConfigured
                ? "Rotating the secret will require updating the secret configured in PostHog alert destinations. Existing webhook deliveries using the old secret will stop authenticating."
                : "Bella will generate a webhook secret and show it once. Copy it into the PostHog alert destination configuration before leaving this page."}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={rotating}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={rotating}
              onClick={(event) => {
                event.preventDefault();
                void rotateSecret();
              }}
            >
              {rotating && <Spinner data-icon="inline-start" />}
              {webhookConfigured ? "Rotate secret" : "Generate secret"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmDisconnectOpen} onOpenChange={setConfirmDisconnectOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Disconnect PostHog?</AlertDialogTitle>
            <AlertDialogDescription>
              This removes the PostHog integration, encrypted webhook secret, API token, and sync
              checkpoints. Historical Bella incidents and signals are preserved.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={disconnecting}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={disconnecting}
              onClick={(event) => {
                event.preventDefault();
                void disconnectPosthog();
              }}
            >
              {disconnecting && <Spinner data-icon="inline-start" />}
              Disconnect PostHog
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function stringMetadata(metadata: Record<string, unknown>, key: string) {
  const value = metadata[key];
  return typeof value === "string" ? value : "";
}

function SetupStep({ done, label, note }: { done: boolean; label: string; note?: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border px-3 py-2">
      <div className="flex items-center gap-2 text-sm">
        {done ? (
          <CheckIcon className="text-success" />
        ) : (
          <CircleIcon className="text-muted-foreground" />
        )}
        <span>{label}</span>
      </div>
      <Badge variant={done ? "secondary" : "outline"}>{done ? "Done" : (note ?? "Todo")}</Badge>
    </div>
  );
}

function KeyValueRow({
  label,
  value,
  action,
}: {
  label: string;
  value: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
      <div className="min-w-32 text-sm font-medium">{label}</div>
      <div className="min-w-0 flex-1 rounded-lg border bg-muted px-3 py-2 text-sm">
        <span className="block truncate">{value}</span>
      </div>
      {action}
    </div>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right font-medium">{value}</span>
    </div>
  );
}

function formatDateTime(value: string | undefined) {
  if (!value) return "Never";
  return new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

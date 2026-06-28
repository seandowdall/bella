"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeftIcon,
  CheckIcon,
  CopyIcon,
  ExternalLinkIcon,
  GitPullRequestIcon,
  RefreshCwIcon,
  ShieldCheckIcon,
  Trash2Icon,
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
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { IntegrationIcon } from "@/components/integration-icon";
import {
  deleteGithubIntegration,
  getGithubInstallUrl,
  getGithubRepositories,
  getIntegrations,
} from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type { GithubRepository, Integration } from "@/lib/dashboard-types";

const publicApiUrl = process.env.NEXT_PUBLIC_BELLA_PUBLIC_API_URL ?? "http://127.0.0.1:3000";

export default function GithubIntegrationPage() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const { selectedOrganization, selectedOrganizationId } = useAuth();
  const [github, setGithub] = useState<Integration | null>(null);
  const [repositories, setRepositories] = useState<GithubRepository[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [copied, setCopied] = useState("");

  const canManage =
    selectedOrganization?.role === "owner" || selectedOrganization?.role === "admin";
  const connected = github?.status === "connected";
  const installUrl = selectedOrganizationId ? getGithubInstallUrl(selectedOrganizationId) : "";
  const setupStatus = searchParams.get("github");
  const callbackUrl = useMemo(
    () => `${publicApiUrl.replace(/\/$/, "")}/v1/integrations/github/callback`,
    [],
  );
  const webhookUrl = useMemo(() => `${publicApiUrl.replace(/\/$/, "")}/v1/github/webhook`, []);

  useEffect(() => {
    if (setupStatus === "connected") {
      setNotice("GitHub App installed. Repository access has been refreshed.");
      router.replace("/integrations/github");
    } else if (setupStatus === "cancelled") {
      setNotice("GitHub App installation was cancelled.");
      router.replace("/integrations/github");
    }
  }, [router, setupStatus]);

  useEffect(() => {
    if (!selectedOrganizationId) return;
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setError("");
      try {
        const nextIntegrations = await getIntegrations(selectedOrganizationId);
        const nextGithub =
          nextIntegrations.find((integration) => integration.integration_type === "github") ?? null;
        if (cancelled) return;
        setGithub(nextGithub);
        if (nextGithub?.status === "connected") {
          const response = await getGithubRepositories(selectedOrganizationId);
          if (!cancelled) setRepositories(response.repositories);
        } else {
          setRepositories([]);
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Could not load GitHub.");
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [selectedOrganizationId]);

  const refreshRepositories = async () => {
    if (!selectedOrganizationId) return;
    setRefreshing(true);
    setError("");
    setNotice("");
    try {
      const response = await getGithubRepositories(selectedOrganizationId);
      setRepositories(response.repositories);
      setNotice(`GitHub repository access refreshed at ${formatDateTime(response.refreshed_at)}.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not refresh GitHub repositories.");
    } finally {
      setRefreshing(false);
    }
  };

  const disconnect = async () => {
    if (!selectedOrganizationId) return;
    setDisconnecting(true);
    setError("");
    setNotice("");
    try {
      await deleteGithubIntegration(selectedOrganizationId);
      router.push("/integrations");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not disconnect GitHub.");
    } finally {
      setDisconnecting(false);
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
            <IntegrationIcon integration="github" name="GitHub" />
            <div className="flex flex-col gap-1">
              <h1 className="text-2xl font-semibold tracking-tight">GitHub</h1>
              <p className="text-sm text-muted-foreground">
                Install Bella as a GitHub App so agents can read repository context and open PRs.
              </p>
            </div>
          </div>
          <Badge variant={connected ? "secondary" : github ? "outline" : "outline"}>
            {connected ? "Connected" : github ? "Needs attention" : "Not installed"}
          </Badge>
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
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <Skeleton className="h-80 rounded-xl" />
          <Skeleton className="h-80 rounded-xl" />
        </div>
      ) : (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <div className="flex flex-col gap-4">
            <Card>
              <CardHeader>
                <CardTitle>Setup progress</CardTitle>
                <CardDescription>
                  GitHub App credentials are configured per environment. Bella stores only the
                  installation ID and repository metadata.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                <SetupStep done={connected} label="GitHub App installed" />
                <SetupStep done={repositories.length > 0} label="Repository access synced" />
                <SetupStep done={connected} label="PR creation available to agents" />
              </CardContent>
              <CardFooter className="flex flex-wrap gap-2">
                {canManage && installUrl ? (
                  <Button asChild>
                    <a href={installUrl}>
                      <ShieldCheckIcon data-icon="inline-start" />
                      {connected ? "Update installation" : "Install GitHub App"}
                    </a>
                  </Button>
                ) : (
                  <Button disabled>
                    <ShieldCheckIcon data-icon="inline-start" />
                    Install GitHub App
                  </Button>
                )}
                <Button
                  type="button"
                  variant="outline"
                  disabled={!connected || refreshing}
                  onClick={() => void refreshRepositories()}
                >
                  {refreshing ? <Spinner /> : <RefreshCwIcon data-icon="inline-start" />}
                  Refresh repos
                </Button>
              </CardFooter>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Environment URLs</CardTitle>
                <CardDescription>
                  Use these values in the GitHub App for this Bella environment.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-4">
                <KeyValueRow
                  label="Setup callback URL"
                  value={callbackUrl}
                  onCopy={() => void copy("callback", callbackUrl)}
                  copied={copied === "callback"}
                />
                <KeyValueRow
                  label="Webhook URL"
                  value={webhookUrl}
                  onCopy={() => void copy("webhook", webhookUrl)}
                  copied={copied === "webhook"}
                />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Repositories</CardTitle>
                <CardDescription>
                  Repositories visible to the GitHub App installation. Agents can use these for
                  incident context and future PR creation.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                {repositories.length === 0 ? (
                  <div className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground">
                    No repositories synced yet. Install the GitHub App or refresh repository access.
                  </div>
                ) : (
                  repositories.map((repository) => (
                    <div
                      key={repository.id}
                      className="flex flex-col gap-2 rounded-lg border p-3 sm:flex-row sm:items-center sm:justify-between"
                    >
                      <div className="min-w-0">
                        <div className="truncate font-medium">{repository.full_name}</div>
                        <div className="text-sm text-muted-foreground">
                          Default branch {repository.default_branch}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge variant="outline">{repository.private ? "Private" : "Public"}</Badge>
                        <Button asChild variant="ghost" size="sm">
                          <a href={repository.html_url} target="_blank" rel="noreferrer">
                            <ExternalLinkIcon data-icon="inline-start" />
                            Open
                          </a>
                        </Button>
                      </div>
                    </div>
                  ))
                )}
              </CardContent>
            </Card>
          </div>

          <div className="flex flex-col gap-4">
            <Card>
              <CardHeader>
                <CardTitle>Agent capabilities</CardTitle>
                <CardDescription>Initial platform plumbing for AI SRE workflows.</CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-4 text-sm text-muted-foreground">
                <div className="flex gap-3">
                  <GitPullRequestIcon />
                  <div>Create branches and pull requests through installation tokens.</div>
                </div>
                <Separator />
                <div className="flex gap-3">
                  <ShieldCheckIcon />
                  <div>Use repository permissions granted to the GitHub App installation.</div>
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Connection details</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-3 text-sm">
                <Detail
                  label="Account"
                  value={stringMetadata(github?.metadata, "account_login") || "Not installed"}
                />
                <Detail
                  label="Repository selection"
                  value={stringMetadata(github?.metadata, "repository_selection") || "Unknown"}
                />
                <Detail label="Last updated" value={formatDateTime(github?.updated_at)} />
              </CardContent>
              <CardFooter>
                <Button
                  type="button"
                  variant="destructive"
                  disabled={!github || !canManage || disconnecting}
                  onClick={() => void disconnect()}
                >
                  {disconnecting ? <Spinner /> : <Trash2Icon data-icon="inline-start" />}
                  Disconnect
                </Button>
              </CardFooter>
            </Card>
          </div>
        </div>
      )}
    </div>
  );
}

function SetupStep({ done, label }: { done: boolean; label: string }) {
  return (
    <div className="flex items-center gap-3 text-sm">
      <Badge variant={done ? "secondary" : "outline"}>{done ? "Done" : "Pending"}</Badge>
      <span className={done ? "text-foreground" : "text-muted-foreground"}>{label}</span>
    </div>
  );
}

function KeyValueRow({
  label,
  value,
  copied,
  onCopy,
}: {
  label: string;
  value: string;
  copied: boolean;
  onCopy: () => void;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-lg border p-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        <div className="truncate font-mono text-sm text-muted-foreground">{value}</div>
      </div>
      <Button type="button" variant="outline" onClick={onCopy}>
        {copied ? <CheckIcon data-icon="inline-start" /> : <CopyIcon data-icon="inline-start" />}
        Copy
      </Button>
    </div>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="truncate text-right">{value}</span>
    </div>
  );
}

function stringMetadata(metadata: Record<string, unknown> | undefined, key: string) {
  const value = metadata?.[key];
  return typeof value === "string" ? value : "";
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

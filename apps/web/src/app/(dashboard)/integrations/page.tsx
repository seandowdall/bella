"use client"

import { useEffect, useMemo, useState } from "react"
import {
  CheckIcon,
  CopyIcon,
  ExternalLinkIcon,
  MessageSquareIcon,
  RotateCcwIcon,
} from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import {
  connectPosthogIntegration,
  createSlackInstallUrl,
  getIntegrations,
  getSlackStatus,
} from "@/lib/api"
import { useAuth } from "@/lib/auth-context"
import type {
  Integration,
  PosthogConnection,
  SlackStatus,
} from "@/lib/dashboard-types"

const publicApiUrl =
  process.env.NEXT_PUBLIC_BELLA_PUBLIC_API_URL ?? "http://127.0.0.1:3000"

export default function IntegrationsPage() {
  const { selectedOrganizationId } = useAuth()
  const [integrations, setIntegrations] = useState<Integration[]>([])
  const [slackStatus, setSlackStatus] = useState<SlackStatus | null>(null)
  const [connection, setConnection] = useState<PosthogConnection | null>(null)
  const [loading, setLoading] = useState(true)
  const [installingSlack, setInstallingSlack] = useState(false)
  const [connecting, setConnecting] = useState(false)
  const [error, setError] = useState("")
  const [copied, setCopied] = useState("")

  const posthog = integrations.find(
    (integration) => integration.integration_type === "posthog",
  )
  const webhookUrl = useMemo(() => {
    if (!selectedOrganizationId) return ""
    return `${publicApiUrl.replace(/\/$/, "")}/v1/organizations/${selectedOrganizationId}/webhooks/posthog`
  }, [selectedOrganizationId])

  useEffect(() => {
    if (!selectedOrganizationId) return
    let cancelled = false
    const load = async () => {
      setError("")
      try {
        const [nextIntegrations, nextSlackStatus] = await Promise.all([
          getIntegrations(selectedOrganizationId),
          getSlackStatus(selectedOrganizationId),
        ])
        if (!cancelled) {
          setIntegrations(nextIntegrations)
          setSlackStatus(nextSlackStatus)
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Could not load integrations.")
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    void load()
    return () => {
      cancelled = true
    }
  }, [selectedOrganizationId])

  const connectPosthog = async () => {
    if (!selectedOrganizationId) return
    setConnecting(true)
    setError("")
    try {
      const nextConnection = await connectPosthogIntegration({
        organizationId: selectedOrganizationId,
      })
      setConnection(nextConnection)
      setIntegrations((current) => [
        ...current.filter((item) => item.id !== nextConnection.integration.id),
        nextConnection.integration,
      ])
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not connect PostHog.")
    } finally {
      setConnecting(false)
    }
  }

  const installSlack = async () => {
    if (!selectedOrganizationId) return
    setInstallingSlack(true)
    setError("")
    try {
      const result = await createSlackInstallUrl({
        organizationId: selectedOrganizationId,
        returnTo: window.location.href,
      })
      window.location.assign(result.install_url)
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not start Slack install.")
      setInstallingSlack(false)
    }
  }

  const copy = async (label: string, value: string) => {
    await navigator.clipboard.writeText(value)
    setCopied(label)
    window.setTimeout(() => setCopied(""), 1600)
  }

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
              <CardTitle className="flex items-center gap-2">
                <MessageSquareIcon />
                Slack
              </CardTitle>
              <CardDescription>
                Route new incident threads to the Slack channels where Bella is
                invited.
              </CardDescription>
            </div>
            <Badge variant={slackStatus?.installed ? "secondary" : "outline"}>
              {slackStatus?.installed ? "Installed" : "Not installed"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          {loading ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner />
              Loading Slack
            </div>
          ) : slackStatus?.installed ? (
            <>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="slack-workspace">Workspace</FieldLabel>
                  <Input
                    id="slack-workspace"
                    value={`${slackStatus.workspace?.team_name ?? "Slack workspace"} · ${formatLabel(slackStatus.workspace?.status ?? "connected")}`}
                    readOnly
                  />
                  {slackStatus.workspace?.status_reason && (
                    <FieldDescription>
                      {formatLabel(slackStatus.workspace.status_reason)}
                    </FieldDescription>
                  )}
                </Field>
                <Field>
                  <FieldLabel htmlFor="slack-invite-command">
                    Channel invite
                  </FieldLabel>
                  <div className="flex gap-2">
                    <Input id="slack-invite-command" value="/invite @Bella" readOnly />
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => void copy("slack-invite", "/invite @Bella")}
                    >
                      {copied === "slack-invite" ? (
                        <CheckIcon data-icon="inline-start" />
                      ) : (
                        <CopyIcon data-icon="inline-start" />
                      )}
                      Copy
                    </Button>
                  </div>
                  <FieldDescription>
                    Run this in each Slack channel where Bella should create
                    incident threads.
                  </FieldDescription>
                </Field>
              </FieldGroup>

              {slackStatus.channels.length > 0 ? (
                <div className="flex flex-col gap-2">
                  <p className="text-sm font-medium">Detected channels</p>
                  <div className="flex flex-col gap-2">
                    {slackStatus.channels.map((channel) => (
                      <div
                        key={channel.id}
                        className="flex flex-col gap-2 rounded-lg border p-3 sm:flex-row sm:items-center sm:justify-between"
                      >
                        <div className="flex min-w-0 flex-col gap-1">
                          <p className="truncate text-sm font-medium">
                            {channel.channel_name
                              ? `#${channel.channel_name}`
                              : channel.channel_id}
                          </p>
                          <p className="text-muted-foreground text-xs">
                            {formatLabel(channel.channel_type)}
                          </p>
                        </div>
                        <Badge
                          variant={
                            channel.status === "active" ? "secondary" : "outline"
                          }
                        >
                          {formatLabel(channel.status)}
                        </Badge>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <Alert>
                  <AlertDescription>
                    Invite Bella to a Slack channel to activate incident
                    delivery.
                  </AlertDescription>
                </Alert>
              )}
            </>
          ) : (
            <Alert>
              <AlertDescription>
                Install Bella in Slack, then invite it to the channel where
                incidents should be posted.
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
        <CardFooter className="justify-end gap-3">
          <Button
            type="button"
            onClick={() => void installSlack()}
            disabled={!selectedOrganizationId || installingSlack}
          >
            {installingSlack ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <ExternalLinkIcon data-icon="inline-start" />
            )}
            {slackStatus?.installed ? "Reconnect Slack" : "Install Bella in Slack"}
          </Button>
        </CardFooter>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div className="flex flex-col gap-1">
              <CardTitle>PostHog</CardTitle>
              <CardDescription>
                Receive error tracking alerts, exception events, and product
                signals as Bella incidents.
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
                    Set this as the HTTP webhook destination in PostHog. For
                    deployed self-hosting, set NEXT_PUBLIC_BELLA_PUBLIC_API_URL
                    to your public API origin.
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
                        onClick={() =>
                          void copy("secret", connection.webhook_secret)
                        }
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
                    Send this as Authorization: Bearer, X-Bella-Webhook-Secret,
                    or X-PostHog-Webhook-Secret. The full secret is shown only
                    when generated or rotated.
                  </FieldDescription>
                </Field>
              </FieldGroup>

              <div className="rounded-lg bg-muted p-4 text-sm">
                <p className="font-medium">PostHog setup</p>
                <ol className="mt-2 list-decimal pl-5 text-muted-foreground">
                  <li>Create an error tracking alert or realtime destination.</li>
                  <li>Choose HTTP webhook as the destination.</li>
                  <li>Paste the webhook URL above.</li>
                  <li>Add the secret as a bearer token or webhook header.</li>
                  <li>Trigger a test alert and check Bella Incidents.</li>
                </ol>
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
          <Button onClick={() => void connectPosthog()} disabled={connecting}>
            {connecting ? (
              <Spinner data-icon="inline-start" />
            ) : posthog ? (
              <RotateCcwIcon data-icon="inline-start" />
            ) : null}
            {posthog ? "Rotate secret" : "Connect PostHog"}
          </Button>
        </CardFooter>
      </Card>
    </div>
  )
}

function formatLabel(value: string) {
  return value
    .split("_")
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ")
}

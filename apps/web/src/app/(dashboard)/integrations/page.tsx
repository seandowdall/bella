"use client"

import { useEffect, useMemo, useState } from "react"
import { CheckIcon, CopyIcon, ExternalLinkIcon, RotateCcwIcon } from "lucide-react"
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
import { connectPosthogIntegration, getIntegrations } from "@/lib/api"
import { useAuth } from "@/lib/auth-context"
import type { Integration, PosthogConnection } from "@/lib/dashboard-types"

const publicApiUrl =
  process.env.NEXT_PUBLIC_BELLA_PUBLIC_API_URL ?? "http://127.0.0.1:3000"

export default function IntegrationsPage() {
  const { selectedOrganizationId } = useAuth()
  const [integrations, setIntegrations] = useState<Integration[]>([])
  const [connection, setConnection] = useState<PosthogConnection | null>(null)
  const [loading, setLoading] = useState(true)
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
        const nextIntegrations = await getIntegrations(selectedOrganizationId)
        if (!cancelled) setIntegrations(nextIntegrations)
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

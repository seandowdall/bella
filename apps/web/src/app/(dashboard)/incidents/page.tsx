"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { AlertTriangleIcon, RefreshCwIcon } from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Spinner } from "@/components/ui/spinner"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { getIncidents } from "@/lib/api"
import { useAuth } from "@/lib/auth-context"
import type { IncidentListItem, IncidentSeverity } from "@/lib/dashboard-types"

export default function IncidentsPage() {
  const { selectedOrganizationId } = useAuth()
  const [incidents, setIncidents] = useState<IncidentListItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")

  const loadIncidents = async () => {
    if (!selectedOrganizationId) return
    setLoading(true)
    setError("")
    try {
      setIncidents(await getIncidents(selectedOrganizationId))
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load incidents.")
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (!selectedOrganizationId) return
    const organizationId = selectedOrganizationId
    let cancelled = false

    const load = async () => {
      setError("")
      try {
        const nextIncidents = await getIncidents(organizationId)
        if (!cancelled) setIncidents(nextIncidents)
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Could not load incidents.")
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

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold tracking-tight">Incidents</h1>
          <p className="text-muted-foreground text-sm">
            PostHog error signals normalized into Bella incidents.
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={() => void loadIncidents()}>
          {loading ? (
            <Spinner data-icon="inline-start" />
          ) : (
            <RefreshCwIcon data-icon="inline-start" />
          )}
          Refresh
        </Button>
      </div>

      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Recent incidents</CardTitle>
          <CardDescription>
            The first read model for PostHog ingestion. Slack, agent runs, and
            GitHub remediation will attach to these records next.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="flex items-center gap-2 py-8 text-sm text-muted-foreground">
              <Spinner />
              Loading incidents
            </div>
          ) : incidents.length ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Incident</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Severity</TableHead>
                  <TableHead>Signals</TableHead>
                  <TableHead>Detected</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {incidents.map((incident) => (
                  <TableRow key={incident.id}>
                    <TableCell>
                      <Link
                        href={`/incidents/${incident.id}`}
                        className="font-medium hover:underline"
                      >
                        {incident.title}
                      </Link>
                      <div className="mt-1 max-w-md truncate text-xs text-muted-foreground">
                        {incident.source} / {incident.fingerprint}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{formatLabel(incident.status)}</Badge>
                    </TableCell>
                    <TableCell>
                      <SeverityBadge severity={incident.severity} />
                    </TableCell>
                    <TableCell>{incident.signal_count}</TableCell>
                    <TableCell>{formatDate(incident.detected_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <AlertTriangleIcon />
                </EmptyMedia>
                <EmptyTitle>No incidents yet</EmptyTitle>
                <EmptyDescription>
                  Send a PostHog error tracking webhook to start building the
                  incident timeline.
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function SeverityBadge({ severity }: { severity: IncidentSeverity }) {
  const variant =
    severity === "critical" || severity === "high"
      ? "destructive"
      : severity === "unknown"
        ? "outline"
        : "secondary"
  return <Badge variant={variant}>{formatLabel(severity)}</Badge>
}

function formatLabel(value: string) {
  return value.replaceAll("_", " ")
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value))
}

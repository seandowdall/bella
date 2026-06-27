"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import {
  ArrowLeftIcon,
  CheckCircle2Icon,
  ClockIcon,
  EyeIcon,
  FlagIcon,
  RadioIcon,
  ShieldCheckIcon,
  WrenchIcon,
} from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { getIncident, updateIncidentStatus } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type {
  IncidentDetail,
  IncidentEventDetail,
  IncidentSeverity,
  IncidentStatus,
  SignalDetail,
} from "@/lib/dashboard-types";

const lifecycleActions: {
  status: IncidentStatus;
  label: string;
  icon: typeof ShieldCheckIcon;
}[] = [
  { status: "acknowledged", label: "Acknowledge", icon: ShieldCheckIcon },
  { status: "investigating", label: "Investigating", icon: EyeIcon },
  { status: "mitigated", label: "Mitigated", icon: WrenchIcon },
  { status: "resolved", label: "Resolved", icon: CheckCircle2Icon },
  { status: "follow_up", label: "Follow-up", icon: FlagIcon },
];

export default function IncidentDetailPage() {
  const params = useParams<{ incidentId: string }>();
  const { selectedOrganization } = useAuth();
  const [incident, setIncident] = useState<IncidentDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [transitioning, setTransitioning] = useState<IncidentStatus | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!selectedOrganization || !params.incidentId) return;
    let cancelled = false;

    const loadIncident = async () => {
      setLoading(true);
      setError("");
      try {
        const nextIncident = await getIncident({
          organizationId: selectedOrganization.id,
          incidentId: params.incidentId,
        });
        if (!cancelled) setIncident(nextIncident);
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Could not load incident.");
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    void loadIncident();

    return () => {
      cancelled = true;
    };
  }, [params.incidentId, selectedOrganization]);

  const changeStatus = async (status: IncidentStatus) => {
    if (!selectedOrganization || !incident) return;
    setTransitioning(status);
    setError("");
    try {
      setIncident(
        await updateIncidentStatus({
          organizationId: selectedOrganization.id,
          incidentId: incident.id,
          status,
        }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not update incident status.");
    } finally {
      setTransitioning(null);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Spinner />
        Loading incident
      </div>
    );
  }

  if (error) {
    return (
      <Alert variant="destructive">
        <AlertDescription>{error}</AlertDescription>
      </Alert>
    );
  }

  if (!incident) return null;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-3">
        <Button variant="ghost" size="sm" asChild className="w-fit">
          <Link href="/incidents">
            <ArrowLeftIcon data-icon="inline-start" />
            Incidents
          </Link>
        </Button>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex flex-col gap-2">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary">{formatLabel(incident.status)}</Badge>
              <SeverityBadge severity={incident.severity} />
              <Badge variant="outline">{incident.source}</Badge>
            </div>
            <h1 className="text-2xl font-semibold tracking-tight">{incident.title}</h1>
            <p className="text-muted-foreground text-sm">
              Detected {formatDate(incident.detected_at)} / fingerprint {incident.fingerprint}
            </p>
          </div>
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_24rem]">
        <div className="flex flex-col gap-4">
          <Card>
            <CardHeader>
              <CardTitle>Timeline</CardTitle>
              <CardDescription>
                Durable incident events. Agent, Slack, and GitHub actions will append here next.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              {incident.events.length ? (
                incident.events.map((event, index) => (
                  <TimelineEvent key={event.id} event={event} first={index === 0} />
                ))
              ) : (
                <p className="text-sm text-muted-foreground">No timeline events recorded.</p>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Signals</CardTitle>
              <CardDescription>Raw PostHog signals attached to this incident.</CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              {incident.signals.map((signal) => (
                <SignalCard key={signal.id} signal={signal} />
              ))}
            </CardContent>
          </Card>
        </div>

        <Card className="h-fit">
          <CardHeader>
            <CardTitle>Incident state</CardTitle>
            <CardDescription>Responder state for the dogfood incident candidate.</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-3 text-sm">
            <DetailRow label="Status" value={formatLabel(incident.status)} />
            <DetailRow label="Severity" value={formatLabel(incident.severity)} />
            <DetailRow label="Source" value={incident.source} />
            <DetailRow label="Signals" value={String(incident.signals.length)} />
            <DetailRow label="Detected" value={formatDate(incident.detected_at)} />
            <DetailRow
              label="Resolved"
              value={incident.resolved_at ? formatDate(incident.resolved_at) : "Open"}
            />
            <Separator />
            <div className="flex flex-col gap-2">
              {lifecycleActions.map((action) => {
                const Icon = action.icon;
                const active = incident.status === action.status;
                return (
                  <Button
                    key={action.status}
                    type="button"
                    variant={active ? "secondary" : "outline"}
                    className="justify-start"
                    disabled={active || transitioning !== null}
                    onClick={() => void changeStatus(action.status)}
                  >
                    {transitioning === action.status ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <Icon data-icon="inline-start" />
                    )}
                    {action.label}
                  </Button>
                );
              })}
            </div>
            <Separator />
            <pre className="max-h-80 overflow-auto rounded-lg bg-muted p-3 text-xs">
              {JSON.stringify(incident.metadata, null, 2)}
            </pre>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function TimelineEvent({ event, first }: { event: IncidentEventDetail; first: boolean }) {
  return (
    <div className="flex gap-3">
      <div className="flex flex-col items-center gap-2">
        <div className="flex size-8 items-center justify-center rounded-full bg-muted">
          {first ? <RadioIcon /> : <ClockIcon />}
        </div>
        <div className="min-h-4 w-px flex-1 bg-border" />
      </div>
      <div className="min-w-0 flex-1 pb-4">
        <div className="flex flex-wrap items-center gap-2">
          <p className="font-medium">{event.title}</p>
          <Badge variant="outline">{event.event_type}</Badge>
        </div>
        <p className="mt-1 text-xs text-muted-foreground">{formatDate(event.created_at)}</p>
        {event.body && <p className="mt-2 text-sm">{event.body}</p>}
      </div>
    </div>
  );
}

function SignalCard({ signal }: { signal: SignalDetail }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{signal.title}</CardTitle>
        <CardDescription>
          {signal.signal_type} / received {formatDate(signal.received_at)}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <SeverityBadge severity={signal.severity} />
          {signal.source_event_id && <Badge variant="outline">{signal.source_event_id}</Badge>}
        </div>
        <pre className="max-h-96 overflow-auto rounded-lg bg-muted p-3 text-xs">
          {JSON.stringify(signal.payload, null, 2)}
        </pre>
      </CardContent>
    </Card>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right font-medium">{value}</span>
    </div>
  );
}

function SeverityBadge({ severity }: { severity: IncidentSeverity }) {
  let variant: "secondary" | "destructive" | "outline" = "secondary";
  if (severity === "critical" || severity === "high") variant = "destructive";
  if (severity === "unknown") variant = "outline";
  return <Badge variant={variant}>{formatLabel(severity)}</Badge>;
}

function formatLabel(value: string) {
  return value.replaceAll("_", " ");
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

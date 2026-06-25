"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { ChartAreaInteractive } from "@/components/chart-area-interactive";
import type { TimeRange } from "@/components/chart-area-interactive";
import { SectionCards } from "@/components/section-cards";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { getUsageSummary } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import type { UsageSummary } from "@/lib/dashboard-types";
import { useAiCostVisibilityFlag } from "@/lib/feature-flags";
import { daysForRange } from "@/components/chart-area-interactive";

export default function DataPage() {
  const router = useRouter();
  const { enabled: costVisibilityEnabled, loaded: costVisibilityLoaded } =
    useAiCostVisibilityFlag();
  const { selectedOrganization } = useAuth();
  const [timeRange, setTimeRange] = useState<TimeRange>("30d");
  const [summary, setSummary] = useState<UsageSummary>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!costVisibilityEnabled || !selectedOrganization) return;
    const load = async () => {
      setLoading(true);
      setError("");
      try {
        const { start, end } = dateRange(daysForRange(timeRange));
        setSummary(
          await getUsageSummary({
            organizationId: selectedOrganization.id,
            start,
            end,
          }),
        );
      } catch (e) {
        setError(e instanceof Error ? e.message : "Could not load usage data.");
      } finally {
        setLoading(false);
      }
    };
    void load();
  }, [costVisibilityEnabled, selectedOrganization, timeRange]);

  useEffect(() => {
    if (costVisibilityLoaded && !costVisibilityEnabled) {
      router.replace("/");
    }
  }, [costVisibilityEnabled, costVisibilityLoaded, router]);

  if (!costVisibilityEnabled) return null;

  return (
    <>
      <SectionCards summary={summary} loading={loading} />
      {error && <p className="text-sm text-destructive">{error}</p>}
      <ChartAreaInteractive
        timeRange={timeRange}
        onTimeRangeChange={setTimeRange}
        chartData={chartData(summary, daysForRange(timeRange))}
      />
      <ModelBreakdown summary={summary} />
    </>
  );
}

function dateRange(days: number) {
  const end = new Date();
  end.setHours(0, 0, 0, 0);
  const start = new Date(end);
  start.setDate(start.getDate() - (days - 1));
  return {
    start: start.toISOString().slice(0, 10),
    end: end.toISOString().slice(0, 10),
  };
}

function chartData(summary: UsageSummary | undefined, days: number) {
  const values = new Map(summary?.daily_spend.map((item) => [item.date, item.amount_micros]) ?? []);
  return Array.from({ length: days }, (_, offset) => {
    const date = new Date();
    date.setHours(0, 0, 0, 0);
    date.setDate(date.getDate() - (days - offset - 1));
    const key = date.toISOString().slice(0, 10);
    return { date: key, spend: (values.get(key) ?? 0) / 1_000_000 };
  });
}

function ModelBreakdown({ summary }: { summary?: UsageSummary }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Provider and model breakdown</CardTitle>
        <CardDescription>
          Imported spend and usage grouped by normalized provider/model keys.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-hidden rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Provider</TableHead>
                <TableHead>Model</TableHead>
                <TableHead className="text-right">Spend</TableHead>
                <TableHead className="text-right">Input</TableHead>
                <TableHead className="text-right">Output</TableHead>
                <TableHead className="text-right">Requests</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {summary?.model_breakdown.length ? (
                summary.model_breakdown.map((item) => (
                  <TableRow key={`${item.provider}-${item.model}`}>
                    <TableCell>{item.provider}</TableCell>
                    <TableCell>{item.model}</TableCell>
                    <TableCell className="text-right">{formatMicros(item.amount_micros)}</TableCell>
                    <TableCell className="text-right">{formatNumber(item.input_tokens)}</TableCell>
                    <TableCell className="text-right">{formatNumber(item.output_tokens)}</TableCell>
                    <TableCell className="text-right">{formatNumber(item.request_count)}</TableCell>
                  </TableRow>
                ))
              ) : (
                <TableRow>
                  <TableCell colSpan={6} className="h-24 text-center">
                    No imported usage yet.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  );
}

function formatMicros(value: number) {
  return new Intl.NumberFormat(undefined, {
    style: "currency",
    currency: "USD",
  }).format(value / 1_000_000);
}

function formatNumber(value: number) {
  return new Intl.NumberFormat().format(value);
}

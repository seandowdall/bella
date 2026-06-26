import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { UsageSummary } from "@/lib/dashboard-types";

export function SectionCards({ summary, loading }: { summary?: UsageSummary; loading?: boolean }) {
  const metrics = [
    {
      label: "Total spend",
      value: loading ? "..." : formatMicros(summary?.total_spend_micros ?? 0),
      description: "Selected date range",
    },
    {
      label: "Input tokens",
      value: loading ? "..." : formatNumber(summary?.input_tokens ?? 0),
      description: "Across all providers",
    },
    {
      label: "Output tokens",
      value: loading ? "..." : formatNumber(summary?.output_tokens ?? 0),
      description: "Across all providers",
    },
    {
      label: "Requests",
      value: loading ? "..." : formatNumber(summary?.request_count ?? 0),
      description: "Model calls imported",
    },
  ];

  return (
    <div className="grid grid-cols-1 gap-4 @xl/main:grid-cols-2 @5xl/main:grid-cols-4">
      {metrics.map((metric) => (
        <Card
          key={metric.label}
          className="@container/card bg-gradient-to-t from-primary/5 to-card shadow-xs"
        >
          <CardHeader>
            <CardDescription>{metric.label}</CardDescription>
            <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
              {metric.value}
            </CardTitle>
            <CardAction>
              <Badge variant="outline">{loading ? "Loading" : "Live data"}</Badge>
            </CardAction>
          </CardHeader>
          <CardFooter className="text-muted-foreground text-sm">{metric.description}</CardFooter>
        </Card>
      ))}
    </div>
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

import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from '@/components/ui/chart'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

const chartConfig = {
  spend: {
    label: 'Spend',
    color: 'var(--primary)',
  },
} satisfies ChartConfig

const ranges = {
  '7d': 7,
  '30d': 30,
  '90d': 90,
}

export type TimeRange = keyof typeof ranges

export function daysForRange(range: TimeRange) {
  return ranges[range]
}

export function ChartAreaInteractive({
  timeRange,
  onTimeRangeChange,
  chartData,
}: {
  timeRange: TimeRange
  onTimeRangeChange: (range: TimeRange) => void
  chartData: Array<{ date: string; spend: number }>
}) {

  return (
    <Card className="@container/card">
      <CardHeader>
          <CardTitle>AI spend</CardTitle>
          <CardDescription>
          Daily provider costs from normalized ingestion data.
          </CardDescription>
        <CardAction>
          <Select
            value={timeRange}
            onValueChange={(value) =>
              onTimeRangeChange(value as TimeRange)
            }
          >
            <SelectTrigger size="sm" className="w-36" aria-label="Time range">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="7d">Last 7 days</SelectItem>
                <SelectItem value="30d">Last 30 days</SelectItem>
                <SelectItem value="90d">Last 90 days</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
        </CardAction>
      </CardHeader>
      <CardContent className="px-2 pt-4 sm:px-6">
        <ChartContainer
          config={chartConfig}
          className="aspect-auto h-64 w-full"
        >
          <AreaChart data={chartData}>
            <defs>
              <linearGradient id="fillSpend" x1="0" y1="0" x2="0" y2="1">
                <stop
                  offset="5%"
                  stopColor="var(--color-spend)"
                  stopOpacity={0.7}
                />
                <stop
                  offset="95%"
                  stopColor="var(--color-spend)"
                  stopOpacity={0.05}
                />
              </linearGradient>
            </defs>
            <CartesianGrid vertical={false} />
            <XAxis
              dataKey="date"
              tickLine={false}
              axisLine={false}
              tickMargin={8}
              minTickGap={32}
              tickFormatter={(value: string) =>
                new Date(`${value}T00:00:00`).toLocaleDateString(undefined, {
                  month: 'short',
                  day: 'numeric',
                })
              }
            />
            <YAxis
              tickLine={false}
              axisLine={false}
              width={44}
              tickFormatter={(value: number) => `$${value}`}
            />
            <ChartTooltip
              cursor={false}
              content={
                <ChartTooltipContent
                  indicator="dot"
                  labelFormatter={(value) =>
                    new Date(`${value}T00:00:00`).toLocaleDateString()
                  }
                  formatter={(value) => `$${Number(value).toFixed(2)}`}
                />
              }
            />
            <Area
              dataKey="spend"
              type="monotone"
              fill="url(#fillSpend)"
              stroke="var(--color-spend)"
            />
          </AreaChart>
        </ChartContainer>
      </CardContent>
    </Card>
  )
}

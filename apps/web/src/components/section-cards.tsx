import { Badge } from '@/components/ui/badge'
import {
  Card,
  CardAction,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'

const metrics = [
  {
    label: 'Total spend',
    value: '$0.00',
    description: 'Current billing period',
  },
  {
    label: 'Input tokens',
    value: '0',
    description: 'Across all providers',
  },
  {
    label: 'Output tokens',
    value: '0',
    description: 'Across all providers',
  },
  {
    label: 'Provider accounts',
    value: '0',
    description: 'No credentials stored yet',
  },
]

export function SectionCards() {
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
              <Badge variant="outline">No data</Badge>
            </CardAction>
          </CardHeader>
          <CardFooter className="text-muted-foreground text-sm">
            {metric.description}
          </CardFooter>
        </Card>
      ))}
    </div>
  )
}

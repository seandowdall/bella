import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

export default function CliSuccessPage() {
  return (
    <main className="grid min-h-svh place-items-center p-6">
      <Card className="w-full max-w-xl">
        <CardHeader>
          <p className="text-primary text-xs font-bold tracking-[0.14em] uppercase">
            Bella CLI
          </p>
          <CardTitle className="text-3xl">Login complete</CardTitle>
          <CardDescription>
            You can close this tab and return to your terminal.
          </CardDescription>
        </CardHeader>
      </Card>
    </main>
  )
}

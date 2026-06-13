"use client"

import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { getLoginUrl } from "@/lib/api"

export default function LoginPage() {
  const login = () => {
    window.location.assign(getLoginUrl())
  }

  return (
    <main className="grid min-h-svh place-items-center p-6">
      <Card className="w-full max-w-2xl">
        <CardHeader className="gap-5">
          <p className="text-primary text-xs font-bold tracking-[0.14em] uppercase">
            Open source AI cost visibility
          </p>
          <CardTitle className="text-5xl tracking-tight sm:text-7xl">
            Bella
          </CardTitle>
          <CardDescription className="max-w-xl text-base">
            Track AI usage and spend across teams, workspaces, providers, and
            models.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button size="lg" onClick={login}>
            Log in with GitHub
          </Button>
        </CardContent>
      </Card>
    </main>
  )
}

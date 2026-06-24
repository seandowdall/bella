"use client"

import { useEffect } from "react"
import { useRouter } from "next/navigation"
import { ProviderAccounts } from "@/components/provider-accounts"
import { useAuth } from "@/lib/auth-context"
import { useAiCostVisibilityFlag } from "@/lib/feature-flags"

export default function ProvidersPage() {
  const router = useRouter()
  const { enabled: costVisibilityEnabled, loaded: costVisibilityLoaded } =
    useAiCostVisibilityFlag()
  const { selectedOrganization } = useAuth()

  useEffect(() => {
    if (costVisibilityLoaded && !costVisibilityEnabled) {
      router.replace("/")
    }
  }, [costVisibilityEnabled, costVisibilityLoaded, router])

  if (!costVisibilityEnabled) return null

  return <ProviderAccounts organization={selectedOrganization} />
}

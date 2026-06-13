"use client"

import { ProviderAccounts } from "@/components/provider-accounts"
import { useAuth } from "@/lib/auth-context"

export default function ProvidersPage() {
  const { selectedOrganization } = useAuth()

  return <ProviderAccounts organization={selectedOrganization} />
}

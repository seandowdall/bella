"use client"

import {
  ByokSettings,
  SettingsPageHeader,
} from "@/components/settings-sections"
import { useAuth } from "@/lib/auth-context"

export default function SettingsAiPage() {
  const { selectedOrganization } = useAuth()
  const canManage =
    selectedOrganization?.role === "owner" || selectedOrganization?.role === "admin"

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <SettingsPageHeader
        title="AI"
        description="Configure how the Bella agent answers product questions and which organization-owned model key it uses."
      />
      <ByokSettings
        organizationId={selectedOrganization?.id}
        canManage={canManage}
      />
    </div>
  )
}

"use client"

import {
  ProfileSettings,
  SettingsPageHeader,
} from "@/components/settings-sections"
import { useAuth } from "@/lib/auth-context"

export default function SettingsProfilePage() {
  const { user } = useAuth()

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <SettingsPageHeader
        title="Profile"
        description="Manage the identity Bella uses for authentication and audit context."
      />
      <ProfileSettings user={user} />
    </div>
  )
}

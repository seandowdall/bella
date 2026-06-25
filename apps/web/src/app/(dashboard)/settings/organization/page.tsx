"use client";

import { OrganizationSettings, SettingsPageHeader } from "@/components/settings-sections";
import { useAuth } from "@/lib/auth-context";

export default function SettingsOrganizationPage() {
  const { selectedOrganization } = useAuth();

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <SettingsPageHeader
        title="Organization"
        description="Review the organization and workspace that scope Bella data access."
      />
      <OrganizationSettings organization={selectedOrganization} />
    </div>
  );
}

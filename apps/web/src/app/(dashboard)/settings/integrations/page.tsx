"use client";

import { SettingsPageHeader, SlackSettings } from "@/components/settings-sections";
import { useAuth } from "@/lib/auth-context";

export default function SettingsIntegrationsPage() {
  const { selectedOrganization } = useAuth();
  const canManage =
    selectedOrganization?.role === "owner" || selectedOrganization?.role === "admin";

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <SettingsPageHeader
        title="Integrations"
        description="Manage the external systems Bella uses to investigate and communicate about incidents."
      />
      <SlackSettings organizationId={selectedOrganization?.id} canManage={canManage} />
    </div>
  );
}

import type {
  Organization,
  AgentLlmSettings,
  AgentLlmSettingsList,
  AgentMessageResponse,
  IncidentDetail,
  IncidentListItem,
  IncidentStatus,
  Integration,
  PosthogConnectionCheck,
  PosthogSecretRotation,
  PosthogSyncOutcome,
  ProviderAccount,
  ProviderDefinition,
  SlackInstallUrl,
  SlackStatus,
  OrganizationMember,
  OrganizationMembers,
  OrganizationRole,
  SyncOutcome,
  SlackTestMessage,
  UsageSummary,
  User,
} from "@/lib/dashboard-types";

const apiBaseUrl = process.env.NEXT_PUBLIC_BELLA_API_BASE_URL ?? "/api";
const apiTimeoutMs = 3_000;

async function apiFetch(input: string, init: RequestInit & { timeoutMs?: number } = {}) {
  const { signal, timeoutMs = apiTimeoutMs, ...requestInit } = init;
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);

  const abort = () => controller.abort();
  signal?.addEventListener("abort", abort, { once: true });
  try {
    return await fetch(input, {
      ...requestInit,
      signal: controller.signal,
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error("Bella API did not respond. Is `just dev` or `just api` running?", {
        cause: error,
      });
    }
    throw error;
  } finally {
    window.clearTimeout(timeout);
    signal?.removeEventListener("abort", abort);
  }
}

export async function getMe(): Promise<User | null> {
  const response = await apiFetch(`${apiBaseUrl}/v1/me`, {
    credentials: "include",
  });
  if (!response.ok) return null;
  return response.json() as Promise<User>;
}

export async function getOrganizations(): Promise<Organization[]> {
  const response = await apiFetch(`${apiBaseUrl}/v1/organizations`, {
    credentials: "include",
  });
  if (!response.ok) throw new Error("Could not load your organizations.");
  return response.json() as Promise<Organization[]>;
}

export async function createOrganization(name: string): Promise<Organization> {
  const response = await apiFetch(`${apiBaseUrl}/v1/organizations`, {
    method: "POST",
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": crypto.randomUUID(),
    },
    body: JSON.stringify({ name }),
  });
  const body = await response.json();
  if (!response.ok) throw new Error(body.error ?? "Could not create the organization.");
  return body as Organization;
}

export async function getOrganizationMembers(organizationId: string): Promise<OrganizationMembers> {
  const response = await apiFetch(`${apiBaseUrl}/v1/organizations/${organizationId}/members`, {
    credentials: "include",
  });
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load organization members."));
  }
  return response.json() as Promise<OrganizationMembers>;
}

export async function inviteOrganizationMember({
  organizationId,
  email,
  role,
}: {
  organizationId: string;
  email: string;
  role: "admin" | "member";
}) {
  const response = await apiFetch(`${apiBaseUrl}/v1/organizations/${organizationId}/invitations`, {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email, role }),
    timeoutMs: 10_000,
  });
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not send the invitation."));
  }
  return response.json();
}

export async function revokeOrganizationInvitation(
  organizationId: string,
  invitationId: string,
): Promise<void> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/invitations/${invitationId}`,
    { method: "DELETE", credentials: "include" },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not revoke the invitation."));
  }
}

export async function acceptOrganizationInvitation(token: string): Promise<Organization> {
  const response = await apiFetch(`${apiBaseUrl}/v1/invitations/accept`, {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ token }),
  });
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not accept the invitation."));
  }
  return response.json() as Promise<Organization>;
}

export async function updateOrganizationMemberRole({
  organizationId,
  userId,
  role,
}: {
  organizationId: string;
  userId: string;
  role: Exclude<OrganizationRole, "owner">;
}): Promise<OrganizationMember> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/members/${userId}`,
    {
      method: "PATCH",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ role }),
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not update member role."));
  }
  return response.json() as Promise<OrganizationMember>;
}

export async function removeOrganizationMember(
  organizationId: string,
  userId: string,
): Promise<void> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/members/${userId}`,
    { method: "DELETE", credentials: "include" },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not remove member."));
  }
}

export async function logout(): Promise<void> {
  await apiFetch(`${apiBaseUrl}/v1/auth/logout`, {
    method: "POST",
    credentials: "include",
  });
}

export async function getProviderCatalog(): Promise<ProviderDefinition[]> {
  const response = await apiFetch(`${apiBaseUrl}/v1/providers`, {
    credentials: "include",
  });
  if (!response.ok) throw new Error("Could not load the provider catalog.");
  return response.json() as Promise<ProviderDefinition[]>;
}

export async function getProviderAccounts(organizationId: string): Promise<ProviderAccount[]> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts`,
    { credentials: "include" },
  );
  if (!response.ok) throw new Error("Could not load provider accounts.");
  return response.json() as Promise<ProviderAccount[]>;
}

export async function connectProviderAccount({
  organizationId,
  workspaceId,
  provider,
  displayName,
  secret,
}: {
  organizationId: string;
  workspaceId: string;
  provider: string;
  displayName: string;
  secret: string;
}): Promise<ProviderAccount> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts`,
    {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        workspace_id: workspaceId,
        provider,
        display_name: displayName,
        credentials: { secret },
      }),
    },
  );
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? "Could not connect the provider.");
  }
  return body as ProviderAccount;
}

async function errorMessage(response: Response, fallback: string) {
  const text = await response.text();
  const fallbackWithStatus = `${fallback} HTTP ${response.status}.`;
  if (!text) return fallbackWithStatus;
  try {
    const body = JSON.parse(text) as { error?: string };
    return body.error ?? fallbackWithStatus;
  } catch {
    return fallbackWithStatus;
  }
}

export async function deleteProviderAccount(
  organizationId: string,
  accountId: string,
): Promise<void> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts/${accountId}`,
    {
      method: "DELETE",
      credentials: "include",
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not disconnect the provider."));
  }
}

export async function updateProviderAccount(
  organizationId: string,
  accountId: string,
  displayName: string,
): Promise<ProviderAccount> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts/${accountId}`,
    {
      method: "PATCH",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ display_name: displayName }),
    },
  );
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? "Could not update the provider account.");
  }
  return body as ProviderAccount;
}

export async function syncProviderAccount(
  organizationId: string,
  accountId: string,
): Promise<SyncOutcome> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts/${accountId}/sync`,
    {
      method: "POST",
      credentials: "include",
    },
  );
  const body = await response.json();
  if (!response.ok) {
    throw new Error(body.error ?? "Could not sync provider account.");
  }
  return body as SyncOutcome;
}

export async function getUsageSummary({
  organizationId,
  start,
  end,
}: {
  organizationId: string;
  start: string;
  end: string;
}): Promise<UsageSummary> {
  const params = new URLSearchParams({ start, end });
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/usage/summary?${params}`,
    { credentials: "include" },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load usage summary."));
  }
  return response.json() as Promise<UsageSummary>;
}

export async function getIncidents(organizationId: string): Promise<IncidentListItem[]> {
  const response = await apiFetch(`${apiBaseUrl}/v1/organizations/${organizationId}/incidents`, {
    credentials: "include",
  });
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load incidents."));
  }
  return response.json() as Promise<IncidentListItem[]>;
}

export async function getIncident({
  organizationId,
  incidentId,
}: {
  organizationId: string;
  incidentId: string;
}): Promise<IncidentDetail> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/incidents/${incidentId}`,
    { credentials: "include" },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load incident."));
  }
  return response.json() as Promise<IncidentDetail>;
}

export async function updateIncidentStatus({
  organizationId,
  incidentId,
  status,
}: {
  organizationId: string;
  incidentId: string;
  status: IncidentStatus;
}): Promise<IncidentDetail> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/incidents/${incidentId}`,
    {
      method: "PATCH",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ status }),
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not update incident status."));
  }
  return response.json() as Promise<IncidentDetail>;
}

export async function getIntegrations(organizationId: string): Promise<Integration[]> {
  const response = await apiFetch(`${apiBaseUrl}/v1/organizations/${organizationId}/integrations`, {
    credentials: "include",
  });
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load integrations."));
  }
  return response.json() as Promise<Integration[]>;
}

export async function savePosthogSettings({
  organizationId,
  displayName,
  posthogHost,
  posthogProjectId,
  apiToken,
}: {
  organizationId: string;
  displayName?: string;
  posthogHost?: string;
  posthogProjectId?: string;
  apiToken?: string;
}): Promise<Integration> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/posthog`,
    {
      method: "PATCH",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        display_name: displayName ?? "PostHog",
        posthog_host: posthogHost?.trim() || null,
        posthog_project_id: posthogProjectId?.trim() || null,
        api_token: apiToken?.trim() || null,
      }),
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not save PostHog settings."));
  }
  return response.json() as Promise<Integration>;
}

export async function rotatePosthogWebhookSecret(
  organizationId: string,
): Promise<PosthogSecretRotation> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/posthog/webhook-secret/rotate`,
    {
      method: "POST",
      credentials: "include",
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not rotate PostHog webhook secret."));
  }
  return response.json() as Promise<PosthogSecretRotation>;
}

export async function deletePosthogIntegration(organizationId: string): Promise<void> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/posthog`,
    {
      method: "DELETE",
      credentials: "include",
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not disconnect PostHog."));
  }
}

export async function checkPosthogIntegration(
  organizationId: string,
): Promise<PosthogConnectionCheck> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/posthog/check`,
    {
      method: "POST",
      credentials: "include",
      timeoutMs: 15_000,
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not verify PostHog API access."));
  }
  return response.json() as Promise<PosthogConnectionCheck>;
}

export async function syncPosthogIntegration(organizationId: string): Promise<PosthogSyncOutcome> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/posthog/sync`,
    {
      method: "POST",
      credentials: "include",
      timeoutMs: 30_000,
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not sync PostHog signals."));
  }
  return response.json() as Promise<PosthogSyncOutcome>;
}

export async function sendAgentMessage({
  organizationId,
  message,
  llmSettingId,
  signal,
}: {
  organizationId: string;
  message: string;
  llmSettingId?: string;
  signal?: AbortSignal;
}): Promise<AgentMessageResponse> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/agent/messages`,
    {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message, llm_setting_id: llmSettingId ?? null }),
      signal,
    },
  );
  if (!response.ok) {
    throw new Error(
      await errorMessage(
        response,
        "Bella could not reach the agent API. Restart the API on this branch and try again.",
      ),
    );
  }
  return response.json() as Promise<AgentMessageResponse>;
}

export async function getAgentLlmSettings(organizationId: string): Promise<AgentLlmSettingsList> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/agent/settings`,
    { credentials: "include" },
  );
  if (!response.ok) {
    throw new Error(
      await errorMessage(
        response,
        "Could not load AI settings. Restart the API on this branch so the agent settings route and migration are available.",
      ),
    );
  }
  const body = (await response.json()) as AgentLlmSettingsList | AgentLlmSettings;
  if ("items" in body && Array.isArray(body.items)) {
    return body;
  }
  if ("id" in body) {
    return {
      items: [body],
      default_id: body.id,
      mode: "llm_assisted",
    };
  }
  return { items: [], default_id: null, mode: "deterministic" };
}

export async function saveAgentLlmSettings({
  organizationId,
  settingId,
  displayName,
  provider,
  model,
  apiKey,
  isDefault,
}: {
  organizationId: string;
  settingId?: string;
  displayName: string;
  provider: AgentLlmSettings["provider"];
  model: string;
  apiKey: string;
  isDefault: boolean;
}): Promise<AgentLlmSettings> {
  const response = await apiFetch(
    settingId
      ? `${apiBaseUrl}/v1/organizations/${organizationId}/agent/settings/${settingId}`
      : `${apiBaseUrl}/v1/organizations/${organizationId}/agent/settings`,
    {
      method: settingId ? "PUT" : "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        display_name: displayName,
        provider,
        model,
        api_key: apiKey.trim() || null,
        is_default: isDefault,
      }),
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not save AI settings."));
  }
  return response.json() as Promise<AgentLlmSettings>;
}

export async function deleteAgentLlmSettings(
  organizationId: string,
  settingId: string,
): Promise<void> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/agent/settings/${settingId}`,
    {
      method: "DELETE",
      credentials: "include",
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not remove AI settings."));
  }
}

export async function setDefaultAgentLlmSettings(
  organizationId: string,
  settingId: string,
): Promise<AgentLlmSettings> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/agent/settings/${settingId}/default`,
    {
      method: "POST",
      credentials: "include",
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not set the default AI model."));
  }
  return response.json() as Promise<AgentLlmSettings>;
}

export async function sendSlackTestMessage(organizationId: string): Promise<SlackTestMessage> {
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/slack/test-message`,
    {
      method: "POST",
      credentials: "include",
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not send the Slack test message."));
  }
  return response.json() as Promise<SlackTestMessage>;
}

export async function getSlackStatus(organizationId: string): Promise<SlackStatus> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/slack`,
    { credentials: "include" },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load Slack status."));
  }
  return response.json() as Promise<SlackStatus>;
}

export async function createSlackInstallUrl({
  organizationId,
  returnTo,
}: {
  organizationId: string;
  returnTo?: string;
}): Promise<SlackInstallUrl> {
  const response = await apiFetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/integrations/slack/install-url`,
    {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ return_to: returnTo ?? null }),
    },
  );
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not start Slack install."));
  }
  return response.json() as Promise<SlackInstallUrl>;
}

export function getLoginUrl(returnTo = `${window.location.origin}/`): string {
  return `${apiBaseUrl}/v1/auth/github/start?return_to=${encodeURIComponent(returnTo)}`;
}

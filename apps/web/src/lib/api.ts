import type {
  Organization,
  AgentMessageResponse,
  ProviderAccount,
  ProviderDefinition,
  SyncOutcome,
  UsageSummary,
  User,
} from "@/lib/dashboard-types"

const apiBaseUrl = process.env.NEXT_PUBLIC_BELLA_API_BASE_URL ?? "/api"

export async function getMe(): Promise<User | null> {
  const response = await fetch(`${apiBaseUrl}/v1/me`, {
    credentials: "include",
  })
  if (!response.ok) return null
  return response.json() as Promise<User>
}

export async function getOrganizations(): Promise<Organization[]> {
  const response = await fetch(`${apiBaseUrl}/v1/organizations`, {
    credentials: "include",
  })
  if (!response.ok) throw new Error("Could not load your organizations.")
  return response.json() as Promise<Organization[]>
}

export async function createOrganization(
  name: string,
): Promise<Organization> {
  const response = await fetch(`${apiBaseUrl}/v1/organizations`, {
    method: "POST",
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": crypto.randomUUID(),
    },
    body: JSON.stringify({ name }),
  })
  const body = await response.json()
  if (!response.ok) throw new Error(body.error ?? "Could not create the organization.")
  return body as Organization
}

export async function logout(): Promise<void> {
  await fetch(`${apiBaseUrl}/v1/auth/logout`, {
    method: "POST",
    credentials: "include",
  })
}

export async function getProviderCatalog(): Promise<ProviderDefinition[]> {
  const response = await fetch(`${apiBaseUrl}/v1/providers`, {
    credentials: "include",
  })
  if (!response.ok) throw new Error("Could not load the provider catalog.")
  return response.json() as Promise<ProviderDefinition[]>
}

export async function getProviderAccounts(
  organizationId: string,
): Promise<ProviderAccount[]> {
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts`,
    { credentials: "include" },
  )
  if (!response.ok) throw new Error("Could not load provider accounts.")
  return response.json() as Promise<ProviderAccount[]>
}

export async function connectProviderAccount({
  organizationId,
  workspaceId,
  provider,
  displayName,
  secret,
}: {
  organizationId: string
  workspaceId: string
  provider: string
  displayName: string
  secret: string
}): Promise<ProviderAccount> {
  const response = await fetch(
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
  )
  const body = await response.json()
  if (!response.ok) {
    throw new Error(body.error ?? "Could not connect the provider.")
  }
  return body as ProviderAccount
}

async function errorMessage(response: Response, fallback: string) {
  const text = await response.text()
  if (!text) return fallback
  try {
    const body = JSON.parse(text) as { error?: string }
    return body.error ?? fallback
  } catch {
    return fallback
  }
}

export async function deleteProviderAccount(
  organizationId: string,
  accountId: string,
): Promise<void> {
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts/${accountId}`,
    {
      method: "DELETE",
      credentials: "include",
    },
  )
  if (!response.ok) {
    throw new Error(
      await errorMessage(response, "Could not disconnect the provider."),
    )
  }
}

export async function updateProviderAccount(
  organizationId: string,
  accountId: string,
  displayName: string,
): Promise<ProviderAccount> {
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts/${accountId}`,
    {
      method: "PATCH",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ display_name: displayName }),
    },
  )
  const body = await response.json()
  if (!response.ok) {
    throw new Error(body.error ?? "Could not update the provider account.")
  }
  return body as ProviderAccount
}

export async function syncProviderAccount(
  organizationId: string,
  accountId: string,
): Promise<SyncOutcome> {
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/provider-accounts/${accountId}/sync`,
    {
      method: "POST",
      credentials: "include",
    },
  )
  const body = await response.json()
  if (!response.ok) {
    throw new Error(body.error ?? "Could not sync provider account.")
  }
  return body as SyncOutcome
}

export async function getUsageSummary({
  organizationId,
  start,
  end,
}: {
  organizationId: string
  start: string
  end: string
}): Promise<UsageSummary> {
  const params = new URLSearchParams({ start, end })
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/usage/summary?${params}`,
    { credentials: "include" },
  )
  if (!response.ok) {
    throw new Error(await errorMessage(response, "Could not load usage summary."))
  }
  return response.json() as Promise<UsageSummary>
}

export async function sendAgentMessage({
  organizationId,
  message,
}: {
  organizationId: string
  message: string
}): Promise<AgentMessageResponse> {
  const response = await fetch(
    `${apiBaseUrl}/v1/organizations/${organizationId}/agent/messages`,
    {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message }),
    },
  )
  if (!response.ok) {
    throw new Error(
      await errorMessage(
        response,
        "Bella could not reach the agent API. Restart the API on this branch and try again.",
      ),
    )
  }
  return response.json() as Promise<AgentMessageResponse>
}

export function getLoginUrl(): string {
  const returnTo = `${window.location.origin}/`
  return `${apiBaseUrl}/v1/auth/github/start?return_to=${encodeURIComponent(returnTo)}`
}

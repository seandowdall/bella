export type User = {
  id: string
  github_login: string
  name: string | null
  avatar_url: string | null
}

export type Workspace = {
  id: string
  slug: string
  name: string
}

export type Organization = {
  id: string
  slug: string
  name: string
  role: 'owner' | 'admin' | 'member'
  default_workspace: Workspace
}

export type ProviderDefinition = {
  id: string
  name: string
  category: string
  ingestion: 'usage_api' | 'cloud_billing' | 'connection_only'
  credential_label: string
  credential_placeholder: string
  docs_url: string
}

export type ProviderAccount = {
  id: string
  organization_id: string
  workspace_id: string
  workspace_name: string
  provider: string
  display_name: string
  credential_fingerprint: string
  status:
    | 'saved_unverified'
    | 'verified'
    | 'invalid_credentials'
    | 'insufficient_permissions'
    | 'validation_unavailable'
    | 'disabled'
  validated_at: string | null
  validation_error: string | null
  last_synced_at: string | null
  next_sync_at: string | null
  last_sync_error: string | null
  created_at: string
}

export type DailySpend = {
  date: string
  amount_micros: number
}

export type ModelBreakdown = {
  provider: string
  model: string
  amount_micros: number
  input_tokens: number
  output_tokens: number
  request_count: number
}

export type UsageSummary = {
  start: string
  end: string
  total_spend_micros: number
  input_tokens: number
  output_tokens: number
  request_count: number
  daily_spend: DailySpend[]
  model_breakdown: ModelBreakdown[]
}

export type SyncOutcome = {
  sync_run_id: string
  provider_account_id: string
  provider: string
  window_start: string
  window_end: string
  usage_buckets: number
  cost_snapshots: number
}

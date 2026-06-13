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
  created_at: string
}

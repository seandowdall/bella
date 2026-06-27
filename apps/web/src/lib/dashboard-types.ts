export type User = {
  id: string;
  github_login: string;
  primary_email: string | null;
  name: string | null;
  avatar_url: string | null;
};

export type Workspace = {
  id: string;
  slug: string;
  name: string;
};

export type Organization = {
  id: string;
  slug: string;
  name: string;
  role: "owner" | "admin" | "member";
  default_workspace: Workspace;
};

export type OrganizationRole = Organization["role"];

export type OrganizationMember = {
  user_id: string;
  github_login: string;
  name: string | null;
  avatar_url: string | null;
  primary_email: string | null;
  role: OrganizationRole;
  created_at: string;
};

export type OrganizationInvitation = {
  id: string;
  email: string;
  role: "admin" | "member";
  status: "pending" | "expired" | "revoked";
  invited_by_github_login: string;
  expires_at: string;
  created_at: string;
};

export type OrganizationMembers = {
  members: OrganizationMember[];
  invitations: OrganizationInvitation[];
};

export type ProviderDefinition = {
  id: string;
  name: string;
  category: string;
  ingestion: "usage_api" | "cloud_billing" | "connection_only";
  credential_label: string;
  credential_placeholder: string;
  docs_url: string;
};

export type ProviderAccount = {
  id: string;
  organization_id: string;
  workspace_id: string;
  workspace_name: string;
  provider: string;
  display_name: string;
  credential_fingerprint: string;
  status:
    | "saved_unverified"
    | "verified"
    | "invalid_credentials"
    | "insufficient_permissions"
    | "validation_unavailable"
    | "disabled";
  validated_at: string | null;
  validation_error: string | null;
  last_synced_at: string | null;
  next_sync_at: string | null;
  last_sync_error: string | null;
  created_at: string;
};

export type DailySpend = {
  date: string;
  amount_micros: number;
};

export type ModelBreakdown = {
  provider: string;
  model: string;
  amount_micros: number;
  input_tokens: number;
  output_tokens: number;
  request_count: number;
};

export type UsageSummary = {
  start: string;
  end: string;
  total_spend_micros: number;
  input_tokens: number;
  output_tokens: number;
  request_count: number;
  daily_spend: DailySpend[];
  model_breakdown: ModelBreakdown[];
};

export type SyncOutcome = {
  sync_run_id: string;
  provider_account_id: string;
  provider: string;
  window_start: string;
  window_end: string;
  usage_buckets: number;
  cost_snapshots: number;
};

export type AgentMessageResponse = {
  answer: string;
  metric_type: "provider_reported" | "estimated_live" | "reconciled" | "finalized";
  freshness: string | null;
  agent_mode: "deterministic" | "llm_assisted";
  sources: string[];
  suggestions: string[];
};

export type AgentLlmSettings = {
  id: string;
  display_name: string;
  provider: "openai" | "anthropic";
  model: string;
  credential_fingerprint: string;
  is_default: boolean;
};

export type AgentLlmSettingsList = {
  items: AgentLlmSettings[];
  default_id: string | null;
  mode: "deterministic" | "llm_assisted";
};

export type SlackTestMessage = {
  channel_id: string;
  message_ts: string;
};

export type SlackWorkspaceStatus = {
  team_id: string;
  team_name: string;
  status: "connected" | "needs_attention" | "disabled" | "uninstalled";
  status_reason: string | null;
  installed_at: string;
};

export type SlackChannelStatus = {
  id: string;
  channel_id: string;
  channel_name: string | null;
  channel_type: "public_channel" | "private_channel";
  status: "active" | "needs_attention" | "disabled" | "archived";
  discovered_by: "event" | "refresh" | "oauth" | "manual";
  last_seen_at: string;
};

export type SlackStatus = {
  installed: boolean;
  workspace: SlackWorkspaceStatus | null;
  channels: SlackChannelStatus[];
};

export type SlackInstallUrl = {
  install_url: string;
  expires_in: number;
};

export type IncidentStatus =
  | "triggered"
  | "acknowledged"
  | "mitigated"
  | "follow_up"
  | "triaging"
  | "investigating"
  | "identified"
  | "monitoring"
  | "resolved"
  | "false_positive";

export type IncidentSeverity = "unknown" | "info" | "low" | "medium" | "high" | "critical";

export type IncidentListItem = {
  id: string;
  title: string;
  status: IncidentStatus;
  severity: IncidentSeverity;
  source: string;
  fingerprint: string;
  signal_count: number;
  detected_at: string;
  updated_at: string;
  resolved_at: string | null;
};

export type SignalDetail = {
  id: string;
  source: string;
  signal_type: string;
  source_event_id: string | null;
  title: string;
  severity: IncidentSeverity;
  payload: unknown;
  received_at: string;
};

export type IncidentEventDetail = {
  id: string;
  event_type: string;
  title: string;
  body: string | null;
  metadata: unknown;
  created_at: string;
};

export type IncidentDetail = {
  id: string;
  organization_id: string;
  title: string;
  status: IncidentStatus;
  severity: IncidentSeverity;
  source: string;
  fingerprint: string;
  summary: string | null;
  impact: string | null;
  detected_at: string;
  resolved_at: string | null;
  metadata: unknown;
  signals: SignalDetail[];
  events: IncidentEventDetail[];
};

export type Integration = {
  id: string;
  integration_type: string;
  display_name: string;
  status: "connected" | "needs_attention" | "disabled";
  metadata: Record<string, unknown>;
  credential_fingerprint: string | null;
  api_token_fingerprint: string | null;
  created_at: string;
  updated_at: string;
};

export type PosthogSecretRotation = {
  integration: Integration;
  webhook_secret: string;
};

export type PosthogConnectionCheck = {
  ok: boolean;
  integration_id: string;
  posthog_host: string;
  posthog_project_id: string;
  observed_rows: number;
};

export type PosthogSyncOutcome = {
  sync_run_id: string;
  integration_id: string;
  window_start: string;
  window_end: string;
  signals_seen: number;
  signals_upserted: number;
  incident_candidates_created: number;
};

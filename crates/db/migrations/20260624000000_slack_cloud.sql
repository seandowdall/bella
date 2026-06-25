create table slack_installations (
    id uuid primary key,
    integration_id uuid not null references integrations(id) on delete cascade,
    organization_id uuid not null references organizations(id) on delete cascade,
    slack_team_id text not null,
    slack_team_name text not null,
    slack_enterprise_id text,
    slack_app_id text not null,
    slack_bot_user_id text not null,
    scopes text[] not null default '{}'::text[],
    status text not null default 'connected'
        check (status in ('connected', 'needs_attention', 'disabled', 'uninstalled')),
    status_reason text,
    installed_by uuid not null references users(id),
    installed_at timestamptz not null default now(),
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (integration_id),
    unique (organization_id, slack_team_id),
    check (char_length(slack_team_id) between 1 and 80),
    check (char_length(slack_team_name) between 1 and 120),
    check (slack_enterprise_id is null or char_length(slack_enterprise_id) between 1 and 80),
    check (char_length(slack_app_id) between 1 and 80),
    check (char_length(slack_bot_user_id) between 1 and 80)
);

create index slack_installations_integration_idx
on slack_installations(integration_id);

create index slack_installations_organization_status_idx
on slack_installations(organization_id, status);

create index slack_installations_team_idx
on slack_installations(slack_team_id);

create table slack_delivery_targets (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    slack_installation_id uuid not null references slack_installations(id) on delete cascade,
    slack_channel_id text not null,
    slack_channel_name text,
    channel_type text not null
        check (channel_type in ('public_channel', 'private_channel')),
    status text not null default 'active'
        check (status in ('active', 'needs_attention', 'disabled', 'archived')),
    discovered_by text not null
        check (discovered_by in ('event', 'refresh', 'oauth', 'manual')),
    last_seen_at timestamptz not null default now(),
    confirmed_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (slack_installation_id, slack_channel_id),
    check (char_length(slack_channel_id) between 1 and 80),
    check (slack_channel_name is null or char_length(slack_channel_name) between 1 and 120)
);

create index slack_delivery_targets_organization_status_idx
on slack_delivery_targets(organization_id, status);

create index slack_delivery_targets_installation_idx
on slack_delivery_targets(slack_installation_id);

create table slack_oauth_states (
    state_hash text primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    return_to text,
    expires_at timestamptz not null,
    consumed_at timestamptz,
    created_at timestamptz not null default now()
);

create index slack_oauth_states_organization_idx
on slack_oauth_states(organization_id);

create index slack_oauth_states_expires_at_idx
on slack_oauth_states(expires_at);

create table slack_events (
    id uuid primary key,
    slack_event_id text not null unique,
    slack_team_id text,
    slack_app_id text,
    slack_event_type text not null,
    organization_id uuid references organizations(id) on delete cascade,
    slack_installation_id uuid references slack_installations(id) on delete set null,
    status text not null default 'received'
        check (status in ('received', 'processed', 'ignored', 'failed')),
    last_error text,
    received_at timestamptz not null default now(),
    processed_at timestamptz,
    updated_at timestamptz not null default now(),
    check (char_length(slack_event_id) between 1 and 160),
    check (slack_team_id is null or char_length(slack_team_id) between 1 and 80),
    check (slack_app_id is null or char_length(slack_app_id) between 1 and 80),
    check (char_length(slack_event_type) between 1 and 120)
);

create index slack_events_installation_idx
on slack_events(slack_installation_id);

create index slack_events_status_received_idx
on slack_events(status, received_at);

create table incident_slack_threads (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    incident_id uuid not null references incidents(id) on delete cascade,
    slack_delivery_target_id uuid references slack_delivery_targets(id) on delete set null,
    slack_installation_id uuid references slack_installations(id) on delete set null,
    slack_channel_id text not null,
    slack_thread_ts text not null,
    status text not null default 'active'
        check (status in ('active', 'failed', 'archived')),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (incident_id, slack_delivery_target_id),
    unique (organization_id, slack_channel_id, slack_thread_ts),
    check (char_length(slack_channel_id) between 1 and 80),
    check (char_length(slack_thread_ts) between 1 and 80)
);

create index incident_slack_threads_organization_idx
on incident_slack_threads(organization_id);

create index incident_slack_threads_incident_idx
on incident_slack_threads(incident_id);

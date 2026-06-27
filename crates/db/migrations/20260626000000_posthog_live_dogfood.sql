alter table incidents drop constraint incidents_status_check;

alter table incidents
add constraint incidents_status_check
check (
    status in (
        'triggered',
        'acknowledged',
        'investigating',
        'mitigated',
        'resolved',
        'follow_up',
        'triaging',
        'identified',
        'monitoring',
        'false_positive'
    )
);

create table posthog_sync_runs (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    integration_id uuid not null references integrations(id) on delete cascade,
    natural_key text not null,
    status text not null
        check (status in ('running', 'succeeded', 'failed')),
    posthog_host text not null,
    posthog_project_id text not null,
    window_start timestamptz not null,
    window_end timestamptz not null,
    signals_seen integer not null default 0,
    signals_upserted integer not null default 0,
    incident_candidates_created integer not null default 0,
    error text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (window_start < window_end),
    check (char_length(natural_key) between 1 and 240),
    check (posthog_host ~ '^https?://'),
    check (char_length(posthog_project_id) between 1 and 120),
    unique (organization_id, integration_id, natural_key)
);

create index posthog_sync_runs_integration_window_idx
on posthog_sync_runs(integration_id, window_end desc);

create table posthog_sync_checkpoints (
    integration_id uuid primary key references integrations(id) on delete cascade,
    last_synced_at timestamptz not null,
    updated_at timestamptz not null default now()
);

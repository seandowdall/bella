create table integrations (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    workspace_id uuid references workspaces(id) on delete cascade,
    integration_type text not null,
    display_name text not null,
    status text not null default 'connected'
        check (status in ('connected', 'needs_attention', 'disabled')),
    metadata jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (organization_id, integration_type, display_name),
    check (integration_type ~ '^[a-z0-9]+(?:_[a-z0-9]+)*$'),
    check (char_length(display_name) between 1 and 120)
);

create index integrations_organization_id_idx
on integrations(organization_id);

create table incidents (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    title text not null,
    status text not null default 'triaging'
        check (status in ('triaging', 'investigating', 'identified', 'monitoring', 'resolved', 'false_positive')),
    severity text not null default 'unknown'
        check (severity in ('unknown', 'info', 'low', 'medium', 'high', 'critical')),
    source text not null,
    fingerprint text not null,
    summary text,
    impact text,
    started_at timestamptz,
    detected_at timestamptz not null default now(),
    resolved_at timestamptz,
    slack_channel_id text,
    slack_thread_ts text,
    metadata jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (source ~ '^[a-z0-9]+(?:_[a-z0-9]+)*$'),
    check (char_length(title) between 1 and 240),
    check (char_length(fingerprint) between 1 and 240)
);

create unique index incidents_open_fingerprint_idx
on incidents(organization_id, source, fingerprint)
where resolved_at is null;

create index incidents_organization_detected_idx
on incidents(organization_id, detected_at desc);

create table signals (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    integration_id uuid references integrations(id) on delete set null,
    incident_id uuid references incidents(id) on delete set null,
    source text not null,
    signal_type text not null,
    source_event_id text,
    fingerprint text not null,
    title text not null,
    severity text not null default 'unknown'
        check (severity in ('unknown', 'info', 'low', 'medium', 'high', 'critical')),
    payload jsonb not null,
    received_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    check (source ~ '^[a-z0-9]+(?:_[a-z0-9]+)*$'),
    check (signal_type ~ '^[a-z0-9]+(?:[._][a-z0-9]+)*$'),
    check (char_length(title) between 1 and 240),
    check (char_length(fingerprint) between 1 and 240)
);

create unique index signals_source_event_idx
on signals(organization_id, source, source_event_id)
where source_event_id is not null;

create index signals_incident_received_idx
on signals(incident_id, received_at desc);

create table incident_events (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    incident_id uuid not null references incidents(id) on delete cascade,
    event_type text not null,
    title text not null,
    body text,
    metadata jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    check (event_type ~ '^[a-z0-9]+(?:[._][a-z0-9]+)*$'),
    check (char_length(title) between 1 and 240)
);

create index incident_events_incident_created_idx
on incident_events(incident_id, created_at asc);

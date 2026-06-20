create table sdk_usage_events (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    provider_account_id uuid not null references provider_accounts(id) on delete cascade,
    event_id text not null,
    provider text not null,
    model text not null default '',
    operation text not null default '',
    status text not null check (status in ('succeeded', 'failed')),
    started_at timestamptz not null,
    ended_at timestamptz not null,
    input_tokens bigint not null default 0,
    output_tokens bigint not null default 0,
    total_tokens bigint not null default 0,
    cost_micros bigint,
    currency text not null default 'usd',
    request_metadata jsonb not null default '{}'::jsonb,
    error_message text,
    created_at timestamptz not null default now(),
    check (started_at <= ended_at),
    check (char_length(event_id) between 1 and 160),
    unique (organization_id, event_id)
);

create index sdk_usage_events_account_started_idx
on sdk_usage_events(provider_account_id, started_at desc);

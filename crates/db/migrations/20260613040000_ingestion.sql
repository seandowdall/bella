alter table provider_accounts
add column next_sync_at timestamptz,
add column last_sync_error text;

create table provider_sync_runs (
    id uuid primary key,
    provider_account_id uuid not null references provider_accounts(id) on delete cascade,
    provider text not null,
    status text not null check (status in ('running', 'succeeded', 'failed')),
    window_start timestamptz not null,
    window_end timestamptz not null,
    error text,
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    check (window_start < window_end)
);

create index provider_sync_runs_account_started_idx
on provider_sync_runs(provider_account_id, started_at desc);

create table provider_sync_checkpoints (
    provider_account_id uuid not null references provider_accounts(id) on delete cascade,
    stream text not null check (stream in ('usage', 'costs')),
    checkpoint_at timestamptz not null,
    updated_at timestamptz not null default now(),
    primary key (provider_account_id, stream)
);

create table provider_raw_payloads (
    id uuid primary key,
    provider_account_id uuid not null references provider_accounts(id) on delete cascade,
    sync_run_id uuid not null references provider_sync_runs(id) on delete cascade,
    provider text not null,
    endpoint text not null,
    request_window_start timestamptz not null,
    request_window_end timestamptz not null,
    page_cursor text not null default '',
    payload_hash text not null,
    payload jsonb not null,
    created_at timestamptz not null default now(),
    unique (provider_account_id, endpoint, request_window_start, request_window_end, page_cursor, payload_hash)
);

create table usage_buckets (
    id uuid primary key,
    provider_account_id uuid not null references provider_accounts(id) on delete cascade,
    provider text not null,
    bucket_start timestamptz not null,
    bucket_end timestamptz not null,
    model text not null default '',
    project_external_id text not null default '',
    user_external_id text not null default '',
    api_key_external_id text not null default '',
    operation text not null default '',
    input_tokens bigint not null default 0,
    output_tokens bigint not null default 0,
    request_count bigint not null default 0,
    raw_payload_id uuid references provider_raw_payloads(id) on delete set null,
    updated_at timestamptz not null default now(),
    check (bucket_start < bucket_end),
    unique (
        provider_account_id,
        bucket_start,
        bucket_end,
        model,
        project_external_id,
        user_external_id,
        api_key_external_id,
        operation
    )
);

create index usage_buckets_account_date_idx
on usage_buckets(provider_account_id, bucket_start, bucket_end);

create table cost_snapshots (
    id uuid primary key,
    provider_account_id uuid not null references provider_accounts(id) on delete cascade,
    provider text not null,
    bucket_start timestamptz not null,
    bucket_end timestamptz not null,
    line_item text not null default '',
    model text not null default '',
    project_external_id text not null default '',
    amount_micros bigint not null,
    currency text not null default 'usd',
    raw_payload_id uuid references provider_raw_payloads(id) on delete set null,
    updated_at timestamptz not null default now(),
    check (bucket_start < bucket_end),
    unique (
        provider_account_id,
        bucket_start,
        bucket_end,
        line_item,
        model,
        project_external_id,
        currency
    )
);

create index cost_snapshots_account_date_idx
on cost_snapshots(provider_account_id, bucket_start, bucket_end);

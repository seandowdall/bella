create table incident_delivery_jobs (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    incident_id uuid not null references incidents(id) on delete cascade,
    delivery_type text not null,
    dedupe_key text not null unique,
    status text not null default 'pending'
        check (status in ('pending', 'processing', 'delivered', 'failed')),
    attempts integer not null default 0 check (attempts >= 0),
    available_at timestamptz not null default now(),
    locked_at timestamptz,
    last_error text,
    delivered_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (delivery_type ~ '^[a-z0-9]+(?:[._][a-z0-9]+)*$'),
    check (char_length(dedupe_key) between 1 and 240)
);

create index incident_delivery_jobs_claim_idx
on incident_delivery_jobs(status, available_at, created_at)
where status in ('pending', 'processing');

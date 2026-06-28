create table github_installation_flows (
    state_hash text primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    return_to text,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index github_installation_flows_expires_at_idx
on github_installation_flows(expires_at);

create table github_repositories (
    id uuid primary key,
    integration_id uuid not null references integrations(id) on delete cascade,
    github_repository_id bigint not null,
    full_name text not null,
    private boolean not null,
    default_branch text not null,
    html_url text not null,
    selected boolean not null default true,
    last_seen_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (integration_id, github_repository_id),
    check (char_length(full_name) between 1 and 240),
    check (char_length(default_branch) between 1 and 240),
    check (char_length(html_url) between 1 and 500)
);

create index github_repositories_integration_id_idx
on github_repositories(integration_id);

create table users (
    id uuid primary key,
    github_user_id bigint not null unique,
    github_login text not null,
    name text,
    avatar_url text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table cli_login_requests (
    id uuid primary key,
    poll_secret_hash text not null,
    user_id uuid references users(id) on delete cascade,
    api_token text,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index cli_login_requests_expires_at_idx on cli_login_requests(expires_at);

create table oauth_flows (
    state_hash text primary key,
    flow_kind text not null check (flow_kind in ('web', 'cli')),
    cli_request_id uuid references cli_login_requests(id) on delete cascade,
    browser_nonce_hash text,
    return_to text,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index oauth_flows_expires_at_idx on oauth_flows(expires_at);

create table web_sessions (
    token_hash text primary key,
    user_id uuid not null references users(id) on delete cascade,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index web_sessions_user_id_idx on web_sessions(user_id);
create index web_sessions_expires_at_idx on web_sessions(expires_at);

create table api_tokens (
    token_hash text primary key,
    user_id uuid not null references users(id) on delete cascade,
    label text not null,
    last_used_at timestamptz,
    created_at timestamptz not null default now(),
    revoked_at timestamptz
);

create index api_tokens_user_id_idx on api_tokens(user_id);

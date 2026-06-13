create table provider_accounts (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    workspace_id uuid not null references workspaces(id) on delete cascade,
    provider text not null,
    display_name text not null,
    credential_ciphertext bytea not null,
    credential_nonce bytea not null,
    credential_fingerprint text not null,
    status text not null default 'connected'
        check (status in ('connected', 'needs_attention', 'disabled')),
    last_synced_at timestamptz,
    created_by uuid not null references users(id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (workspace_id, provider, display_name),
    check (provider ~ '^[a-z0-9]+(?:_[a-z0-9]+)*$'),
    check (char_length(display_name) between 1 and 80),
    check (octet_length(credential_nonce) = 12),
    check (char_length(credential_fingerprint) between 4 and 16)
);

create index provider_accounts_organization_id_idx
on provider_accounts(organization_id);

create index provider_accounts_workspace_id_idx
on provider_accounts(workspace_id);

create table integration_credentials (
    id uuid primary key,
    integration_id uuid not null references integrations(id) on delete cascade,
    kind text not null,
    credential_ciphertext bytea not null,
    credential_nonce bytea not null,
    credential_fingerprint text not null,
    created_by uuid not null references users(id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (integration_id, kind),
    check (kind ~ '^[a-z0-9]+(?:_[a-z0-9]+)*$'),
    check (octet_length(credential_nonce) = 12),
    check (char_length(credential_fingerprint) between 4 and 16)
);

create index integration_credentials_integration_id_idx
on integration_credentials(integration_id);

create table organization_agent_llm_settings (
    organization_id uuid primary key references organizations(id) on delete cascade,
    provider text not null check (provider in ('openai', 'anthropic')),
    model text not null,
    api_key_ciphertext bytea not null,
    api_key_nonce bytea not null,
    api_key_fingerprint text not null,
    created_by uuid not null references users(id),
    updated_by uuid not null references users(id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (char_length(model) between 1 and 120),
    check (octet_length(api_key_nonce) = 12),
    check (char_length(api_key_fingerprint) between 4 and 16)
);

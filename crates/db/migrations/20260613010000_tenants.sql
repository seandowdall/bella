create table organizations (
    id uuid primary key,
    slug text not null unique,
    name text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$')
);

create table organization_memberships (
    organization_id uuid not null references organizations(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    role text not null check (role in ('owner', 'admin', 'member')),
    created_at timestamptz not null default now(),
    primary key (organization_id, user_id)
);

create index organization_memberships_user_id_idx
on organization_memberships(user_id);

create table workspaces (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    slug text not null,
    name text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (organization_id, slug),
    check (slug ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$')
);

create table organization_create_requests (
    user_id uuid not null references users(id) on delete cascade,
    idempotency_key text not null,
    request_hash text not null,
    organization_id uuid not null references organizations(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (user_id, idempotency_key)
);

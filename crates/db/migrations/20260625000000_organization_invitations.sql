alter table users
add column primary_email text;

create table organization_invitations (
    id uuid primary key,
    organization_id uuid not null references organizations(id) on delete cascade,
    email text not null,
    role text not null check (role in ('admin', 'member')),
    token_hash text not null unique,
    invited_by_user_id uuid not null references users(id) on delete cascade,
    accepted_by_user_id uuid references users(id) on delete set null,
    expires_at timestamptz not null,
    accepted_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now(),
    check (email = lower(email)),
    check (accepted_at is null or revoked_at is null)
);

create index organization_invitations_organization_id_idx
on organization_invitations(organization_id);

create index organization_invitations_expires_at_idx
on organization_invitations(expires_at);

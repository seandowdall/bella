alter table provider_accounts
add column validated_at timestamptz,
add column validation_error text;

alter table provider_accounts
drop constraint provider_accounts_status_check;

update provider_accounts
set status = 'saved_unverified'
where status = 'connected';

alter table provider_accounts
alter column status set default 'saved_unverified';

alter table provider_accounts
add constraint provider_accounts_status_check
check (
    status in (
        'saved_unverified',
        'verified',
        'invalid_credentials',
        'insufficient_permissions',
        'validation_unavailable',
        'disabled'
    )
);

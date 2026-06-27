alter table api_tokens
add column expires_at timestamptz not null default (now() + interval '90 days');

create index api_tokens_expires_at_idx on api_tokens(expires_at);

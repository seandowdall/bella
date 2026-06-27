create table rate_limit_hits (
    key text not null,
    hit_at timestamptz not null default now(),
    check (char_length(key) between 1 and 240)
);

create index rate_limit_hits_key_hit_at_idx on rate_limit_hits(key, hit_at);

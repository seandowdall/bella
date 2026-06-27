delete from slack_oauth_states;

alter table slack_oauth_states
add column browser_nonce_hash text not null;

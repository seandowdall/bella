# Bella Agent Notes

Bella is an open source AI cost visibility platform.

Keep setup and ingestion paths deterministic and retryable. Prefer stable natural
keys for organizations, workspaces, provider accounts, model names, usage events,
and cost snapshots. Agent-facing commands should be non-interactive by default and
return structured output when JSON flags are introduced.

Before committing Rust changes, run `cargo clippy --workspace --all-targets -- -D warnings`
in addition to formatting and tests. Treat clippy warnings as CI-blocking.

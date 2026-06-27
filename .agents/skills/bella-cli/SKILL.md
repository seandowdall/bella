---
name: bella-cli
description: Install and use the Bella CLI beta in QA or production. Use when an agent needs to run Bella CLI commands, authenticate with Bella, inspect organizations/providers/integrations, or test CLI behavior against app.qa.bella.md or app.bella.md.
user-invocable: true
allowed-tools: Bash(curl *), Bash(sh *), Bash(bella *)
---

# Bella CLI Beta

Use the Bella CLI beta for agent-facing Bella operations. GitHub OAuth is the
only supported interactive login method during beta.

## Install

Install the latest QA channel CLI:

```sh
curl -fsSL https://raw.githubusercontent.com/seandowdall/bella/main/scripts/install-bella-cli.sh | sh -s -- --channel qa
```

Install the latest production channel CLI:

```sh
curl -fsSL https://raw.githubusercontent.com/seandowdall/bella/main/scripts/install-bella-cli.sh | sh -s -- --channel prod
```

The installer detects macOS Apple Silicon, macOS Intel, and Linux x86_64,
downloads the matching GitHub release asset, verifies its SHA-256 checksum, and
installs `bella` into `/usr/local/bin` by default.

Use a custom install directory when `/usr/local/bin` is not writable:

```sh
curl -fsSL https://raw.githubusercontent.com/seandowdall/bella/main/scripts/install-bella-cli.sh | sh -s -- --channel qa --install-dir "$HOME/.local/bin"
```

## Environments

Production is the CLI default:

```sh
bella login
bella whoami
```

Use QA explicitly:

```sh
bella --environment qa login
bella --environment qa whoami
```

For non-interactive agent sessions, prefer the environment variable:

```sh
BELLA_ENVIRONMENT=qa bella config
BELLA_ENVIRONMENT=prod bella config
```

`BELLA_API_BASE_URL` and `--api-base-url` override `BELLA_ENVIRONMENT`.

## Authentication Notes

- `bella login` opens GitHub OAuth in a browser and waits for completion.
- CLI credentials are scoped to the exact API URL, so QA and prod logins are separate.
- The CLI stores its Bella API token in `~/.config/bella/credentials.json` with owner-only permissions on Unix systems.
- The CLI does not bypass Bella API allow lists; the target API applies its configured GitHub email/org authorization.

## Common Commands

```sh
bella config
bella whoami
bella organizations list
bella providers catalog
bella providers accounts
bella integrations list
```

For JSON output:

```sh
bella --json --environment qa organizations list
```

# Bella CLI Beta

The Bella CLI is distributed as a beta for technical users and self-hosted
operators. GitHub OAuth is the only supported login method during the beta.

## Automatic Channel Releases

The CLI beta is released automatically when CLI-relevant changes merge:

- Merges to `qa` publish the `bella-cli-qa` prerelease.
- Merges to `main` publish the `bella-cli-prod` prerelease.

The release workflow only runs for changes to the CLI, release workflow,
installer, lockfile, or CLI beta docs.

## Install With The Script

Install the latest QA channel:

```sh
curl -fsSL https://raw.githubusercontent.com/seandowdall/bella/main/scripts/install-bella-cli.sh | sh -s -- --channel qa
```

Install the latest production channel:

```sh
curl -fsSL https://raw.githubusercontent.com/seandowdall/bella/main/scripts/install-bella-cli.sh | sh -s -- --channel prod
```

The installer detects macOS Apple Silicon, macOS Intel, and Linux x86_64,
downloads the matching release asset, verifies the SHA-256 checksum, and installs
the `bella` binary into `/usr/local/bin` by default.

Use a custom install directory when needed:

```sh
curl -fsSL https://raw.githubusercontent.com/seandowdall/bella/main/scripts/install-bella-cli.sh | sh -s -- --channel qa --install-dir "$HOME/.local/bin"
```

## Manual Install From GitHub Releases

Download the release archive for your platform from the `bella-cli-qa` or
`bella-cli-prod` GitHub release, verify the checksum, then install the `bella`
binary somewhere on your `PATH`:

```sh
shasum -a 256 -c bella-cli-linux-x86_64.tar.gz.sha256
tar -xzf bella-cli-linux-x86_64.tar.gz
sudo install -m 0755 bella-cli-linux-x86_64/bella /usr/local/bin/bella
```

On macOS, choose the `macos-aarch64` archive for Apple Silicon or the
`macos-x86_64` archive for Intel Macs.

You can also install from source when Rust is available:

```sh
cargo install --git https://github.com/seandowdall/bella --branch main bella-cli
```

## Environments

Production is the default environment:

```sh
bella login
bella whoami
```

Use QA explicitly when testing `app.qa.bella.md`:

```sh
bella --environment qa login
bella --environment qa whoami
```

For local development, use the local shortcut or an explicit API URL:

```sh
bella --environment local login
bella --api-base-url http://127.0.0.1:3000 login
```

Environment variables are supported for non-interactive setup:

```sh
BELLA_ENVIRONMENT=qa bella whoami
BELLA_API_BASE_URL=https://api.qa.bella.md bella whoami
```

`BELLA_API_BASE_URL` and `--api-base-url` always override `BELLA_ENVIRONMENT`.

## Authentication

`bella login` opens a browser to GitHub OAuth and waits for Bella to complete
the login. The production API is `https://api.bella.md`; the QA API is
`https://api.qa.bella.md`. GitHub redirects back through the matching Bella app
environment.

The CLI stores its Bella API token in `~/.config/bella/credentials.json` with
owner-only permissions on Unix systems. Provider API keys are sent to the Bella
API for encrypted storage and are not written to this local credential file.

Use logout to revoke the token and remove the local credential:

```sh
bella logout
```

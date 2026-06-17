#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
env_file="${1:-${ENV_FILE:-$script_dir/.env}}"

failures=0

fail() {
  printf 'FAIL %s\n' "$1" >&2
  failures=$((failures + 1))
}

ok() {
  printf 'OK   %s\n' "$1"
}

require_command() {
  if command -v "$1" >/dev/null 2>&1; then
    ok "$1 is installed"
  else
    fail "$1 is not installed"
  fi
}

require_value() {
  name="$1"
  value="$(eval "printf '%s' \"\${$name:-}\"")"
  if [ -n "$value" ]; then
    ok "$name is set"
  else
    fail "$name must be set"
  fi
}

reject_placeholder() {
  name="$1"
  value="$(eval "printf '%s' \"\${$name:-}\"")"
  case "$value" in
    "" | *"..."* | *"change-me"* | "your_"* | *"<"*">"*)
      fail "$name still contains a placeholder"
      ;;
    *)
      ok "$name is not a placeholder"
      ;;
  esac
}

require_https_url() {
  name="$1"
  value="$(eval "printf '%s' \"\${$name:-}\"")"
  case "$value" in
    https://*)
      ok "$name uses HTTPS"
      ;;
    *)
      fail "$name must use https://"
      ;;
  esac
  case "$value" in
    */)
      fail "$name must not end with a trailing slash"
      ;;
  esac
}

if [ ! -f "$env_file" ]; then
  fail "missing env file: $env_file"
  printf '\nCreate one with:\n  cp deploy/hetzner/.env.example deploy/hetzner/.env\n' >&2
  exit 2
fi

set -a
. "$env_file"
set +a

require_command docker
require_command curl
require_command openssl
require_command gzip

if docker compose version >/dev/null 2>&1; then
  ok "Docker Compose plugin is installed"
else
  fail "Docker Compose plugin is not installed"
fi

for name in \
  BELLA_DOMAIN \
  ACME_EMAIL \
  POSTGRES_DB \
  POSTGRES_USER \
  POSTGRES_PASSWORD \
  DATABASE_URL \
  BELLA_PUBLIC_API_URL \
  BELLA_WEB_URL \
  BELLA_SECURE_COOKIES \
  BELLA_API_BIND_ADDR \
  BELLA_CREDENTIAL_ENCRYPTION_KEY \
  GITHUB_OAUTH_CLIENT_ID \
  GITHUB_OAUTH_CLIENT_SECRET
do
  require_value "$name"
  reject_placeholder "$name"
done

case "${BELLA_DOMAIN:-}" in
  http://* | https://* | */*)
    fail "BELLA_DOMAIN must be a hostname, not a URL or path"
    ;;
  *.*)
    ok "BELLA_DOMAIN looks like a hostname"
    ;;
  *)
    fail "BELLA_DOMAIN should be a fully qualified hostname"
    ;;
esac

case "${ACME_EMAIL:-}" in
  *@*.*)
    ok "ACME_EMAIL looks like an email address"
    ;;
  *)
    fail "ACME_EMAIL should be a real email address for TLS notices"
    ;;
esac

require_https_url BELLA_PUBLIC_API_URL
require_https_url BELLA_WEB_URL

expected_api_url="https://${BELLA_DOMAIN}/api"
expected_web_url="https://${BELLA_DOMAIN}"
if [ "${BELLA_PUBLIC_API_URL:-}" = "$expected_api_url" ]; then
  ok "BELLA_PUBLIC_API_URL matches BELLA_DOMAIN"
else
  fail "BELLA_PUBLIC_API_URL must be $expected_api_url"
fi

if [ "${BELLA_WEB_URL:-}" = "$expected_web_url" ]; then
  ok "BELLA_WEB_URL matches BELLA_DOMAIN"
else
  fail "BELLA_WEB_URL must be $expected_web_url"
fi

if [ "${BELLA_SECURE_COOKIES:-}" = "true" ]; then
  ok "BELLA_SECURE_COOKIES is true"
else
  fail "BELLA_SECURE_COOKIES must be true for production HTTPS"
fi

if [ "${BELLA_API_BIND_ADDR:-}" = "0.0.0.0:3000" ]; then
  ok "BELLA_API_BIND_ADDR exposes the API inside Docker"
else
  fail "BELLA_API_BIND_ADDR must be 0.0.0.0:3000 for this Compose bundle"
fi

case "${DATABASE_URL:-}" in
  postgres://*@postgres:5432/"${POSTGRES_DB:-}")
    ok "DATABASE_URL points at the private Postgres service"
    ;;
  *)
    fail "DATABASE_URL must point at postgres:5432/${POSTGRES_DB:-bella}"
    ;;
esac

decoded_key_bytes="$(
  printf '%s' "${BELLA_CREDENTIAL_ENCRYPTION_KEY:-}" \
    | openssl base64 -d -A 2>/dev/null \
    | wc -c \
    | tr -d ' '
)"
if [ "$decoded_key_bytes" = "32" ]; then
  ok "BELLA_CREDENTIAL_ENCRYPTION_KEY decodes to 32 bytes"
else
  fail "BELLA_CREDENTIAL_ENCRYPTION_KEY must be base64 for exactly 32 bytes"
fi

if [ "$failures" -gt 0 ]; then
  printf '\nPreflight failed with %s problem(s).\n' "$failures" >&2
  exit 1
fi

printf '\nPreflight passed for %s\n' "$env_file"

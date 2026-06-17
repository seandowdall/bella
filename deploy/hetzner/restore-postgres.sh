#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/docker-compose.yml}"
env_file="${ENV_FILE:-$script_dir/.env}"
backup_file="${1:-}"

if [ -z "$backup_file" ]; then
  echo "usage: deploy/hetzner/restore-postgres.sh backups/hetzner/bella-postgres-YYYYMMDDTHHMMSSZ.sql.gz" >&2
  exit 2
fi

if [ ! -f "$backup_file" ]; then
  echo "missing backup file: $backup_file" >&2
  exit 2
fi

if [ "${BELLA_CONFIRM_RESTORE:-}" != "yes" ]; then
  echo "refusing to restore without BELLA_CONFIRM_RESTORE=yes" >&2
  exit 2
fi

if [ ! -f "$env_file" ]; then
  echo "missing env file: $env_file" >&2
  exit 2
fi

set -a
. "$env_file"
set +a

gzip -dc "$backup_file" | docker compose --env-file "$env_file" -f "$compose_file" exec -T postgres \
  sh -c 'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"'

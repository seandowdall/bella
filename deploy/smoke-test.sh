#!/usr/bin/env sh
set -eu

base_url="${1:-}"

if [ -z "$base_url" ]; then
  echo "usage: deploy/smoke-test.sh https://bella.example.com" >&2
  exit 2
fi

base_url="${base_url%/}"

check() {
  path="$1"
  url="$base_url$path"
  status="$(curl -fsS -o /dev/null -w '%{http_code}' "$url")"
  if [ "$status" != "200" ]; then
    echo "FAIL $path returned HTTP $status" >&2
    exit 1
  fi
  echo "OK   $path"
}

check "/"
check "/api/health"

cat <<EOF

Manual smoke tests:
  bella --api-base-url $base_url/api login
  bella --api-base-url $base_url/api whoami
  bella --api-base-url $base_url/api organizations list
  bella --api-base-url $base_url/api providers catalog
  bella --api-base-url $base_url/api logout
EOF

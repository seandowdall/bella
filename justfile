set dotenv-load

dev:
    docker compose up -d
    cargo run -p bella-api

dev-watch:
    docker compose up -d
    cargo watch -x "run -p bella-api"

docker:
    docker compose up -d

postgres:
    docker compose up -d postgres

pgweb:
    docker compose up -d pgweb

api:
    cargo run -p bella-api

web:
    bun run --cwd apps/web dev

site:
    bun run --cwd apps/site dev

cli *args:
    cargo run -p bella-cli -- {{args}}

stop:
    docker compose down

reset-db:
    docker compose down -v
    docker compose up -d postgres pgweb

hetzner-config env_file="deploy/hetzner/.env":
    @env -u ACME_EMAIL -u BELLA_API_BIND_ADDR -u BELLA_CREDENTIAL_ENCRYPTION_KEY -u BELLA_DOMAIN -u BELLA_INTERNAL_API_URL -u BELLA_PUBLIC_API_URL -u BELLA_SECURE_COOKIES -u BELLA_WEB_URL -u DATABASE_URL -u GITHUB_OAUTH_CLIENT_ID -u GITHUB_OAUTH_CLIENT_SECRET -u POSTGRES_DB -u POSTGRES_PASSWORD -u POSTGRES_USER docker compose --env-file {{env_file}} -f deploy/hetzner/docker-compose.yml config

hetzner-preflight env_file="deploy/hetzner/.env":
    deploy/hetzner/preflight.sh {{env_file}}

hetzner-up env_file="deploy/hetzner/.env":
    deploy/hetzner/preflight.sh {{env_file}}
    @env -u ACME_EMAIL -u BELLA_API_BIND_ADDR -u BELLA_CREDENTIAL_ENCRYPTION_KEY -u BELLA_DOMAIN -u BELLA_INTERNAL_API_URL -u BELLA_PUBLIC_API_URL -u BELLA_SECURE_COOKIES -u BELLA_WEB_URL -u DATABASE_URL -u GITHUB_OAUTH_CLIENT_ID -u GITHUB_OAUTH_CLIENT_SECRET -u POSTGRES_DB -u POSTGRES_PASSWORD -u POSTGRES_USER docker compose --env-file {{env_file}} -f deploy/hetzner/docker-compose.yml up -d --build

hetzner-logs env_file="deploy/hetzner/.env":
    @env -u ACME_EMAIL -u BELLA_API_BIND_ADDR -u BELLA_CREDENTIAL_ENCRYPTION_KEY -u BELLA_DOMAIN -u BELLA_INTERNAL_API_URL -u BELLA_PUBLIC_API_URL -u BELLA_SECURE_COOKIES -u BELLA_WEB_URL -u DATABASE_URL -u GITHUB_OAUTH_CLIENT_ID -u GITHUB_OAUTH_CLIENT_SECRET -u POSTGRES_DB -u POSTGRES_PASSWORD -u POSTGRES_USER docker compose --env-file {{env_file}} -f deploy/hetzner/docker-compose.yml logs -f caddy bella-api bella-web postgres

hetzner-down env_file="deploy/hetzner/.env":
    @env -u ACME_EMAIL -u BELLA_API_BIND_ADDR -u BELLA_CREDENTIAL_ENCRYPTION_KEY -u BELLA_DOMAIN -u BELLA_INTERNAL_API_URL -u BELLA_PUBLIC_API_URL -u BELLA_SECURE_COOKIES -u BELLA_WEB_URL -u DATABASE_URL -u GITHUB_OAUTH_CLIENT_ID -u GITHUB_OAUTH_CLIENT_SECRET -u POSTGRES_DB -u POSTGRES_PASSWORD -u POSTGRES_USER docker compose --env-file {{env_file}} -f deploy/hetzner/docker-compose.yml down

hetzner-backup:
    deploy/hetzner/backup-postgres.sh

hetzner-smoke url:
    deploy/smoke-test.sh {{url}}

fmt:
    cargo fmt --all

check:
    cargo check --workspace

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

verify: fmt check lint test

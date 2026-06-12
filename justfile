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

cli *args:
    cargo run -p bella-cli -- {{args}}

stop:
    docker compose down

reset-db:
    docker compose down -v
    docker compose up -d postgres pgweb

fmt:
    cargo fmt --all

check:
    cargo check --workspace

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

verify: fmt check lint test

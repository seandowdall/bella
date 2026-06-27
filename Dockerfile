FROM rust:1.88-bookworm AS builder

WORKDIR /app
COPY . .

RUN cargo build --release -p bella-api -p bella-worker

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 bella

COPY --from=builder /app/target/release/bella-api /usr/local/bin/bella-api
COPY --from=builder /app/target/release/bella-worker /usr/local/bin/bella-worker

USER bella

CMD ["sh", "-c", "exec ${BELLA_PROCESS:-bella-api}"]

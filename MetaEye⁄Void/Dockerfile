# Stage 1: Hermetic Offline Verification & Build
FROM rust:1.82-slim-bookworm AS builder
WORKDIR /workspace
COPY Cargo.toml build.rs sqlx-data.json ./
COPY contracts ./contracts
COPY proto ./proto
COPY src ./src

ENV NEEDLE_TELEMETRY=0 \
    DO_NOT_TRACK=1 \
    SQLX_OFFLINE=true

RUN cargo test --release -- --nocapture && \
    cargo build --release

# Stage 2: Distroless Non-Root Sandbox Runtime (0-Cost Minimal RAM Footprint)
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
USER nonroot:nonroot

ENV NEEDLE_TELEMETRY=0 \
    DO_NOT_TRACK=1 \
    PORT=8080 \
    RUST_LOG=info

COPY --from=builder --chown=nonroot:nonroot /workspace/target/release/needle-discord-metacog /app/needle-discord-metacog
COPY --from=builder --chown=nonroot:nonroot /workspace/contracts /app/contracts
COPY --from=builder --chown=nonroot:nonroot /workspace/proto /app/proto

EXPOSE 8080 50051
ENTRYPOINT ["/app/needle-discord-metacog"]

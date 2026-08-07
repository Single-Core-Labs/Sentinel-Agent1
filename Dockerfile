# Sentinel AI — container image
# Builds the sentinel CLI and ships it in a minimal Debian runtime.

FROM rust:bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release --locked -p sentinel-cli --bin sentinel

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/sentinel /usr/local/bin/sentinel
ENTRYPOINT ["sentinel"]
CMD ["ai"]

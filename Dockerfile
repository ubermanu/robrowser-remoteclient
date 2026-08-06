FROM rust:1.91-slim-bookworm AS build

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

# debug = true in the release profile is for local profiling; the image only
# needs the stripped binary
RUN cargo build --release && \
  strip target/release/robrowser-remoteclient

FROM debian:bookworm-slim

LABEL org.opencontainers.image.description="Serves assets over HTTP for a roBrowser client."

COPY --from=build /src/target/release/robrowser-remoteclient /usr/local/bin/

# /client is a mount point, so its ownership comes from the host
RUN mkdir -p /client

EXPOSE 8080

USER nobody

ENTRYPOINT ["robrowser-remoteclient"]
CMD ["/client"]

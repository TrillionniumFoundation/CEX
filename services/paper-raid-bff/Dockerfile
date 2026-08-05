FROM --platform=linux/amd64 docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb AS builder
WORKDIR /src
COPY services/paper-raid-bff/docker/workspace.Cargo.toml Cargo.toml
COPY services/paper-raid-bff/docker/Cargo.lock Cargo.lock
COPY crates/hepta-paper-raid-contracts crates/hepta-paper-raid-contracts
COPY services/paper-raid-bff/Cargo.toml services/paper-raid-bff/Cargo.toml
COPY services/paper-raid-bff/src services/paper-raid-bff/src
COPY services/paper-raid-bff/migrations services/paper-raid-bff/migrations
RUN cargo fetch --locked
RUN cargo build --locked --offline --release -p paper-raid-bff

FROM --platform=linux/amd64 gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98
COPY --from=builder --chown=65532:65532 /src/target/release/paper-raid-bff /paper-raid-bff
USER 65532:65532
EXPOSE 7020
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 CMD ["/paper-raid-bff", "--probe-ready"]
ENTRYPOINT ["/paper-raid-bff"]

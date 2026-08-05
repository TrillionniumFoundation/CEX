ARG SOURCE_DATE_EPOCH=0
FROM --platform=linux/amd64 docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb AS builder
ARG SOURCE_DATE_EPOCH
ENV SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH}
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
ARG PAPER_RAID_BFF_REVISION
ARG PAPER_RAID_BFF_CREATED
ARG PAPER_RAID_BFF_SBOM_SHA256
ARG PAPER_RAID_BFF_CARGO_LOCK_SHA256
LABEL org.opencontainers.image.title="Hepta Paper Raid BFF" \
      org.opencontainers.image.description="Same-origin Consumer Edge for the Hepta Paper Raid alpha" \
      org.opencontainers.image.source="https://github.com/TrillionniumFoundation/CEX" \
      org.opencontainers.image.version="0.1.0" \
      org.opencontainers.image.revision="${PAPER_RAID_BFF_REVISION}" \
      org.opencontainers.image.created="${PAPER_RAID_BFF_CREATED}" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.vendor="Trillionnium Foundation" \
      org.opencontainers.image.sbom="/usr/share/doc/paper-raid-bff/sbom.cdx.json" \
      org.trillionnium.cargo-lock.sha256="${PAPER_RAID_BFF_CARGO_LOCK_SHA256}" \
      org.trillionnium.sbom.sha256="${PAPER_RAID_BFF_SBOM_SHA256}"
COPY --from=builder --chown=65532:65532 /src/target/release/paper-raid-bff /paper-raid-bff
COPY --chown=65532:65532 services/paper-raid-bff/docker/sbom.cdx.json /usr/share/doc/paper-raid-bff/sbom.cdx.json
USER 65532:65532
EXPOSE 7020
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 CMD ["/paper-raid-bff", "--probe-ready"]
ENTRYPOINT ["/paper-raid-bff"]

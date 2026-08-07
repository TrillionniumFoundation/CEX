# syntax=docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89
FROM docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb AS workspace

WORKDIR /src
ENV CARGO_NET_GIT_FETCH_WITH_CLI=false

# The compile DAG is deliberately closed. In particular, the tracked SBOM and
# all release labels/hashes are absent until after the Rust binary exists.
COPY services/hepta-research-league/docker/workspace.Cargo.toml Cargo.toml
COPY services/hepta-research-league/docker/rust-toolchain.manifest /toolchain/rust-toolchain.manifest
COPY crates/hepta-paper-raid-contracts/Cargo.toml crates/hepta-paper-raid-contracts/Cargo.toml
COPY crates/hepta-paper-raid-contracts/src crates/hepta-paper-raid-contracts/src
COPY services/hepta-research-league/Cargo.toml services/hepta-research-league/Cargo.toml
COPY services/hepta-research-league/src services/hepta-research-league/src
COPY vendor/trnm-finality-types/Cargo.toml vendor/trnm-finality-types/Cargo.toml
COPY vendor/trnm-finality-types/src vendor/trnm-finality-types/src
COPY vendor/trnm-finality-verifier/Cargo.toml vendor/trnm-finality-verifier/Cargo.toml
COPY vendor/trnm-finality-verifier/src vendor/trnm-finality-verifier/src
COPY vendor/trnm-protocol/Cargo.toml vendor/trnm-protocol/Cargo.toml
COPY vendor/trnm-protocol/src vendor/trnm-protocol/src
COPY vendor/trnm-research-protocol/Cargo.toml vendor/trnm-research-protocol/Cargo.toml
COPY vendor/trnm-research-protocol/src vendor/trnm-research-protocol/src
COPY migrations/0031_add_hepta_research_league.sql migrations/0031_add_hepta_research_league.sql
COPY migrations/0032_add_hepta_paper_raid_v2.sql migrations/0032_add_hepta_paper_raid_v2.sql
COPY migrations/0033_add_hepta_paper_collaboration_kernel.sql migrations/0033_add_hepta_paper_collaboration_kernel.sql
COPY migrations/0034_add_hepta_paper_review_appeal.sql migrations/0034_add_hepta_paper_review_appeal.sql
COPY migrations/0035_add_hepta_secure_onboarding.sql migrations/0035_add_hepta_secure_onboarding.sql
COPY migrations/0036_add_hepta_nakama_research_control.sql migrations/0036_add_hepta_nakama_research_control.sql
COPY migrations/0037_add_hepta_paper_chain_finality_v1.sql migrations/0037_add_hepta_paper_chain_finality_v1.sql
COPY migrations/0038_add_hepta_paper_chain_finality_v2.sql migrations/0038_add_hepta_paper_chain_finality_v2.sql
COPY docs/openapi/hepta-research-league-v1.yaml docs/openapi/hepta-research-league-v1.yaml
COPY docs/openapi/hepta-paper-raid-v2.yaml docs/openapi/hepta-paper-raid-v2.yaml

RUN test "$(sed -n '1p' /toolchain/rust-toolchain.manifest)" = \
      "builder=docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb" \
    && test "$(sed -n '2p' /toolchain/rust-toolchain.manifest)" = \
      "rustc=rustc 1.95.0 (59807616e 2026-04-14)" \
    && test "$(sed -n '3p' /toolchain/rust-toolchain.manifest)" = \
      "cargo=cargo 1.95.0 (f2d3ce0bd 2026-03-21)" \
    && test "$(wc -l < /toolchain/rust-toolchain.manifest)" -eq 3 \
    && test "$(rustc --version)" = "rustc 1.95.0 (59807616e 2026-04-14)" \
    && test "$(cargo --version)" = "cargo 1.95.0 (f2d3ce0bd 2026-03-21)"

FROM workspace AS lockfile-verifier
COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock
RUN CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=5 \
    cargo fetch --locked \
    && cargo metadata --locked --offline --format-version 1 \
      > /cargo-lock-metadata.json

FROM scratch AS cargo-lock-export
COPY --from=lockfile-verifier /src/Cargo.lock /Cargo.lock

FROM workspace AS builder
COPY services/hepta-research-league/docker/Cargo.lock Cargo.lock
RUN CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=5 \
    cargo fetch --locked
RUN cargo metadata --locked --offline --format-version 1 > /cargo-metadata.json
RUN env -u SOURCE_DATE_EPOCH \
        -u RUNTIME_BINARY_SHA256 \
        -u VCS_REF \
        -u SOURCE_TREE \
        -u SBOM_SHA256 \
      cargo build --locked --offline --release \
        -p hepta-research-league --bin hepta-research-league

FROM scratch AS runtime-binary-export
COPY --from=builder /src/target/release/hepta-research-league /hepta-research-league

FROM scratch AS sbom-metadata-export
COPY --from=builder /cargo-metadata.json /cargo-metadata.json

FROM builder AS release
ARG SOURCE_DATE_EPOCH
ARG RUNTIME_BINARY_SHA256
COPY deploy/hepta-research-league/hepta-research-league.cdx.json /tracked/hepta-research-league.cdx.json
RUN case "$SOURCE_DATE_EPOCH" in ''|*[!0-9]*) exit 64 ;; esac \
    && case "$RUNTIME_BINARY_SHA256" in ''|*[!0-9a-f]*) exit 65 ;; esac \
    && test "${#RUNTIME_BINARY_SHA256}" -eq 64 \
    && printf '%s  %s\n' "$RUNTIME_BINARY_SHA256" \
      /src/target/release/hepta-research-league \
      | sha256sum --check --strict - \
    && install -D -m 0755 -o 65532 -g 65532 \
      /src/target/release/hepta-research-league \
      /release/usr/local/bin/hepta-research-league \
    && install -D -m 0644 -o 65532 -g 65532 \
      /tracked/hepta-research-league.cdx.json \
      /release/usr/share/doc/hepta-research-league/sbom.cdx.json \
    && find /release -type d -exec touch --no-dereference --date="@$SOURCE_DATE_EPOCH" {} + \
    && find /release -type f -exec touch --no-dereference --date="@$SOURCE_DATE_EPOCH" {} +

FROM gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98

ARG VCS_REF
ARG SOURCE_TREE
ARG SOURCE_DATE_EPOCH
ARG SBOM_SHA256
ARG RUNTIME_BINARY_SHA256
ARG CARGO_LOCK_SHA256
ARG DOCKERFILE_SHA256
ARG RUST_TOOLCHAIN_SHA256
LABEL org.opencontainers.image.title="Hepta Research League" \
      org.opencontainers.image.description="Paper Raid research control plane" \
      org.opencontainers.image.source="https://github.com/TrillionniumFoundation/CEX.git" \
      org.opencontainers.image.revision="$VCS_REF" \
      org.opencontainers.image.licenses="MIT" \
      org.trillionnium.source.tree="$SOURCE_TREE" \
      org.trillionnium.sbom.sha256="$SBOM_SHA256" \
      org.trillionnium.cargo-lock.sha256="$CARGO_LOCK_SHA256" \
      org.trillionnium.dockerfile.sha256="$DOCKERFILE_SHA256" \
      org.trillionnium.rust-toolchain.sha256="$RUST_TOOLCHAIN_SHA256" \
      io.trillionnium.hepta.source-tree="$SOURCE_TREE" \
      io.trillionnium.hepta.source-date-epoch="$SOURCE_DATE_EPOCH" \
      io.trillionnium.hepta.application-sbom.path="/usr/share/doc/hepta-research-league/sbom.cdx.json" \
      io.trillionnium.hepta.application-sbom.sha256="$SBOM_SHA256" \
      io.trillionnium.hepta.runtime-binary.sha256="$RUNTIME_BINARY_SHA256" \
      io.trillionnium.hepta.runtime-base="gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98" \
      io.trillionnium.hepta.builder-base="docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb"

COPY --from=release /release/ /

USER 65532:65532
EXPOSE 7011
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
  CMD ["/usr/local/bin/hepta-research-league", "--probe-ready"]
ENTRYPOINT ["/usr/local/bin/hepta-research-league"]

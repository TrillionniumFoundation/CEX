# syntax=docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89
FROM docker.io/library/rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb AS builder
WORKDIR /src
ENV CARGO_NET_GIT_FETCH_WITH_CLI=false \
    RUSTUP_TOOLCHAIN=1.98.1

# The immutable bootstrap image contains rustup but predates the patched point
# release. Install and verify the exact compiler before dependency resolution.
RUN rustup set profile minimal \
    && rustup toolchain install 1.98.1 --component rustfmt --component clippy \
    && rustup default 1.98.1 \
    && rustup show active-toolchain | tee /rustup-active-toolchain.txt \
    && rustc --version --verbose | tee /rustc-version.txt \
    && cargo --version --verbose | tee /cargo-version.txt \
    && test "$(rustc --version | awk '{print $2}')" = "1.98.1" \
    && grep -Eq '^1\.98\.1-' /rustup-active-toolchain.txt \
    && grep -Fx 'commit-hash: 48a229ceaefd4985c50990b14116b6d856af0985' /rustc-version.txt

COPY services/paper-raid-bff/docker/workspace.Cargo.toml Cargo.toml
COPY services/paper-raid-bff/docker/Cargo.lock Cargo.lock
COPY crates/hepta-paper-raid-contracts crates/hepta-paper-raid-contracts
COPY services/paper-raid-bff/Cargo.toml services/paper-raid-bff/Cargo.toml
COPY services/paper-raid-bff/src services/paper-raid-bff/src
COPY services/paper-raid-bff/migrations services/paper-raid-bff/migrations
RUN CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_NET_RETRY=5 \
    cargo fetch --locked
RUN env -u SOURCE_DATE_EPOCH cargo build --locked --offline --release -p paper-raid-bff

FROM scratch AS runtime-binary-export
COPY --from=builder /src/target/release/paper-raid-bff /paper-raid-bff

FROM builder AS release
ARG PAPER_RAID_BFF_RELEASE_EPOCH
ARG PAPER_RAID_BFF_RUNTIME_BINARY_SHA256
COPY services/paper-raid-bff/docker/sbom.cdx.json services/paper-raid-bff/docker/sbom.cdx.json
RUN case "$PAPER_RAID_BFF_RELEASE_EPOCH" in \
      ''|*[!0-9]*) exit 64 ;; \
    esac \
    && case "$PAPER_RAID_BFF_RUNTIME_BINARY_SHA256" in \
      *[!0-9a-f]*|'') exit 64 ;; \
    esac \
    && test "${#PAPER_RAID_BFF_RUNTIME_BINARY_SHA256}" -eq 64 \
    && printf '%s  %s\n' \
      "$PAPER_RAID_BFF_RUNTIME_BINARY_SHA256" \
      /src/target/release/paper-raid-bff \
      | sha256sum --check --status \
    && install -d -m 0755 /release/usr/share/doc/paper-raid-bff \
    && install -m 0755 -o 65532 -g 65532 \
      /src/target/release/paper-raid-bff /release/paper-raid-bff \
    && install -m 0644 -o 65532 -g 65532 \
      /src/services/paper-raid-bff/docker/sbom.cdx.json \
      /release/usr/share/doc/paper-raid-bff/sbom.cdx.json \
    && install -m 0644 -o 65532 -g 65532 /rustc-version.txt \
      /release/usr/share/doc/paper-raid-bff/rustc-version.txt \
    && install -m 0644 -o 65532 -g 65532 /cargo-version.txt \
      /release/usr/share/doc/paper-raid-bff/cargo-version.txt \
    && find /release -type d -exec touch --date="@$PAPER_RAID_BFF_RELEASE_EPOCH" {} + \
    && find /release -type f -exec touch --date="@$PAPER_RAID_BFF_RELEASE_EPOCH" {} +

FROM gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98
ARG PAPER_RAID_BFF_REVISION
ARG PAPER_RAID_BFF_SOURCE_TREE
ARG PAPER_RAID_BFF_CREATED
ARG PAPER_RAID_BFF_SBOM_SHA256
ARG PAPER_RAID_BFF_CARGO_LOCK_SHA256
ARG PAPER_RAID_BFF_RUNTIME_BINARY_SHA256
LABEL org.opencontainers.image.title="Hepta Paper Raid BFF" \
      org.opencontainers.image.description="Same-origin Consumer Edge for the Hepta Paper Raid alpha" \
      org.opencontainers.image.source="https://github.com/TrillionniumFoundation/CEX" \
      org.opencontainers.image.version="0.1.0" \
      org.opencontainers.image.revision="${PAPER_RAID_BFF_REVISION}" \
      org.trillionnium.source.tree="${PAPER_RAID_BFF_SOURCE_TREE}" \
      org.opencontainers.image.created="${PAPER_RAID_BFF_CREATED}" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.vendor="Trillionnium Foundation" \
      org.opencontainers.image.sbom="/usr/share/doc/paper-raid-bff/sbom.cdx.json" \
      org.trillionnium.cargo-lock.sha256="${PAPER_RAID_BFF_CARGO_LOCK_SHA256}" \
      org.trillionnium.sbom.sha256="${PAPER_RAID_BFF_SBOM_SHA256}" \
      org.trillionnium.runtime-binary.sha256="${PAPER_RAID_BFF_RUNTIME_BINARY_SHA256}" \
      org.trillionnium.rust-toolchain="1.98.1" \
      org.trillionnium.rust-release-commit="48a229ceaefd4985c50990b14116b6d856af0985"
# Copy the complete normalized release tree. Copying lone files lets BuildKit
# stamp their destination parent directories with wall-clock time, producing a
# different layer digest even when the Rust binary and SBOM bytes are stable.
COPY --from=release /release/ /
USER 65532:65532
EXPOSE 7020
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 CMD ["/paper-raid-bff", "--probe-ready"]
ENTRYPOINT ["/paper-raid-bff"]

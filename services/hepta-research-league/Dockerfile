# syntax=docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89
ARG SOURCE_DATE_EPOCH=0
FROM rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb AS builder

WORKDIR /src
ARG SOURCE_DATE_EPOCH
ARG RUNTIME_BINARY_SHA256
ENV CARGO_NET_GIT_FETCH_WITH_CLI=false
COPY . .
RUN release_source_date_epoch="$SOURCE_DATE_EPOCH" \
    && case "$release_source_date_epoch" in ''|*[!0-9]*) exit 64 ;; esac \
    && test "${#RUNTIME_BINARY_SHA256}" -eq 64 \
    && case "$RUNTIME_BINARY_SHA256" in *[!0-9a-f]*) exit 65 ;; esac \
    && env -u SOURCE_DATE_EPOCH cargo build --locked --release -p hepta-research-league --bin hepta-research-league \
    && printf '%s  %s\n' "$RUNTIME_BINARY_SHA256" target/release/hepta-research-league | sha256sum --check --strict - \
    && install -D -m 0755 target/release/hepta-research-league /release/usr/local/bin/hepta-research-league \
    && install -D -m 0644 deploy/hepta-research-league/hepta-research-league.cdx.json /release/usr/share/doc/hepta-research-league/sbom.cdx.json \
    && find /release -exec touch --no-dereference --date="@${release_source_date_epoch}" {} +

FROM gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98

ARG VCS_REF
ARG SOURCE_TREE
ARG SOURCE_DATE_EPOCH
ARG SBOM_SHA256
ARG RUNTIME_BINARY_SHA256
LABEL org.opencontainers.image.title="Hepta Research League" \
      org.opencontainers.image.description="Paper Raid research control plane" \
      org.opencontainers.image.source="https://github.com/TrillionniumFoundation/CEX.git" \
      org.opencontainers.image.revision="$VCS_REF" \
      org.opencontainers.image.licenses="MIT" \
      org.trillionnium.source.tree="$SOURCE_TREE" \
      org.trillionnium.sbom.sha256="$SBOM_SHA256" \
      io.trillionnium.hepta.source-tree="$SOURCE_TREE" \
      io.trillionnium.hepta.source-date-epoch="$SOURCE_DATE_EPOCH" \
      io.trillionnium.hepta.application-sbom.path="/usr/share/doc/hepta-research-league/sbom.cdx.json" \
      io.trillionnium.hepta.application-sbom.sha256="$SBOM_SHA256" \
      io.trillionnium.hepta.runtime-binary.sha256="$RUNTIME_BINARY_SHA256" \
      io.trillionnium.hepta.runtime-base="gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98" \
      io.trillionnium.hepta.builder-base="rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb"

COPY --from=builder /release/ /

USER 65532:65532
EXPOSE 7011
HEALTHCHECK --interval=10s --timeout=3s --retries=6 \
  CMD ["/usr/local/bin/hepta-research-league", "--probe-ready"]
ENTRYPOINT ["/usr/local/bin/hepta-research-league"]

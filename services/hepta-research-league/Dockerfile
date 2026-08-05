FROM rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb AS builder

WORKDIR /src
ARG SOURCE_DATE_EPOCH
ENV CARGO_NET_GIT_FETCH_WITH_CLI=false
ENV SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH
COPY . .
RUN cargo build --locked --release -p hepta-research-league --bin hepta-research-league \
    && install -D -m 0755 target/release/hepta-research-league /release/hepta-research-league \
    && install -D -m 0644 deploy/hepta-research-league/hepta-research-league.cdx.json /release/hepta-research-league.cdx.json

FROM gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98

ARG VCS_REF
ARG SOURCE_TREE
ARG SOURCE_DATE_EPOCH
ARG SBOM_SHA256
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
      io.trillionnium.hepta.runtime-base="gcr.io/distroless/cc-debian12@sha256:471dbca9cad607b9a32c10e9c31fb09ffaeb2d460e0afbff86c27abbc80b1b98" \
      io.trillionnium.hepta.builder-base="rust@sha256:4c2fd73ef19c5ef9d54bee03b06b2839a392604fbfcd578ed948b71b37c1d7fb"

COPY --from=builder /release/hepta-research-league /usr/local/bin/hepta-research-league
COPY --from=builder /release/hepta-research-league.cdx.json /usr/share/doc/hepta-research-league/sbom.cdx.json

USER 65532:65532
EXPOSE 7011
HEALTHCHECK --interval=10s --timeout=3s --retries=6 \
  CMD ["/usr/local/bin/hepta-research-league", "--probe-ready"]
ENTRYPOINT ["/usr/local/bin/hepta-research-league"]

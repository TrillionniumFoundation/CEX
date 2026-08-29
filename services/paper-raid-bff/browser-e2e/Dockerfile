FROM mcr.microsoft.com/playwright@sha256:ad57c625d284e8d287abcd40d18434582b2e354de71c8428cb080112b0c45960
ENV NODE_ENV=production \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright \
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
WORKDIR /runner
COPY services/paper-raid-bff/browser-e2e/package.json ./package.json
COPY services/paper-raid-bff/browser-e2e/package-lock.json ./package-lock.json
RUN npm ci --ignore-scripts --no-audit --no-fund --omit=dev \
    && npm cache clean --force
COPY services/paper-raid-bff/browser-e2e/e2e.mjs ./e2e.mjs
COPY services/paper-raid-bff/browser-e2e/mobile-a11y.mjs ./mobile-a11y.mjs
COPY services/paper-raid-bff/browser-e2e/mock-hepta.mjs ./mock-hepta.mjs
LABEL org.opencontainers.image.title="Hepta Paper Raid Browser E2E" \
      org.opencontainers.image.description="Disposable pinned-Chromium gates for Paper Raid" \
      org.opencontainers.image.source="https://github.com/TrillionniumFoundation/CEX" \
      org.opencontainers.image.version="1.49.1" \
      org.opencontainers.image.licenses="MIT" \
      org.trillionnium.playwright.base.digest="sha256:ad57c625d284e8d287abcd40d18434582b2e354de71c8428cb080112b0c45960"
ENTRYPOINT ["node", "/runner/e2e.mjs"]

# Build stage: compile the React app.
FROM node:26-alpine AS build
WORKDIR /app
# .npmrc carries `legacy-peer-deps=true` (glide-data-grid's React-18 peer ceiling
# vs the project's React 19); npm ci must see it or the install fails.
COPY package.json package-lock.json .npmrc ./
# npm's default retry budget (2 retries, short timeouts) doesn't ride out a flaky
# connection from inside the build container (seen as ECONNRESET / "network
# aborted" on macOS Docker backends). Raise npm's own retries and wrap the
# install in a shell-level retry loop as a backstop. `npm config set` writes the
# user config (~/.npmrc), so the project .npmrc copied above still applies.
RUN npm config set fetch-retries 5 \
    && npm config set fetch-retry-mintimeout 20000 \
    && npm config set fetch-retry-maxtimeout 120000 \
    && for attempt in 1 2 3; do \
           npm ci --ignore-scripts && break; \
           echo "npm ci failed (attempt $attempt/3)"; \
           [ "$attempt" = 3 ] && exit 1; \
           sleep 5; \
       done
COPY tsconfig.json vite.config.ts postcss.config.cjs index.html ./
COPY src ./src
# scripts/compile-docs.js regenerates src/docsCatalog.ts from src/docs/*.md as
# the first step of `npm run build`, so it must be in the image.
COPY scripts ./scripts
# Build stamp: the git commit the bundle was built from, surfaced in the About
# dialog (see CLAUDE.md "Build stamping"). Local builds pass VITE_COMMIT_HASH via
# frees.sh; Railway auto-provides RAILWAY_GIT_COMMIT_SHA as a build arg.
ARG VITE_COMMIT_HASH=
ARG RAILWAY_GIT_COMMIT_SHA=
ARG VITE_ASYNC_API=
ARG VITE_API_BASE=
RUN VITE_COMMIT_HASH="${VITE_COMMIT_HASH:-${RAILWAY_GIT_COMMIT_SHA:-dev}}" \
    VITE_ASYNC_API="${VITE_ASYNC_API}" \
    VITE_API_BASE="${VITE_API_BASE}" \
    npm run build

# Runtime stage: unprivileged nginx (runs as uid 101, listens on 8080)
# serves the static bundle and proxies /api to the backend. The nginx config
# is a template: the 20-envsubst-on-templates.sh entrypoint script expands
# ${BACKEND_HOST} at container start, so the same image works in docker-compose
# (BACKEND_HOST=api-node) and Railway (BACKEND_HOST=frees-api.railway.internal).
FROM nginxinc/nginx-unprivileged:alpine
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
# Enable the image's built-in 15-local-resolvers.envsh entrypoint, which exports
# ${NGINX_LOCAL_RESOLVERS} from /etc/resolv.conf before envsubst runs. The /api/
# block in nginx.conf.template uses it as the `resolver` so nginx re-resolves the
# backend upstream per request instead of caching a stale IP across redeploys
# (the Railway private-network 504 trap).
ENV NGINX_ENTRYPOINT_LOCAL_RESOLVERS=1
# Security headers, included by every location block in the nginx config.
COPY security-headers.conf /etc/nginx/security-headers.conf
COPY --from=build /app/dist /usr/share/nginx/html
# Runtime build-info: an entrypoint script stamps the live commit (see the
# script and CLAUDE.md "Build stamping"). build-info.js must be writable by the
# unprivileged runtime user; it starts empty so local builds fall back cleanly.
COPY docker-entrypoint.d/40-build-info.sh /docker-entrypoint.d/40-build-info.sh
# real_ip trust list, generated at start from TRUSTED_PROXY_CIDRS (empty when
# unset — see the script). Must exist even when empty because the nginx config
# includes it unconditionally, and must be writable by the runtime user.
COPY docker-entrypoint.d/10-real-ip.sh /docker-entrypoint.d/10-real-ip.sh
USER root
RUN chmod +x /docker-entrypoint.d/40-build-info.sh /docker-entrypoint.d/10-real-ip.sh \
    && : > /usr/share/nginx/html/build-info.js \
    && chown 101:101 /usr/share/nginx/html/build-info.js \
    && chmod 644 /usr/share/nginx/html/build-info.js \
    && : > /etc/nginx/real-ip.conf \
    && chown 101:101 /etc/nginx/real-ip.conf \
    && chmod 644 /etc/nginx/real-ip.conf
USER 101
EXPOSE 8080

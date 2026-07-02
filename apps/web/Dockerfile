# web-start on Bun — the Fargate PERFORMANCE path (infra/lib/web-start-fargate-stack.ts).
# NOT used by the free-tier Lambda deploy. Build context = apps/web-start; run
# `pnpm --filter web-start build:aws` first (produces dist/ + .aws-lambda/vegify-seed.db).
FROM oven/bun:1 AS runtime
WORKDIR /app

# Built app + the Bun entrypoint (serve-bun.mjs runs the WinterCG build and serves dist/client).
COPY dist ./dist
COPY serve-bun.mjs ./
COPY aws/sitemap.mjs ./aws/sitemap.mjs

# @libsql/client is kept external from the SSR bundle (vite ssr.external); Bun installs the linux
# binding. Everything else is already bundled into dist/server.
RUN echo '{"type":"module","dependencies":{"@libsql/client":"^0.17.3"}}' > package.json && bun install

# Seed copied onto the EBS volume on first boot (see docker-entrypoint.sh).
COPY .aws-lambda/vegify-seed.db ./vegify-seed.db
COPY docker-entrypoint.sh ./
RUN chmod +x docker-entrypoint.sh

ENV PORT=3001 DATABASE_URL=file:/data/vegify.db NODE_ENV=production
EXPOSE 3001
ENTRYPOINT ["./docker-entrypoint.sh"]

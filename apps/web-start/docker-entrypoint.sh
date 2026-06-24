#!/bin/sh
# Seed the EBS-backed libSQL DB on first boot, then run the long-running Bun server.
# (Fargate performance path — see infra/lib/web-start-fargate-stack.ts.)
set -e
if [ ! -f /data/vegify.db ]; then
  mkdir -p /data
  cp /app/vegify-seed.db /data/vegify.db
fi
exec bun serve-bun.mjs

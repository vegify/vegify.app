# Vegify dev tasks — the SAME commands CI runs (.github/workflows/ci.yml + release.yml), so
# `just check` locally is exactly what the pipeline enforces. No more "passed locally, red in CI".
# Install just with `brew install just`. Run `just check` before every push.

# Show the recipes.
default:
    @just --list

# Everything CI enforces. Run before every push.
check: tokens typecheck web-test build rust

# JS deps, frozen exactly as CI installs them.
install:
    pnpm install --frozen-lockfile

# Design tokens -> packages/tokens/dist/theme.css (gitignored; a prereq for the JS typecheck/build).
tokens:
    pnpm --filter @vegify/tokens build

# Typecheck EVERY package (turbo). The all-packages run that an ad-hoc `--filter X typecheck` misses.
typecheck:
    pnpm typecheck

# Web unit tests.
web-test:
    pnpm --filter web test

# Build both app frontends (web + desktop).
build:
    pnpm --filter web build && pnpm --filter desktop build

# All Rust tests: server + shared core + the desktop crate.
rust: rust-core rust-desktop

# Server + shared core (CI's `ci` rust job — no dev DB needed).
rust-core:
    cargo test -p vegify-server -p vegify-core

# The desktop crate (CI covers it in the release job). Needs the dev DB (`just db`) for the
# schema-parity test.
rust-desktop:
    cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml

# Regenerate the desktop TS bindings after changing Rust procedures/types.
bindings:
    pnpm --filter desktop gen:bindings

# Create/refresh the dev DB from the Drizzle schema + seed it.
db:
    pnpm db:push && pnpm db:seed

# ── Deploy decisions (SSM Parameter Store — the account itself is the config store) ──────────────
# One-time account setup: record the deploy decisions under /vegify/deploy/. Everything else derives
# (zone lookup, cert, cross-stack wiring, generated origin-verify secret). Needs an AWS login
# (`aws sts get-caller-identity` works) in the deploy region. Args are positional:
#   just init vegify.app,www.vegify.app        # domains (first is primary); signups default closed
init domains signups="0":
    aws ssm put-parameter --name /vegify/deploy/domain-names --type String --overwrite --value "{{domains}}"
    aws ssm put-parameter --name /vegify/deploy/signups-open --type String --overwrite --value "{{signups}}"
    @echo "Recorded. Optional extras: just config-set email-from 'You <hello@your.domain>' | email-domain | mail-from-domain | cert-arn | apple-secret-id"
    @echo "Next: cdk bootstrap (once), then cdk deploy VegifyVpc VegifyServer && cdk deploy VegifyWebStart VegifyClientLogs"

# Show the recorded deploy decisions.
config:
    aws ssm get-parameters-by-path --path /vegify/deploy/ --query 'Parameters[].{key:Name,value:Value}' --output table

# Set one deploy decision, e.g. `just config-set signups-open 1` (takes effect on the next release).
config-set key value:
    aws ssm put-parameter --name /vegify/deploy/{{key}} --type String --overwrite --value "{{value}}"

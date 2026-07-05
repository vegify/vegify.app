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

# Rebuild the USDA catalog artifact from the raw FDC downloads (.data/import/usda/) → .data/build/.
usda-data:
    cargo run -p usda-importer

# Ship the artifact to the server's Data bucket (name from SSM — written by the VegifyServer stack).
# The server ingests it at the next boot (marker-gated), i.e. the next server deploy.
usda-upload:
    aws s3 cp --region {{deploy_region}} .data/build/usda-plants.json.gz s3://$(aws ssm get-parameter --region {{deploy_region}} --name /vegify/deploy/data-bucket --query Parameter.Value --output text)/catalog/usda-plants.json.gz

# Create/refresh the dev DB from the Drizzle schema + seed it.
db:
    pnpm db:push && pnpm db:seed

# ── Deploy decisions (SSM Parameter Store — the account itself is the config store) ──────────────
# The decisions live in the DEPLOY region — pinned here so a shell whose default AWS region differs
# can't write them somewhere the deploys never look (exactly the bug that shipped placeholder email
# config in v0.18.0). Override with AWS_REGION if you deploy elsewhere.
deploy_region := env_var_or_default("AWS_REGION", "us-east-1")

# One-time account setup: record the deploy decisions under /vegify/deploy/. Everything else derives
# (zone lookup, cert, cross-stack wiring, generated origin-verify secret). Needs an AWS login
# (`aws sts get-caller-identity` works). Args are positional:
#   just init vegify.app,www.vegify.app        # domains (first is primary); signups default closed
init domains signups="0":
    aws ssm put-parameter --region {{deploy_region}} --name /vegify/deploy/domain-names --type String --overwrite --value "{{domains}}"
    aws ssm put-parameter --region {{deploy_region}} --name /vegify/deploy/signups-open --type String --overwrite --value "{{signups}}"
    @echo "Recorded in {{deploy_region}}. Optional extras: just config-set email-from 'You <hello@your.domain>' | email-domain | mail-from-domain | cert-arn | apple-secret-id"
    @echo "Next: cdk bootstrap (once), then cdk deploy VegifyVpc VegifyServer && cdk deploy VegifyWebStart VegifyClientLogs"

# Show the recorded deploy decisions.
config:
    aws ssm get-parameters-by-path --region {{deploy_region}} --path /vegify/deploy/ --query 'Parameters[].{key:Name,value:Value}' --output table

# Set one deploy decision, e.g. `just config-set signups-open 1` (takes effect on the next release).
config-set key value:
    aws ssm put-parameter --region {{deploy_region}} --name /vegify/deploy/{{key}} --type String --overwrite --value "{{value}}"

# ── Releases (stormdeck model: merging to main ships + auto-cuts a PATCH release) ─────────────────
# Bigger bumps are the human lever: dispatch the deploy workflow with a minor/major bump from main
# HEAD. Patch releases need no command — every shipping merge cuts one ([skip release] in the PR
# title suppresses it).
release level="minor":
    gh workflow run deploy.yml -f bump={{level}}
    @echo "dispatched — follow with: gh run list --workflow deploy.yml (verify conclusion via gh run view <id>, not gh run watch)"

# Re-run the deploy for an existing tag (the recovery path), e.g. `just redeploy v0.18.2`.
redeploy tag:
    gh workflow run deploy.yml -f tag={{tag}}

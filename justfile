# Vegify dev tasks — the SAME commands CI runs (.github/workflows/ci.yml + release.yml), so
# `just check` locally is exactly what the pipeline enforces. No more "passed locally, red in CI".
# Install just with `brew install just`. Run `just check` before every push.

# Warnings are errors in every cargo run these recipes make — locally and in CI (ci.yml runs
# these same recipes). Dev iteration outside `just` (bare cargo, rust-analyzer) stays lenient.
export RUSTFLAGS := "-Dwarnings"

# Show the recipes.
default:
    @just --list

# Everything CI enforces. Run before every push.
check: lint tokens typecheck web-test build rust

# JS deps, frozen exactly as CI installs them.
install:
    pnpm install --frozen-lockfile

# One lint/format layer for the whole workspace (biome; config at the root).
lint:
    pnpm exec biome check .

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

# Formatting is part of the rust gate (CI runs rust-core, so it rides along).
rust-fmt:
    cargo fmt --all --check

# Server + shared core (CI's `ci` rust job — no dev DB needed).
rust-core: rust-fmt
    cargo test -p vegify-server -p vegify-core
    # Lint gate for every non-desktop member (the desktop crate needs the webview
    # stack, which CI's ubuntu runner doesn't carry — rust-desktop covers it).
    cargo clippy -p vegify-server -p vegify-core -p vegify-config -p vegify-api-types -p vegify-typegen -p vegify-admin -p usda-importer -p vegify-client-rs --all-targets --locked -- -D warnings
    # The dev-tool crates ship in no build, so nothing else compiles them — keep them honest here.
    cargo check -p vegify-admin -p usda-importer
    # The SDK compiles standalone WITHOUT its specta feature (a plain consumer's tree) — the desktop
    # only ever builds it WITH the feature, so this is the no-specta path's only gate.
    cargo check -p vegify-client-rs

# The desktop crate (CI covers it in the release job). Needs the dev DB (`just db`) for the
# schema-parity test.
rust-desktop:
    cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
    cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --locked -- -D warnings
    # Release-mode check: `cfg(debug_assertions)` branches compile OUT in release, so a warning can
    # hide from every debug build and first appear in the shipped desktop build (it happened: an
    # unused PathBuf import only the release build could see). Check, not build — types + lints only.
    cargo check --release --manifest-path apps/desktop/src-tauri/Cargo.toml

# Regenerate ALL generated type artifacts from the Rust source of truth: the desktop's IPC bindings
# (in-crate, ttipc's test pattern — the app is a leaf, never a dependency) + @vegify/api-types
# (crates/vegify-typegen, from the server's wire-contract crate).
bindings:
    VEGIFY_REGEN_BINDINGS=1 cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib bindings_ts_is_current -- --nocapture
    cargo run -p vegify-typegen

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
    @echo "Recorded in {{deploy_region}}. Optional extras: just config-set email-from 'You <hello@your.domain>' | email-domain | mail-from-domain | cert-arn | apple-secret-id | mas-secret-id"
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

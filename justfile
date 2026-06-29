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

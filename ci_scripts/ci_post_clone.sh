#!/bin/sh
# Xcode Cloud runs this automatically right after cloning, before it resolves packages and archives.
# Its build image is a clean macOS + Xcode — it has NO Rust and NO pnpm. But this is a Tauri app: the
# iOS target's build phase runs `pnpm tauri ios xcode-script`, which builds the web frontend and
# compiles the Rust library for aarch64-apple-ios. So both toolchains, the installed JS deps, and the
# design tokens must exist before `xcodebuild archive` runs. Install them here.
#
# (The build phase itself prepends ~/.cargo/bin and /opt/homebrew/bin to PATH — see the iOS target's
# pre-build script — because a ci_post_clone PATH change does NOT persist into the xcodebuild step.)
#
# Signing is NOT handled here: Xcode Cloud injects the team + mints the distribution cert/profile
# natively (CODE_SIGN_STYLE=Automatic), which is the whole reason this path sidesteps the ASC-key
# cloud-signing wall the GitHub Actions workflow hit.
set -eu

echo "→ Rust toolchain + iOS target"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
rustup target add aarch64-apple-ios

echo "→ Node + pnpm (Homebrew is preinstalled on the Xcode Cloud image)"
brew install node
npm install -g pnpm

echo "→ JS deps + design tokens + frontend build (generate_context! and the build phase need dist present)"
cd "$CI_PRIMARY_REPOSITORY_PATH"
pnpm install --frozen-lockfile
pnpm --filter @vegify/tokens build
pnpm --filter desktop build

echo "✓ ci_post_clone complete"

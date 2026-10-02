#!/usr/bin/env bash
# Table tests for the release scripts in this directory — `just scripts-test`, part of `just check`.
# Plain bash + git, no framework; runs the same on bash 3.2 (macOS) and bash 5 (the runners).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fails=0
ok() { echo "ok   - $1"; }
fail() { echo "FAIL - $1" >&2; fails=$((fails + 1)); }

# ── bump-level.sh: subject → patch|minor|major ───────────────────────────────────────────────────
level_is() { # <expected> <subject>
  local got
  got=$("$here/bump-level.sh" "$2" 2>/dev/null) || got="exit $?"
  if [ "$got" = "$1" ]; then ok "$1 ← '$2'"; else fail "expected $1, got '$got' ← '$2'"; fi
}
# The three rules, on real merge subjects from this repo's history.
level_is minor 'feat: an A-Z index of the ingredient catalog (#246)'
level_is minor 'feat(web): per-page titles, descriptions, canonical URLs, and structured data (#239)'
level_is patch 'fix: keep unlisted recipes out of search indexes (#245)'
level_is patch 'fix(server): rate-limit SSR traffic per visitor, not per Lambda egress IP (#244)'
level_is patch 'perf(web): skip the backend session lookup for logged-out visitors (#240)'
level_is patch 'ci(deploy): dump the instance console when the health gate fails [skip release] (#231)'
level_is patch 'build(deps): bump the cargo-minor-patch group with 3 updates (#236)'
level_is patch 'chore(deps): bump the actions-all group (#200)'
level_is patch 'release: redeploys resolve the existing release id so artifacts attach [skip release] (#163)'
# A `!` after the type or scope is the ONLY major signal.
level_is major 'feat!: drop the v1 sync endpoints'
level_is major 'feat(api)!: rename the diary wire shapes'
level_is major 'fix(server)!: reject unsigned viewer-ip headers'
level_is major 'refactor!: split vegify-core'
# A dependency's own major is not ours; a `!` outside the prefix is punctuation; a body footer never
# reaches this script (it only ever sees the subject).
level_is patch 'build(deps-dev): bump vitest from 4.1.11 to 5.0.0 (#223)'
level_is patch "fix: don't panic! on an empty catalog"
level_is patch 'BREAKING CHANGE: this is a footer token, not a prefix'
# Types are case-insensitive; a missing space after the colon is tolerated; `feature` is not `feat`.
level_is minor 'Feat: capitalised type'
level_is minor 'feat:no space after the colon'
level_is patch 'feature: not the conventional type'
level_is patch 'revert: feat: an A-Z index of the ingredient catalog (#246)'
# No conventional prefix at all → patch (never over-bump an unlabelled merge).
level_is patch 'Gate CI caches to push events only (#205)'
level_is patch 'specta pin: advance to ef21f05 (openapi.json now targets 3.1) (#159)'
level_is patch ''
# Two arguments is a usage error; none reads HEAD's subject.
got=$("$here/bump-level.sh" a b 2>/dev/null) || got="exit $?"
if [ "$got" = "exit 1" ]; then ok "usage error on two arguments"; else fail "expected a usage error on two arguments, got '$got'"; fi

# ── next-version.sh: level + latest v* tag → next tag ────────────────────────────────────────────
next_is() { # <expected> <level> [tag...] — run in a throwaway repo holding exactly those tags
  local expected=$1 level=$2 repo got t
  shift 2
  repo=$(mktemp -d)
  git -C "$repo" init -q
  git -C "$repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false -c core.hooksPath=/dev/null \
    commit -q --allow-empty -m init
  for t in "$@"; do git -C "$repo" -c tag.gpgsign=false tag "$t"; done
  got=$(cd "$repo" && "$here/next-version.sh" "$level" 2>/dev/null) || got="exit $?"
  rm -rf "$repo"
  if [ "$got" = "$expected" ]; then ok "$expected ← $level on {$*}"; else fail "expected $expected, got '$got' ← $level on {$*}"; fi
}
next_is v0.1.0 patch
next_is v1.0.69 patch v1.0.68
next_is v1.1.0 minor v1.0.68
next_is v2.0.0 major v1.0.68
next_is v1.0.11 patch v1.0.9 v1.0.10                 # version order, not string order
next_is v1.1.0 minor v1.0.10 v0.9.0 v1.0.9           # the highest tag wins, whatever the creation order
next_is 'exit 1' bogus v1.0.68

# bump-level.sh with no argument reads HEAD's subject.
repo=$(mktemp -d)
git -C "$repo" init -q
git -C "$repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false -c core.hooksPath=/dev/null \
  commit -q --allow-empty -m 'feat: seeded from HEAD'
got=$(cd "$repo" && "$here/bump-level.sh" 2>/dev/null) || got="exit $?"
rm -rf "$repo"
if [ "$got" = minor ]; then ok "minor ← HEAD's subject when no argument is given"; else fail "expected minor from HEAD's subject, got '$got'"; fi

if [ "$fails" -gt 0 ]; then echo "$fails failing" >&2; exit 1; fi
echo "all scripts tests passed"

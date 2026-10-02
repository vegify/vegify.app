#!/usr/bin/env bash
# Which semver bump a shipping merge calls for, read from its commit subject (= the squash-merged
# PR title) by the Conventional Commits prefix `type(scope)!: summary`:
#   a `!` after the type/scope  → major   (a breaking change: `feat!:`, `fix(api)!:` …)
#   feat                        → minor
#   anything else               → patch   (fix, perf, build, ci, chore, docs, refactor, revert …;
#                                          a subject with NO conventional prefix is a patch too, so
#                                          an unlabelled merge can never over-bump)
# Only the subject counts. A `BREAKING CHANGE:` footer is deliberately NOT honoured: the squash
# body carries the PR body / commit messages, and dependabot PRs quote upstream changelogs that
# contain exactly that token — it would turn a routine deps bump into a major.
# Used by the deploy workflow's release job (`.scripts/next-version.sh` turns the level into the
# next tag). Usage: bump-level.sh ["<subject>"] — defaults to HEAD's subject; prints the level,
# explains itself on stderr.
set -euo pipefail

if [ $# -gt 1 ]; then
  echo 'usage: bump-level.sh ["<commit subject>"]' >&2
  exit 1
fi
subject="${1-$(git log -1 --pretty=%s)}"

# The pattern lives in a variable: bash 3.2 (macOS) misparses the parens when it is inline.
prefix='^([A-Za-z]+)(\([^)]*\))?(!)?:[[:space:]]*'
if [[ "$subject" =~ $prefix ]]; then
  type=$(printf '%s' "${BASH_REMATCH[1]}" | tr '[:upper:]' '[:lower:]')
  if [ -n "${BASH_REMATCH[3]-}" ]; then
    echo "$type! → major (breaking change)" >&2
    echo major
  elif [ "$type" = feat ]; then
    echo "feat → minor" >&2
    echo minor
  else
    echo "$type → patch" >&2
    echo patch
  fi
else
  echo "no conventional-commit prefix in '$subject' → patch" >&2
  echo patch
fi

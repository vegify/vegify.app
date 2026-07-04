#!/usr/bin/env bash
# Single source of version-bump logic (stormdeck pattern): read the latest v* tag, print the next
# vX.Y.Z. Used by the deploy workflow's release job (patch per shipping merge; minor/major via
# `just release`). The version is git-tag-derived — no committed version field is authoritative.
set -euo pipefail

level="${1:-patch}"
latest=$(git tag --list 'v*' --sort=-v:refname | head -n1)
if [ -z "$latest" ]; then
  echo "v0.1.0"
  exit 0
fi
IFS=. read -r maj min pat <<<"${latest#v}"
case "$level" in
  major) maj=$((maj + 1)) min=0 pat=0 ;;
  minor) min=$((min + 1)) pat=0 ;;
  patch) pat=$((pat + 1)) ;;
  *)
    echo "usage: next-version.sh [patch|minor|major]" >&2
    exit 1
    ;;
esac
echo "v${maj}.${min}.${pat}"

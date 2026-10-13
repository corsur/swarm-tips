#!/usr/bin/env bash
# Print the commit a deploy pipeline should diff against: the head of the last
# fully SUCCESSFUL push run of this workflow on main. Diffing against the
# previous push (paths-filter's default) loses a program change whose pipeline
# failed: the follow-up fix often touches only tests, so `program` reads false
# and the change never deploys (shillbot 711ebbc, 2026-10-13). A successful run
# implies everything up to its head was deployed, so this base is always safe.
#
# Usage: deploy-base-sha.sh <workflow name|file> <fallback sha> [event=push]
# Needs GH_TOKEN with actions:read. Falls back to <fallback> when there is no
# prior success or its head is not an ancestor of HEAD (force push).
set -euo pipefail
WORKFLOW="${1:?workflow name required}"
FALLBACK="${2:-}"
EVENT="${3:-push}"
case "$FALLBACK" in 0000000000000000000000000000000000000000) FALLBACK="" ;; esac

SHA="$(gh run list --workflow "$WORKFLOW" --branch main --event "$EVENT" \
  --status success --limit 1 --json headSha --jq '.[0].headSha // empty' || true)"

if [ -n "$SHA" ] && git merge-base --is-ancestor "$SHA" HEAD 2>/dev/null; then
  echo "diff base: last successful $EVENT run $SHA" >&2
  echo "$SHA"
else
  echo "diff base: fallback ${FALLBACK:-<default>}" >&2
  echo "$FALLBACK"
fi

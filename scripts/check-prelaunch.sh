#!/usr/bin/env bash
set -euo pipefail

# check-prelaunch.sh
#
# Advisory pre-launch checklist scanner. Greps a branch's diff for
# evidence of each of the six items in `docs/PRE_LAUNCH_CHECKLIST.md`.
# Output is informational — items the script can't see (e.g., devnet
# rehearsal artifacts) are reported as "not detected; mark manually."
#
# This is NOT a gate. It runs locally as a sanity check before opening
# a PR; the actual gate is the PR template block being filled in with
# evidence the reviewer can audit.
#
# Exit codes:
#   0  — script ran successfully (regardless of how many items were detected)
#   1  — usage error / git not available

usage() {
  cat <<EOF
Usage: $0 [<base-ref>]

Scans the current branch's diff against <base-ref> (default: main)
for evidence of each pre-launch checklist item. Prints a per-item
summary; does not fail on missing evidence.

Examples:
  $0                     # scan vs. origin/main
  $0 main                # scan vs. local main
  $0 origin/develop      # scan vs. a different base
EOF
  exit 1
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
fi

if ! command -v git >/dev/null 2>&1; then
  echo "error: git not found in PATH" >&2
  exit 1
fi

BASE="${1:-origin/main}"

# Resolve diff list. If BASE doesn't exist, fall back to the working
# tree's untracked + staged files so the script still produces useful
# output during local-only iteration.
if git rev-parse --verify "$BASE" >/dev/null 2>&1; then
  DIFF=$(git diff --name-only "$BASE"...HEAD 2>/dev/null)
  RANGE_DESC="changes vs. $BASE"
else
  DIFF=$(git status --porcelain | awk '{print $2}')
  RANGE_DESC="working-tree changes (no $BASE ref found)"
fi

if [[ -z "$DIFF" ]]; then
  echo "no changes detected in $RANGE_DESC; nothing to check"
  exit 0
fi

echo "=== Pre-launch checklist scan ==="
echo "Range: $RANGE_DESC"
echo
echo "Files in scope:"
echo "$DIFF" | sed 's/^/  /'
echo

# -------------------------------------------------------------------
# Item 1 — threat model
# -------------------------------------------------------------------
# Heuristic: a CLAUDE.md or docs/threat-models/* file is in the diff,
# OR the PR description (read by the human reviewer; we can't read
# from here) explicitly cites one. Print "detected" / "not detected".
echo "[1/6] Threat model — looking for changes to CLAUDE.md or docs/threat-models/..."
if echo "$DIFF" | grep -qE '(^|/)CLAUDE\.md$|^docs/threat-models/'; then
  echo "  ✓ detected: $(echo "$DIFF" | grep -E '(^|/)CLAUDE\.md$|^docs/threat-models/')"
else
  echo "  ? not detected — confirm the threat model lives somewhere durable (CLAUDE.md, docs/threat-models/<name>.md), and link it in the PR."
fi
echo

# -------------------------------------------------------------------
# Item 2 — state-machine impact
# -------------------------------------------------------------------
# Heuristic: state machine changes typically touch state.rs files or
# CLAUDE.md sections that describe the state machine. We grep both.
echo "[2/6] State-machine impact — looking for state-machine-relevant edits..."
SM_HITS=$(echo "$DIFF" | grep -E '(programs/.*/src/state/|backend/.*/models/|services/.*/src/state)' || true)
if [[ -n "$SM_HITS" ]]; then
  echo "  ✓ state-machine-adjacent files in the diff:"
  echo "$SM_HITS" | sed 's/^/    /'
  echo "  → confirm the corresponding ASCII diagram in the relevant CLAUDE.md was updated in this same PR."
else
  echo "  ? not detected — if your change doesn't touch a state machine, mark this item N/A in the PR."
fi
echo

# -------------------------------------------------------------------
# Item 3 — tests
# -------------------------------------------------------------------
# Heuristic: at least one *_test*, *.test.*, tests/, or __tests__/ file
# changed.
echo "[3/6] Tests — looking for test files in the diff..."
TEST_HITS=$(echo "$DIFF" | grep -E '(^|/)(__tests__|tests)/|\.test\.[a-z]+$|_test\.rs$|test_.*\.py$' || true)
if [[ -n "$TEST_HITS" ]]; then
  echo "  ✓ test files in the diff:"
  echo "$TEST_HITS" | sed 's/^/    /'
  echo "  → confirm both happy-path AND every failure-mode rejection are covered."
else
  echo "  ? not detected — every change to a sensitive write surface needs at least happy-path + per-error-variant negative tests."
fi
echo

# -------------------------------------------------------------------
# Item 4 — runbook entry
# -------------------------------------------------------------------
echo "[4/6] Runbook entry — looking for RUNBOOK.md edits..."
if echo "$DIFF" | grep -qE 'RUNBOOK\.md$'; then
  echo "  ✓ detected: $(echo "$DIFF" | grep -E 'RUNBOOK\.md$')"
  echo "  → confirm the entry follows the Signal / Triage / Response / Escalation shape."
else
  echo "  ? not detected — if the change adds a failure mode an on-call would be paged for, add a runbook entry. If purely internal, mark N/A."
fi
echo

# -------------------------------------------------------------------
# Item 5 — devnet rehearsal
# -------------------------------------------------------------------
# Heuristic: nothing reliably greppable in-tree. We always print a
# manual marker — operator confirms in the PR description.
echo "[5/6] Devnet rehearsal — no in-repo artifact; verify manually."
echo "  ! mark this item with the devnet TX signature(s) or test-run evidence in the PR."
echo

# -------------------------------------------------------------------
# Item 6 — public disclosure plan
# -------------------------------------------------------------------
# Heuristic: changes to release-notes, spec files, server.json,
# package.json (version bumps), or shillbot.org status pages signal a
# disclosure plan is in motion. Otherwise the operator marks N/A.
echo "[6/6] Public disclosure plan — looking for release-notes, spec, or version-bump edits..."
DISC_HITS=$(echo "$DIFF" | grep -E '(^|/)(CHANGELOG|RELEASES|RELEASE_NOTES)\.md$|^docs/specs/|^services/.*/server\.json$|^services/.*/package\.json$|^Cargo\.toml$' || true)
if [[ -n "$DISC_HITS" ]]; then
  echo "  ✓ disclosure-adjacent files in the diff:"
  echo "$DISC_HITS" | sed 's/^/    /'
  echo "  → confirm the change is reflected in user-facing release notes / version bump."
else
  echo "  ? not detected — if the change is user-facing, add release-notes / spec bump / status-page entry. If purely internal, mark N/A."
fi
echo

echo "=== End of scan ==="
echo
echo "Reminder: this script is advisory. The actual gate is the PR"
echo "template block from docs/PRE_LAUNCH_CHECKLIST.md being filled in"
echo "with evidence the reviewer can audit."

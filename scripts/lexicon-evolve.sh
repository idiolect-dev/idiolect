#!/usr/bin/env bash
# Lexicon evolution policy: drive a lexicon revision through the
# six-stage panproto-backed pipeline.
#
# CAVEAT: this script is the runnable spec of the policy, not a
# turnkey tool. Several panproto subcommands referenced here (e.g.
# `schema diff --json`, `schema lens verify --chain`) follow the
# `panproto-build-migration` and `panproto-protolenses` skill specs;
# the on-disk panproto CLI release may lag those specs. Stages that
# fail because a subcommand is unavailable should be treated as
# "skip; gate will run once panproto catches up".
#
# Usage:
#   scripts/lexicon-evolve.sh <nsid> <old-rev> <new-rev>
#
# Example:
#   scripts/lexicon-evolve.sh dev.idiolect.vocab 1 2
#
# What it does:
#   Stage 0  diff           panproto's structured schema diff
#   Stage 1  derive          auto-generate a protolens chain
#   Stage 2  classify        record the chain's information-theoretic class
#   Stage 3  coercion check  honesty gate on cross-kind coercions
#   Stage 4  verify corpus   roundtrip law check against live records
#   Stage 5  publish lens    serialize chain and prepare for PDS publish
#
# Outputs land in:
#   migrations/<nsid>/<old>-<new>/{diff.json,chain.json,classification.txt,verification.json}
#
# Exit codes:
#   0   pipeline ran cleanly; lens ready for review/publish
#   1   stage failure; see stderr for which stage and why
#   2   classification gate triggered manual review (Affine/General)
#
# Per the standing policy: every lexicon revision must ship a
# verified, classified, published lens. A stage failure halts the
# pipeline; the gate at Stage 2 may surface a chain that requires
# manual lens authoring + governance sign-off.

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <nsid> <old-rev> <new-rev>" >&2
  exit 1
fi

NSID="$1"
OLD_REV="$2"
NEW_REV="$3"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEX_DIR="$REPO_ROOT/lexicons"
MIG_DIR="$REPO_ROOT/migrations/$NSID/$OLD_REV-$NEW_REV"
mkdir -p "$MIG_DIR"

# Resolve the lexicon JSON paths. Lexicons are organized by the
# reverse-DNS path of their NSID with a trailing .json.
nsid_to_path() {
  local nsid="$1"
  local rev="$2"
  echo "$LEX_DIR/${nsid//.//}.${rev}.json"
}

OLD_PATH="$(nsid_to_path "$NSID" "$OLD_REV")"
NEW_PATH="$(nsid_to_path "$NSID" "$NEW_REV")"

if [[ ! -f "$OLD_PATH" || ! -f "$NEW_PATH" ]]; then
  # Versioned filenames not present; fall back to the unversioned
  # canonical path. The "old" version is whatever git has at the
  # previous tag/branch; the caller is responsible for materializing
  # it via `git show <ref>:<path>` before invoking this script.
  CANONICAL="$LEX_DIR/${NSID//.//}.json"
  if [[ ! -f "$CANONICAL" ]]; then
    echo "lexicon not found at $CANONICAL" >&2
    exit 1
  fi
  OLD_PATH="$CANONICAL"
  NEW_PATH="$CANONICAL"
  echo "  warning: using canonical path for both old and new; this only" >&2
  echo "  exercises stages 1-2 against the current revision." >&2
fi

# Resolve panproto CLI; emit a clear failure if missing so the
# caller knows which prerequisite to install rather than seeing a
# generic command-not-found.
if ! command -v schema >/dev/null 2>&1; then
  echo "panproto 'schema' CLI not on PATH; install per panproto-getting-started" >&2
  exit 1
fi

# Hint file is optional; consumers seed anchors here for ambiguous
# renames.
HINTS="$MIG_DIR/hints.json"

DIFF="$MIG_DIR/diff.json"
CHAIN="$MIG_DIR/chain.json"
CLASS="$MIG_DIR/classification.txt"
VERIFY="$MIG_DIR/verification.json"

# Stage 0 — Diff.
echo "[stage 0] diff $OLD_PATH -> $NEW_PATH"
schema diff --src "$OLD_PATH" --tgt "$NEW_PATH" --json > "$DIFF"

# Stage 1 — Auto-derive a protolens chain.
echo "[stage 1] generate protolens chain"
if [[ -f "$HINTS" ]]; then
  schema lens generate "$OLD_PATH" "$NEW_PATH" --hints "$HINTS" --json > "$CHAIN"
else
  schema lens generate "$OLD_PATH" "$NEW_PATH" --json > "$CHAIN"
fi

# Stage 2 — Classify and gate.
echo "[stage 2] classify"
schema lens inspect "$CHAIN" --protocol atproto > "$CLASS"

class_token() {
  # The first occurrence of one of the canonical class tokens wins.
  grep -oE 'Iso|Injection|Projection|Affine|General' "$CLASS" | head -n 1 || true
}
CLASSIFICATION="$(class_token)"
echo "  -> $CLASSIFICATION"

case "$CLASSIFICATION" in
  Iso|Injection)
    echo "  auto-merge eligible"
    ;;
  Projection)
    echo "  PR review required: complement persistence + data-loss disclosure"
    ;;
  Affine)
    echo "  PR review + community recommendation required"
    GATE_EXIT=2
    ;;
  General)
    echo "  manual lens authoring required: gate held until sign-off"
    GATE_EXIT=2
    ;;
  *)
    echo "  unrecognized classification token; treating as General" >&2
    GATE_EXIT=2
    ;;
esac

# Stage 3 — Coercion law check (only if the chain references coerce
# steps; the panproto CLI exits cleanly when no coercions are
# present).
echo "[stage 3] coercion law check"
if grep -q '"coerce"' "$CHAIN"; then
  schema theory check-coercion-laws "$CHAIN" --json > "$MIG_DIR/coercion.json"
fi

# Stage 4 — Roundtrip verification against the live record corpus
# snapshot. Corpus path is configurable via $CORPUS or defaults to
# `corpus/<nsid>/`. Stage runs only when a corpus exists.
CORPUS="${CORPUS:-$REPO_ROOT/corpus/$NSID}"
if [[ -d "$CORPUS" ]]; then
  echo "[stage 4] verify against corpus $CORPUS"
  schema lens verify "$CORPUS" --protocol atproto --schema "$NEW_PATH" --chain "$CHAIN" --json > "$VERIFY"
else
  echo "[stage 4] no corpus at $CORPUS; skipping (CI will gate this)"
fi

# Stage 5 — Publish lens record (preparation only; the actual PDS
# write is gated behind explicit operator action).
echo "[stage 5] lens prepared at $CHAIN"
echo "  to publish: idiolect-cli publish-lens --collection dev.idiolect.lens --chain $CHAIN"

# Exit code reflects the classification gate.
exit "${GATE_EXIT:-0}"

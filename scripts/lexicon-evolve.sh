#!/usr/bin/env bash
# Produce compatibility and optic-classification evidence for one ATProto
# Lexicon change with Panproto's released CLI.
#
# Usage:
#   scripts/lexicon-evolve.sh <nsid> <old-path> <new-path>
#
# Outputs default to migrations/<nsid>/comparison/. Set OUTPUT_DIR to place
# CI artifacts elsewhere.
#
# Exit codes:
#   0  evidence produced; change is auto-merge eligible or needs PR review
#   1  invalid input or Panproto command failure
#   2  Panproto could not load/classify the schemas, or the optic gate held

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <nsid> <old-path> <new-path>" >&2
  exit 1
fi

NSID="$1"
OLD_PATH="$2"
NEW_PATH="$3"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIG_DIR="${OUTPUT_DIR:-$REPO_ROOT/migrations/$NSID/comparison}"

for path in "$OLD_PATH" "$NEW_PATH"; do
  if [[ ! -f "$path" ]]; then
    echo "lexicon not found: $path" >&2
    exit 1
  fi
done

if ! command -v schema >/dev/null 2>&1; then
  echo "Panproto 'schema' CLI not on PATH; install Panproto v0.74.4" >&2
  exit 1
fi

mkdir -p "$MIG_DIR"
COMPAT="$MIG_DIR/compat.json"
DIFF="$MIG_DIR/diff.txt"
CLASS="$MIG_DIR/classification.txt"

# `compat` accepts a protocol override for a bare Lexicon file, while `diff`
# selects document parsers through a project manifest. Wrap each revision in a
# one-document ATProto project so both commands compare the same parsed graph.
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/idiolect-lexicon-evolve.XXXXXX")"
trap 'rm -rf -- "$WORK_DIR"' EXIT
OLD_PROJECT="$WORK_DIR/old"
NEW_PROJECT="$WORK_DIR/new"
mkdir -p "$OLD_PROJECT/lexicons" "$NEW_PROJECT/lexicons"
cp "$OLD_PATH" "$OLD_PROJECT/lexicons/schema.json"
cp "$NEW_PATH" "$NEW_PROJECT/lexicons/schema.json"
for project in "$OLD_PROJECT" "$NEW_PROJECT"; do
  printf '%s\n' \
    '[workspace]' \
    'name = "idiolect-lexicon-evolution"' \
    '' \
    '[[package]]' \
    'name = "lexicons"' \
    'path = "lexicons"' \
    'protocol = "atproto"' > "$project/panproto.toml"
done

# Compatibility is evidence rather than the final policy gate. Exit 1 means
# Panproto found breaking changes and should not prevent optic classification;
# exit 2 means the input or protocol could not be loaded and is fatal.
echo "[stage 0] classify compatibility: $OLD_PATH -> $NEW_PATH"
set +e
schema compat --protocol atproto --format json "$OLD_PROJECT" "$NEW_PROJECT" > "$COMPAT"
COMPAT_EXIT=$?
set -e
case "$COMPAT_EXIT" in
  0) echo "  no breaking changes" ;;
  1) echo "  breaking changes found; see $COMPAT" ;;
  *)
    echo "Panproto compatibility classification failed (exit $COMPAT_EXIT)" >&2
    exit 2
    ;;
esac

# Panproto v0.74.4 parses the manifest-backed ATProto projects through `diff`
# and auto-classifies their generated migration. Chain serialization is a
# separate authoring step because the released CLI's file-path `--lens --save`
# route is not yet wired to the same project loader.
echo "[stage 1] diff and classify optic"
schema diff --detect-renames --optic-kind "$OLD_PROJECT" "$NEW_PROJECT" > "$DIFF"

CLASSIFICATION="$({
  grep -oE 'Optic classification: (iso|lens|prism|affine|traversal)' "$DIFF" || true
} | tail -n 1 | sed -E 's/.*: //')"

if [[ -z "$CLASSIFICATION" ]]; then
  echo "Panproto did not emit an optic classification; see $DIFF" >&2
  exit 2
fi

{
  echo "nsid=$NSID"
  echo "optic_kind=$CLASSIFICATION"
  echo "compat_exit=$COMPAT_EXIT"
} > "$CLASS"
echo "  optic kind: $CLASSIFICATION"

# Policy maps the current OpticKind API onto review requirements. A prism is
# injection-like; a lens is projection-like and must disclose complement and
# reverse-direction loss. Affine/traversal changes need an authored chain and
# governance sign-off before they can merge.
case "$CLASSIFICATION" in
  iso|prism)
    echo "  auto-merge eligible"
    ;;
  lens)
    echo "  PR review required: confirm complement persistence and data-loss disclosure"
    ;;
  affine|traversal)
    echo "  manual chain authoring and governance sign-off required" >&2
    exit 2
    ;;
  *)
    echo "unrecognized optic classification: $CLASSIFICATION" >&2
    exit 2
    ;;
esac

echo "Evidence written to $MIG_DIR"
echo "A publishable chain, once authored and verified, uses collection dev.panproto.schema.lens."

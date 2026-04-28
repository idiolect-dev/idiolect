## Lens: community.blacksky.assembly.vote ↔ dev.idiolect.deliberationVote
##
## Optic class: Projection (forward Acorn -> idiolect carries no
## information loss; reverse drops idiolect's optional `weight` and
## `rationale`). The §7 evolution policy classifies this as a
## Projection on the reverse direction; complement persistence
## captures the dropped fields so a follow-up reverse-translation
## restores them.
##
## The numeric -1/0/1 -> slug coercion is handled by a CoerceType
## step with CoercionClass=Iso (the integer encoding is bijective
## with three slugs). The slug -> idiolect-canonical-slug step is a
## RenameEdgeName at the prop level, classified as Iso via
## map_enum.

let L = import "panproto/lens.ncl" in
{
  id = "at://did:plc:idiolect-acorn/dev.idiolect.lens/vote-to-deliberation-vote-v1",
  source = "community.blacksky.assembly.vote",
  target = "dev.idiolect.deliberationVote",
  steps = [
    # Acorn `subject` (com.atproto.repo.strongRef) is structurally
    # identical to idiolect's strongRecordRef; rename only.
    L.rename "subject" "subject",

    # Acorn integer value -> idiolect stance slug. Three-way map
    # captured as a coercion. -1=agree, 0=pass, 1=disagree
    # per Acorn's lexicon prose.
    L.coerce "value" "Int" "Str" {
      class = "Iso",
      forward = "case value: -1 => \"agree\"; 0 => \"pass\"; 1 => \"disagree\"",
      inverse = "case value: \"agree\" => -1; \"pass\" => 0; \"disagree\" => 1",
    },
    L.rename "value" "stance",

    # Idiolect-only fields land with sensible defaults on the
    # reverse direction; complement captures them.
    L.add "stanceVocab" "ref" {
      uri = "at://did:plc:idiolect/dev.idiolect.vocab/vote-stances-v1",
    },
    L.add "weight" "integer" null,
    L.add "rationale" "string" null,

    # createdAt is structurally identical.
    L.rename "createdAt" "createdAt",
  ],
} | L.Lens

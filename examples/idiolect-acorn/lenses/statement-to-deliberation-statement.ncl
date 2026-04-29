## Lens: community.blacksky.assembly.statement ↔ dev.idiolect.deliberationStatement
##
## Optic class: Injection. Acorn's statement is a strict subset of
## idiolect's deliberationStatement (which adds `classification`).
## Forward injects with a default `classification = claim`; reverse
## drops it. No information loss going forward.

let L = import "panproto/lens.ncl" in
{
  id = "at://did:plc:idiolect-acorn/dev.idiolect.lens/statement-to-deliberation-statement-v1",
  source = "community.blacksky.assembly.statement",
  target = "dev.idiolect.deliberationStatement",
  steps = [
    L.rename "conversation" "deliberation",
    L.rename "text" "text",
    L.rename "anonymous" "anonymous",
    L.rename "createdAt" "createdAt",

    # Default classification = "claim"; communities authoring against
    # idiolect's richer vocab override.
    L.add "classification" "string" "claim",
    L.add "classificationVocab" "ref" {
      uri = "at://did:plc:idiolect/dev.idiolect.vocab/statement-classifications-v1",
    },
  ],
} | L.Lens

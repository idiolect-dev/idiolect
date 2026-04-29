## Lens: community.blacksky.assembly.conversation ↔ dev.idiolect.deliberation
##
## Optic class: Iso after vocab default fill-in. Acorn's
## conversation carries no classification or status; the lens fills
## these with sensible defaults from the canonical idiolect vocabs
## on the forward direction and drops them on the reverse. With
## complement persistence, the round-trip is faithful.

let L = import "panproto/lens.ncl" in
{
  id = "at://did:plc:idiolect-acorn/dev.idiolect.lens/conversation-to-deliberation-v1",
  source = "community.blacksky.assembly.conversation",
  target = "dev.idiolect.deliberation",
  steps = [
    L.rename "topic" "topic",
    L.rename "description" "description",
    L.rename "authRequired" "authRequired",
    L.rename "createdAt" "createdAt",

    # The owningCommunity field is a derived AT-URI: every Blacksky
    # conversation is owned by the Blacksky community record.
    L.add "owningCommunity" "at-uri"
      "at://did:plc:blacksky/dev.idiolect.community/main",

    # Defaulted fields. Forward fills, reverse drops to complement.
    L.add "classification" "string" "question",
    L.add "classificationVocab" "ref" {
      uri = "at://did:plc:idiolect/dev.idiolect.vocab/deliberation-classifications-v1",
    },
    L.add "status" "string" "open",
    L.add "statusVocab" "ref" {
      uri = "at://did:plc:idiolect/dev.idiolect.vocab/deliberation-statuses-v1",
    },
  ],
} | L.Lens

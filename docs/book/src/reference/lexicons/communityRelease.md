# `dev.idiolect.communityRelease`

`communityRelease` indexes an immutable, signed cut of community-governed definitions.

The required fields are `community`, `version`, `manifestDigest`, `digest`, `changes`, `signatures`, and `createdAt`. `changes` contains approved `changeProposal` at-uris. Optional artifacts carry a location, digest, and media type. Optional dependencies record peer community, relation, selected release, and update policy.

Each signature contains signer DID, `ES256` algorithm, public key, detached signature, and signing time. The signature covers the canonical unsigned bundle digest, not the mutable ATProto envelope around the record.

The record is an index. A portable release bundle carries the full manifest, local packet digests, artifact digests, dependencies, and signatures needed for offline verification.

# `dev.idiolect.federation`

`federation` declares a relationship between two communities without conflating recognition with semantic equivalence.

Required fields are `community`, `peer`, `relation`, `updatePolicy`, and `createdAt`. Known relations are `follows`, `extends`, `bridges`, and `forked-from`. Known update policies are `review`, `follow-compatible`, and `pinned`.

`release` selects a peer release or version requirement. `mappings` contains published lens or mapping at-uris. `notes` may explain a social or operational relationship that does not reduce to a mapping.

A record with no mappings is valid. It asserts the relationship, not that records can be translated or that concepts are equivalent.

# dev.idiolect.community

A group of DIDs that declare shared conventions. Self-constituted:
there is no central roll and no grading of legitimacy. Communities
may be small and many.

> **Source:** [`lexicons/dev/idiolect/community.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/community.json)
> · **Rust:** [`idiolect_records::Community`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Community.html)
> · **TS:** `@idiolect-dev/schema/community`
> · **Fixture:** `idiolect_records::examples::community`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string (≤128) | yes | Human-readable community name. |
| `description` | string (≤2000 graphemes) | yes | Purpose, norms, scope. Narrative. |
| `members` | array of did (≤500) | no | Inline membership for small communities. |
| `roleAssignments` | array of `roleAssignment` (≤500) | no | Sparse role assignments where the role differs from the default. |
| `memberRoleVocab` | `vocabRef` | no | Vocab the role slugs resolve against. |
| `recordHosting` | open enum | no | `member-hosted` / `community-hosted` / `hybrid`. |
| `appviewEndpoint` | uri | no | URL of the community AppView when `recordHosting` is non-default. |
| `membershipRoll` | at-uri | no | External membership record (for communities above ~200 members). |
| `coreSchemas` | array of `schemaRef` | no | Schemas the community treats as canonical. |
| `coreLenses` | array of `lensRef` | no | Lenses the community treats as canonical. |
| `endorsedCommunities` | array of at-uri | no | Other communities recognised as legitimate interlocutors. Not transitive. |
| `conventions` | array (≤64) of structured convention variants | no | Decidable subset of community conventions. |
| `conventionsText` | string (≤10000 graphemes) | no | Narrative conventions: style guides, norms not expressible structurally. |
| `createdAt` | datetime | yes | Publication timestamp. |

### `roleAssignment`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `did` | did | yes | DID of the member. |
| `role` | open enum | yes | `member` / `moderator` / `delegate` / `author`. |

A DID may appear multiple times when the role vocabulary supports
multiple roles per member.

## The convention variants

Each entry in `conventions` is one of three shapes:

| Variant | Captures |
| --- | --- |
| `conventionReviewCadence` | Maximum business-days expected before a review is posted, with optional scope narrowing. |
| `conventionVerificationReq` | A verification kind (and optionally a specific property) the community requires before endorsing a lens. |
| `conventionDeprecationPolicy` | Minimum deprecation notice in days; whether deprecations require a replacement pointer. |

The structured subset is what consumers can match on
mechanically. Style guides, tone, and norms not expressible as a
predicate live in `conventionsText`.

## Field details

### `members` versus `membershipRoll`

Two ways to represent membership:

- **Inline `members`** is appropriate for small communities. The
  list lives directly on the community record; reading the
  community gives you the membership in one fetch. Capped at 500
  entries.
- **External `membershipRoll`** is a pointer to a separate record
  that maintains the roll. Appropriate for larger communities
  (above ~200 members) where the roll is updated frequently and
  shouldn't bloat the community record itself.

A community may use both for the transition period while moving
from inline to external; consumers union the two sets.

### `roleAssignments` versus the default role

The default role (named on the role vocabulary's top node) applies
to every member who does not have an explicit `roleAssignment`.
Only members whose role *differs* from the default need an entry.
A 500-member community with five moderators carries five
`roleAssignment` entries, not five hundred.

The shipped default vocabulary seeds `member` (top), `moderator`,
`delegate`, `author`. A community extends by referencing a custom
`memberRoleVocab` with additional roles.

### `recordHosting`

| Slug | What it means |
| --- | --- |
| `member-hosted` | Records live on individual member PDSes (default ATProto). |
| `community-hosted` | Records live on a community AppView, gated by membership (Acorn-style). |
| `hybrid` | Both. Some records are member-hosted, others are AppView-hosted. |

Consumers crawling for community records use this to choose a
surface. A community that publishes `community-hosted` plus an
`appviewEndpoint` is telling consumers to route XRPC reads
through the AppView instead of crawling member PDSes.

### `endorsedCommunities`

A community names other communities it recognises as legitimate
interlocutors. The endorsement is *not* transitive: A endorsing B
and B endorsing C does not imply A endorsing C. The substrate
records the assertion; consumers decide what to do with it. Common
patterns: a quorum policy that requires endorsements from $k$
trusted communities; a denylist that excludes communities not
endorsed by any trusted party.

## Example

```json
{
  "$type": "dev.idiolect.community",
  "name": "tutorial",
  "description": "Tutorial community for the idiolect documentation.",
  "members": [
    "did:plc:alice", "did:plc:bob", "did:plc:carol"
  ],
  "roleAssignments": [
    { "did": "did:plc:alice", "role": "moderator" }
  ],
  "recordHosting": "member-hosted",
  "coreSchemas": [
    { "uri": "at://did:plc:tutorial/dev.panproto.schema.schema/post-v1" }
  ],
  "conventions": [
    {
      "$type": "dev.idiolect.community#conventionVerificationReq",
      "kind": "roundtrip-test"
    },
    {
      "$type": "dev.idiolect.community#conventionDeprecationPolicy",
      "noticePeriodDays": 90,
      "replacementRequired": true
    }
  ],
  "conventionsText": "Lens authors review within five business days. Style: terse, factual.",
  "createdAt": "2026-04-19T00:00:00.000Z"
}
```

## Concept references

- [Concepts: Idiolect, dialect, language](../../concepts/idiolect-dialect-language.md)
- [Lexicons: dialect](./dialect.md) · [recommendation](./recommendation.md) · [bounty](./bounty.md)

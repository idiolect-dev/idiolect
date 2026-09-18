# `dev.idiolect.changeProposal`

`changeProposal` publishes a governed definition change as one inspectable packet.

## Required fields

| Field | Type | Meaning |
|---|---:|---|
| `community` | at-uri | Governing `dev.idiolect.community` record. |
| `title` | string | Short participant-facing name. |
| `summary` | string | Intent, scope, and expected benefit. |
| `author` | DID | Proposer. |
| `status` | open enum | `draft`, `review`, `approved`, `rejected`, `released`, or `superseded`. |
| `source`, `target` | `#schemaEndpoint` | Protocol, location, and content digest at each side. |
| `compatibility` | open enum | `fully-compatible`, `backward-compatible`, or `breaking`. |
| `verificationStatus` | open enum | `verified`, `refuted`, `incomplete`, or `not-run`. |
| `createdAt`, `updatedAt` | datetime | Lifecycle timestamps. |

Optional fields carry the Panproto optic kind, participant-facing consequences, affected systems, rollback plan, deliberation at-uri, attributable reviews, and published verification at-uris.

`verificationStatus=incomplete` is distinct from `verified` and must not pass a release gate. Consumers should preserve unknown enum slugs and resolve community extensions under their local vocabulary policy.

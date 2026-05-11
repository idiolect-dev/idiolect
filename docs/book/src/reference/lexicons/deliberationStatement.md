# dev.idiolect.deliberationStatement

A participant utterance submitted to a
[`deliberation`](./deliberation.md). Statements are the units
votes attach to; the deliberation itself is not voted on
directly. Classification is an open-enum slug resolved against a
community vocabulary, so communities that draw the line between
`claim` and `proposal` differently can extend or remap without
forking the lexicon.

> **Source:** [`lexicons/dev/idiolect/deliberationStatement.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/deliberationStatement.json)
> · **Rust:** [`idiolect_records::DeliberationStatement`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.DeliberationStatement.html)
> · **TS:** `@idiolect-dev/schema/deliberationStatement`
> · **Fixture:** `idiolect_records::examples::deliberation_statement`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `deliberation` | `strongRecordRef` | yes | AT-URI + CID for the deliberation this statement participates in. |
| `text` | string (≤400 graphemes) | yes | Statement text. |
| `classification` | open enum | no | `claim` / `proposal` / `dissent` / `clarification` / `question`. |
| `classificationVocab` | `vocabRef` | no | Vocab the classification slug resolves against. |
| `anonymous` | boolean (default `false`) | no | Whether the statement was submitted anonymously. |
| `createdAt` | datetime | yes | Publication timestamp. |

## Field details

### Why `strongRecordRef` for the deliberation pointer

`deliberation` carries both the AT-URI and the CID. Pinning by
CID prevents a later deliberation revision from silently
rescoping the statement. A consumer that reads the statement and
follows the pointer gets the exact deliberation revision the
participant was responding to.

This matters when deliberations are edited mid-process (e.g. the
publisher clarifies the topic). Statements published before the
edit pin the pre-edit revision; statements published after pin
the post-edit revision. Folds and consumers can distinguish.

### `text`

The statement itself. The 400-grapheme cap is conventional, not
arbitrary: brevity is what makes statements voteable. Long-form
context belongs on the deliberation record's `description` or
in a community-published companion document linked from the
description.

### `classification`

| Slug | What it captures |
| --- | --- |
| `claim` | An assertion of fact or opinion. |
| `proposal` | A specific proposed action. |
| `dissent` | An objection to a prior statement or to the deliberation framing. |
| `clarification` | A request for or provision of clarification. |
| `question` | An open question requiring an answer. |

Classifications are *argumentative roles*, not topics. A
community that draws different distinctions (`amendment`,
`process-objection`, `tangent`, ...) extends via
`classificationVocab`. The classification is optional; a
deliberation that wants to stay agnostic on argumentative roles
omits it.

### `anonymous`

When `true`, the statement was submitted anonymously. The
typical implementation: the statement is authored on a
designated service DID rather than the participant's personal
repo, so the repo signature does not reveal identity. Consumers
that need provenance match on the repo DID (the service DID),
not on this record's content.

The flag is a *declaration*: the substrate does not enforce
anonymity beyond what the publishing rail provides. A community
that wants strong anonymity uses an anonymizing service DID with
its own access controls.

## Example

```json
{
  "$type": "dev.idiolect.deliberationStatement",
  "deliberation": {
    "uri": "at://did:plc:community/dev.idiolect.deliberation/3l5",
    "cid": "bafy..."
  },
  "text": "Adopting the v2 lens would lose dialect markers on legacy posts.",
  "classification": "dissent",
  "anonymous": false,
  "createdAt": "2026-04-19T00:00:00.000Z"
}
```

## Concept references

- [Concepts: Deliberation](../../concepts/deliberation.md)
- [Lexicons: deliberation](./deliberation.md) · [deliberationVote](./deliberationVote.md) · [deliberationOutcome](./deliberationOutcome.md)

# dev.idiolect.deliberationVote

A stance taken on a [`deliberationStatement`](./deliberationStatement.md).
Stance is an open-enum slug resolved against a community-published
vote-stance vocabulary. The Acorn-style three-way default
(`agree` / `pass` / `disagree`) is canonical; richer vocabularies
(conditional-agree, abstain-with-reason, ranked preference) are
expressible by referencing a different vocab. Optional `weight`
and `rationale` capture additional signal that observers can fold;
consumers that don't need them ignore them.

> **Source:** [`lexicons/dev/idiolect/deliberationVote.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/deliberationVote.json)
> · **Rust:** [`idiolect_records::DeliberationVote`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.DeliberationVote.html)
> · **TS:** `@idiolect-dev/schema/deliberationVote`
> · **Fixture:** `idiolect_records::examples::deliberation_vote`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `subject` | `strongRecordRef` | yes | AT-URI + CID for the statement being voted on. |
| `stance` | open enum | yes | `agree` / `pass` / `disagree`. |
| `stanceVocab` | `vocabRef` | no | Vocab the stance slug resolves against. |
| `weight` | integer ∈ [0, 1000] | no | Optional ranking signal. Convention: scaled by 1000 for the 0.0–1.0 range. |
| `rationale` | string (≤500 graphemes) | no | Optional narrative reason. |
| `createdAt` | datetime | yes | Publication timestamp. |

## Field details

### Why pin the statement by CID

The `subject` field carries both AT-URI and CID. A statement
edited after the vote was cast does not retroactively change what
was voted on. Observers folding the tally read the CID to confirm
they are aggregating votes against the same statement revision.

If a statement is edited and a participant wants to vote on the
new revision, that is a separate vote record with a different
`subject` CID. The substrate does not collapse votes across
revisions; observers do, when their fold method specifies it.

### `stance`

The default vocabulary seeds three slugs:

| Slug | Meaning |
| --- | --- |
| `agree` | Affirms the statement. |
| `pass` | Abstains. |
| `disagree` | Rejects the statement. |

These match Acorn's `+1 / 0 / -1` convention. Communities that
want richer stances publish their own `stanceVocab`. Common
extensions:

- `conditional-agree` — agree under specified conditions.
- `abstain-with-reason` — explicit non-vote with a rationale.
- `rank-1`, `rank-2`, ... — ranked preference.

The `stanceVocab` machinery means consumers do not have to
coordinate on which vocab is in use ahead of time. The vote
record either references an explicit `stanceVocab` or falls back
to the canonical idiolect default.

### `weight`

Optional ranking signal. The `[0, 1000]` integer range encodes
the 0.0–1.0 floating-point range with three decimal places of
precision. Convention follows `pub.chive.graph.edge#weight`.

Consumers that aggregate votes uniformly ignore `weight`. Ranked
or weighted aggregations consume it. A community that wants
quadratic voting publishes a `vote-weights` companion vocabulary
and uses `weight` to encode the scheme; observers running a
quadratic-vote fold read both the stance and the weight.

### `rationale`

Optional narrative. Tally folds do not consume `rationale`;
consumer surfaces (e.g. a deliberation viewer) display it
alongside the vote. The 500-grapheme cap matches the
deliberation-statement length: brevity is conventional.

### Anonymous votes

There is no `anonymous` flag on votes (unlike statements). If a
community wants anonymous voting, the implementation is the same
as anonymous statements: votes are authored on a designated
service DID rather than the voter's personal repo. The repo
signature is the authoritative provenance signal.

## Example

```json
{
  "$type": "dev.idiolect.deliberationVote",
  "subject": {
    "uri": "at://did:plc:community/dev.idiolect.deliberationStatement/3l5",
    "cid": "bafy..."
  },
  "stance": "agree",
  "weight": 750,
  "rationale": "Strong agree, conditional on the dialect-marker preservation work shipping first.",
  "createdAt": "2026-04-19T00:00:00.000Z"
}
```

## Folded into the outcome

A vote does not produce an outcome on its own. An observer reads
the vote stream for a deliberation, folds by `(statement, stance)`
(plus optional weight aggregation), and publishes a
[`deliberationOutcome`](./deliberationOutcome.md) record. Multiple
observers may publish concurrent outcomes; consumers that want
consensus require quorum across trusted observers.

## Concept references

- [Concepts: Deliberation](../../concepts/deliberation.md)
- [Concepts: Observer protocol](../../concepts/observer.md)
- [Lexicons: deliberation](./deliberation.md) · [deliberationStatement](./deliberationStatement.md) · [deliberationOutcome](./deliberationOutcome.md)

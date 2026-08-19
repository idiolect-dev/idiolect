# dev.idiolect.deliberationOutcome

`dev.idiolect.deliberationOutcome` is an observer-aggregated tally for a
[`deliberation`](./deliberation.md), pinned through a
[strong reference](../../glossary.md#strong-reference).
An observer, rather than a participant, produces this record by folding the
vote stream and publishing the result from the observer's repository.
Consumers reading a closed deliberation can fetch the outcome
directly rather than re-folding every vote. Tallies are
per-statement and per-stance, so consumers can render a
Polis-style opinion map without further computation.

> **Source:** [`lexicons/dev/idiolect/deliberationOutcome.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/deliberationOutcome.json)
> · **Rust:** [`idiolect_records::DeliberationOutcome`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.DeliberationOutcome.html)
> · **TS:** `@idiolect-dev/schema/deliberationOutcome`
> · **Fixture:** `idiolect_records::examples::deliberation_outcome`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `deliberation` | `strongRecordRef` | yes | AT-URI + CID for the deliberation. |
| `statementTallies` | array (≤4096) of `statementTally` | yes | Per-statement vote counts. |
| `adopted` | array (≤256) of `strongRecordRef` | no | Statements the community adopted. |
| `stanceVocab` | `vocabRef` | no | Vocab the per-tally stance slugs resolve against. |
| `computedAt` | datetime | yes | When the observer computed this tally. |
| `tool` | `tool` | no | Identity and version of the aggregator. |
| `occurredAt` | datetime | yes | Publication timestamp. |

### `statementTally`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `statement` | `strongRecordRef` | yes | The statement these counts aggregate. |
| `counts` | array (≤64) of `stanceCount` | yes | Per-stance vote counts. |
| `weightedCounts` | array (≤64) of `stanceCount` | no | Per-stance weighted vote counts (when votes carried `weight`). Scaled by 1000. |

### `stanceCount`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `stance` | string (≤256) | yes | Stance slug, resolved through the outcome's `stanceVocab`. |
| `count` | non-negative integer | yes | Vote count. For `weightedCounts`, scaled by 1000. |

## Field details

### Publisher

The deliberation owns the topic. Participants own the statements
and votes. The aggregate is *opinion*: it depends on the
observer's fold method, the cut-off time, and which encounter
kinds it weights. Two observers can produce different outcomes for the same
deliberation.

Outcomes are signed observer records, so consumers can identify their
publishers and compare compatible folds. A consumer that distrusts one
observer's fold can:

- Fetch all outcomes for the deliberation.
- Pick one based on the observer's identity or the `tool` field.
- Require quorum among trusted observers.
- Re-fold the vote stream itself.

### `stanceVocab`

The outcome record uses *one* stance vocabulary across all
tallies. An observer that sees votes referencing different
vocabularies must either:

- Publish separate outcomes per vocab, each tallying votes that
  share a vocab.
- First translate via a `mapEnum` lens (see
  [Open enums](../../concepts/open-enums.md) into a single
  target vocabulary, then tally.

Mixing vocabularies in a single outcome is invalid: the same
slug in two different vocabularies has different semantics, and
adding their counts is meaningless.

### `statementTallies`

The array contains one entry per statement that received at least one vote.
Statements with zero votes are omitted. Each tally carries:

- The statement (strong-ref, so consumers fetching the tally can
  fetch the exact statement revision being tallied).
- The per-stance counts.
- Optional weighted counts when the underlying votes carried
  `weight`.

The 4096-entry cap matches the maximum statement count per
deliberation in practice. Communities expecting more should
publish multiple outcome records partitioned by statement
window.

### `adopted`

A list of strong-refs to statements the community adopted as
the deliberation's resolution. An adopted statement is one the
community treats as the answer to a question, the resolution of
a proposal, or the action item from a grievance.

Adoption is a community decision, not a fold rule. The observer
publishing the outcome typically follows the deliberation's
publishing community: their criterion for adoption (majority
agree, supermajority, consensus) is what the observer encodes
in this list. A different observer running a different criterion
would publish a different outcome.

`adopted` is empty when the deliberation closed without
adoption (rejected, tabled, or closed without resolution).

### `tool` and method versioning

The `tool` field carries the aggregator's identity and version.
Different tools or versions may implement different algorithms. Consumers
should compare their outcomes only when the method semantics align; the `tool`
field identifies the implementation used.

## Example

```json
{
  "$type": "dev.idiolect.deliberationOutcome",
  "deliberation": {
    "uri": "at://did:plc:community/dev.idiolect.deliberation/3l5",
    "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm"
  },
  "statementTallies": [
    {
      "statement": {
        "uri": "at://did:plc:community/dev.idiolect.deliberationStatement/stmt1",
        "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm"
      },
      "counts": [
        { "stance": "agree",    "count": 42 },
        { "stance": "pass",     "count": 7  },
        { "stance": "disagree", "count": 3  }
      ]
    },
    {
      "statement": {
        "uri": "at://did:plc:community/dev.idiolect.deliberationStatement/stmt2",
        "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm"
      },
      "counts": [
        { "stance": "agree",    "count": 18 },
        { "stance": "pass",     "count": 12 },
        { "stance": "disagree", "count": 22 }
      ]
    }
  ],
  "adopted": [
    {
      "uri": "at://did:plc:community/dev.idiolect.deliberationStatement/stmt1",
      "cid": "bafyreidfcm4u3vnuph5ltwdpssiz3a4xfbm2otjrdisftwnbfmnxd6lsxm"
    }
  ],
  "computedAt": "2026-04-30T00:00:00.000Z",
  "tool": {
    "name": "deliberation-tally",
    "version": "1.0.0"
  },
  "occurredAt": "2026-04-30T00:01:00.000Z"
}
```

## Concept references

- [Concepts: Deliberation](../../concepts/deliberation.md)
- [Concepts: Observer protocol](../../concepts/observer.md)
- [Lexicons: deliberation](./deliberation.md) · [deliberationVote](./deliberationVote.md) · [observation](./observation.md)

# dev.idiolect.observation

A signed aggregate over a set of encounter-family records.
Observations decouple ranking from the orchestrator: many
observers publish competing aggregates over the same traces, and
consumers choose whom to trust.

> **Source:** [`lexicons/dev/idiolect/observation.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/observation.json)
> · **Rust:** [`idiolect_records::Observation`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Observation.html)
> · **TS:** `@idiolect-dev/schema/observation`
> · **Fixture:** `idiolect_records::examples::observation`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `observer` | did | yes | DID of the observer publishing this aggregate. |
| `method` | object | yes | `{ name, description?, codeRef?, parameters? }`. The aggregator identity and configuration. |
| `scope` | object | yes | The set of records the observation aggregates over. |
| `output` | unknown | yes | Method-defined payload (counts, scores, diagnostic summaries). |
| `version` | string | yes | Method version. Different versions may produce non-comparable outputs. |
| `basis` | `basis` | no | Grounding when the observer is not the repo owner. |
| `visibility` | `visibility` | yes | Visibility scope. |
| `occurredAt` | datetime | yes | When the observation was published. |

### `method`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string (≤128) | yes | Short method identifier. |
| `description` | string (≤4000 graphemes) | no | Narrative method description. |
| `codeRef` | at-uri | no | Reference to the method's source or specification. |
| `parameters` | unknown | no | Free-form JSON, observer-defined. |

### `scope`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `lenses` | array of `lensRef` | no | Lenses included; empty or omitted means "all". |
| `communities` | array of at-uri | no | Communities whose records are in scope. |
| `encounterKinds` | array of open-enum slugs | no | Encounter kinds weighted in the aggregation. |
| `encounterKindsVocab` | `vocabRef` | no | Vocab the kind slugs resolve against. |
| `window` | `{ from?, until? }` | no | Time window. |

## Field details

### `output`

Deliberately untyped (`unknown`). The shape is determined by
`method`. Common shapes:

- A correction-rate ranking: `[{ lens, rate, sampleCount }]`.
- A quality score: `{ score, ci_low, ci_high }`.
- A structured diagnostic summary: `{ failureModes: [...], hotspots: [...] }`.

A per-statement deliberation tally lives in a separate record
kind, [`deliberationOutcome`](./deliberationOutcome.md), rather
than as an observation output.

A consumer reading an observation must know the method to
interpret the output. The `method.name` plus `version` plus
optional `codeRef` together is what makes the output
interpretable.

### `scope.encounterKinds`

The observer must disclose which encounter kinds it includes or
the observation is uninterpretable. An observation that includes
`adversarial` encounters at the same weight as `invocation-log`
encounters is meaningfully different from one that excludes
adversarial samples; consumers reading the observation rely on
this disclosure to decide whether the result fits their use case.

### `version` versus `occurredAt`

`version` is the method's version. Two observations with the same
`method.name` but different `version`s are not comparable: the
algorithm changed. `occurredAt` is when the observation was
published. Two observations with the same `version` but different
`occurredAt`s are comparable as time-series data.

### `basis`

Most observations are first-party (the repo owner is the
observer). When the observer is a third party (a relay, a cache,
another aggregator that took someone else's output and republished
it), `basis` records the grounds: typically `derivedFromRecord`
pointing at the original observation, with `inferenceRule` set to
the relay or transformation kind.

## Example

```json
{
  "$type": "dev.idiolect.observation",
  "observer": "did:plc:observer.dev",
  "method": {
    "name": "encounter-throughput",
    "version": "1.0.0",
    "parameters": { "windowSeconds": 3600 }
  },
  "scope": {
    "lenses": [
      { "uri": "at://did:plc:lens-author/dev.panproto.schema.lens/3l5" }
    ],
    "encounterKinds": ["invocation-log", "production"],
    "window": {
      "from":  "2026-04-19T00:00:00.000Z",
      "until": "2026-04-19T01:00:00.000Z"
    }
  },
  "output": {
    "total": 1042,
    "byKind": {
      "invocation-log": 940,
      "production":     102
    },
    "byDownstreamResult": {
      "success":   991,
      "corrected":  37,
      "rejected":   12,
      "unknown":     2
    }
  },
  "version":    "1.0.0",
  "visibility": "public-detailed",
  "occurredAt": "2026-04-19T01:00:05.000Z"
}
```

## Why observations and not metrics

A central metrics endpoint cannot:

- Be verified after the fact (the counter is whatever the endpoint
  says it is).
- Be re-folded by an independent party.
- Disagree with itself across observers.

A signed observation can. Two observers running the same fold on
overlapping data will produce records with comparable counts;
consumers can require quorum among trusted observers before
treating an observation as authoritative.

## Concept references

- [Concepts: Observer protocol](../../concepts/observer.md)
- [Concepts: The dev.idiolect.* lexicon family](../../concepts/lexicon-family.md)
- [Guides: Run the observer daemon](../../guide/observer.md)
- [Lexicons: encounter](./encounter.md) · [deliberationOutcome](./deliberationOutcome.md)

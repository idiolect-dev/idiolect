# dev.idiolect.retrospection

A signed annotation of a prior encounter with a delayed finding.
Retrospections address silent-error latency: merges, migrations,
and bitemporal reconciliations often surface failures only after
long delay.

> **Source:** [`lexicons/dev/idiolect/retrospection.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/retrospection.json)
> · **Rust:** [`idiolect_records::Retrospection`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Retrospection.html)
> · **TS:** `@idiolect-dev/schema/retrospection`
> · **Fixture:** `idiolect_records::examples::retrospection`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `encounter` | `encounterRef` | yes | The encounter being retrospected. |
| `finding` | object | yes | `{ kind, kindVocab?, detail, evidence? }`. |
| `latency` | integer (seconds) | no | `detectedAt - encounter.occurredAt`. Precomputed for aggregation. |
| `detectingParty` | did | yes | DID of the party that detected the issue. |
| `confidence` | number ∈ [0, 1] | no | Optional confidence score. |
| `disputedAttribution` | boolean | no | The detecting party's hint that the causal claim may be contested. |
| `basis` | `basis` | no | Structured grounding. |
| `detectedAt` | datetime | yes | When the issue was detected. |
| `occurredAt` | datetime | yes | When this retrospection record was published. |

### `finding`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | open enum | yes | `merge-divergence` / `data-loss` / `reconciliation-mismatch` / `other`. |
| `kindVocab` | `vocabRef` | no | Vocab the kind slug resolves against. |
| `detail` | string (≤8000 graphemes) | yes | Narrative detail. |
| `evidence` | union of `evidenceDivergence` / `evidenceLoss` / `evidenceMismatch` | no | Structured witness for the finding. |

## Why a separate record kind

Encounters are at-record-time. Corrections are short-loop. A
retrospection covers the case where the finding surfaces *after*
the original encounter has been folded into observations and
moved out of the working set:

- A merge that looked correct at write time turns out to have
  silently dropped a field three months later.
- A migration that round-tripped on the test corpus loses
  information on a long-tail input that nobody sampled.
- A bitemporal reconciliation surfaces a divergence between two
  paths that should have converged.

The encounter / correction / observation triple cannot capture
these without distorting their semantics. A retrospection record
is the right shape: it points at the original encounter, names a
delayed finding, and carries the evidence.

## The four finding kinds

| Slug | Evidence shape | What it captures |
| --- | --- | --- |
| `merge-divergence` | `evidenceDivergence` (paths A and B + witness input) | Two translation paths that should have converged but produced different outputs. |
| `data-loss` | `evidenceLoss` (source field path + target schema + witness input) | A source-schema field unrepresented in the target after translation. |
| `reconciliation-mismatch` | `evidenceMismatch` (left and right records + expected equality projection) | Two records that should reconcile under the lens but do not. |
| `other` | (optional) | Catch-all; `evidence` may be omitted, `detail` carries the whole finding. |

The structured evidence variants let downstream consumers match
on the failure mode without parsing the narrative `detail`. An
aggregator that wants to surface "lenses with high data-loss
counts" filters by `finding.kind = data-loss` and folds.

## Field details

### `latency`

Precomputed for aggregation convenience. The value is
`detectedAt - encounter.occurredAt` in seconds. Folds that bucket
findings by latency (e.g. "what's the median time-to-detect for
merge-divergence findings?") read this field directly. Authors
may omit it for `kind: other` findings where latency is not
meaningful.

### `confidence`

Optional, in `[0, 1]`. A finding the detecting party is sure of
omits it. A finding hedged on uncertain evidence sets a value
below 1. Aggregators may weight findings by confidence; consumers
treating findings as ground truth filter for high confidence.

### `disputedAttribution`

A hint the detecting party expects the causal attribution to be
contested. The substrate does not enforce contestation; a
contesting party publishes its own retrospection with
disagreement, or a `dev.idiolect.belief` over the finding. The
flag exists so consumers can flag the finding as
"interpretation pending" rather than treating it as settled.

### `detectingParty` versus the repo signer

`detectingParty` is the party who actually detected the issue.
Most retrospections are first-party: the repo owner is the
detecting party. Some are third-party: a researcher republishing
a finding from a trusted source. `holder` is not a field here
(unlike encounter / belief / correction); `detectingParty` plus
the repo signer carry the relevant attribution.

## Example

```json
{
  "$type": "dev.idiolect.retrospection",
  "encounter": {
    "uri": "at://did:plc:user/dev.idiolect.encounter/3l5"
  },
  "finding": {
    "kind": "data-loss",
    "detail": "Lens dropped the `provenance` array on records with > 100 entries; not detected at write time because all sampled inputs had ≤ 50 entries.",
    "evidence": {
      "$type": "dev.idiolect.defs#evidenceLoss",
      "sourceField": "provenance",
      "targetSchema": {
        "uri": "at://did:plc:schema-author/dev.panproto.schema.schema/v2"
      }
    }
  },
  "latency": 7776000,
  "detectingParty": "did:plc:detector",
  "confidence": 0.95,
  "detectedAt": "2026-07-19T00:00:00.000Z",
  "occurredAt": "2026-07-19T00:01:00.000Z"
}
```

## Concept references

- [Concepts: The dev.idiolect.* lexicon family](../../concepts/lexicon-family.md)
- [Lexicons: defs (`#evidence`)](./defs.md) · [encounter](./encounter.md) · [correction](./correction.md)

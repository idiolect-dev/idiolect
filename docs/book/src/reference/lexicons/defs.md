# dev.idiolect.defs

Shared types for the `dev.idiolect.*` lexicon family. Two kinds of
content live here:

- **Cross-cutting reference shapes** — lens, schema, encounter,
  vocab, and strong-record references; tool identity; visibility.
- **Content-theory types** — purpose, lens property, evidence,
  caveat, basis. Shared across multiple records.

Record-specific combinator trees (condition, eligibility,
constraint, convention) live in their respective record lexicons,
not here.

> **Source:** [`lexicons/dev/idiolect/defs.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/defs.json)
> · **Rust:** [`idiolect_records::generated::defs`](https://docs.rs/idiolect-records/latest/idiolect_records/generated/defs/index.html)
> · **TS:** `@idiolect-dev/schema/defs`

## Reference shapes

### `schemaRef`

Reference to a schema. Either an at-uri or a content hash; at
least one must be present.

| Subfield | Type | Notes |
| --- | --- | --- |
| `uri` | at-uri | AT-URI pointing to a schema record. |
| `cid` | cid-link | Content hash of the schema. |
| `language` | string | Schema-language identifier (`atproto-lexicon`, `postgres-sql`, `protobuf`, `graphql`, `json-schema`). |

### `lensRef`

| Subfield | Type | Notes |
| --- | --- | --- |
| `uri` | at-uri | AT-URI of a lens record. |
| `cid` | cid-link | Content hash of the lens. |
| `direction` | enum | `unidirectional` / `bidirectional`. |

### `encounterRef`

| Subfield | Type | Notes |
| --- | --- | --- |
| `uri` | at-uri (required) | AT-URI of the encounter. |
| `cid` | cid-link | Optional CID for revision pinning. |

### `vocabRef`

| Subfield | Type | Notes |
| --- | --- | --- |
| `uri` | at-uri | AT-URI of a vocab record. |
| `cid` | cid-link | Content hash pinning a specific vocab revision. |

### `strongRecordRef`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `uri` | at-uri | yes | AT-URI of the referenced record. |
| `cid` | cid-link | yes | Content hash. |

Parallel to `com.atproto.repo.strongRef`. Repeated here so the
defs tree is self-contained.

### `tool`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `name` | string | yes | Canonical tool name (`panproto`, `coq`, `tlaplus`, `z3`, `nextest`). |
| `version` | string | yes | Version string. |
| `commit` | string | no | Optional source commit or build identifier. |

### `visibility`

A closed-enum string. Five values:

| Value | Meaning |
| --- | --- |
| `public-detailed` | Full record body published. |
| `public-minimal` | Record published with elided detail (e.g. omits source instance). |
| `public-aggregate-only` | Record consumed only by aggregators; individual reads suppressed. |
| `community-scoped` | Reserved for v1 substrate enforcement; should not be served to parties outside the named community once enforcement lands. |
| `private` | Should not be published at all. |

The substrate does not enforce these today; they are policy
hints.

## Content-theory types

### `use`

The compound "what was done, on what material, for what end, by
which actor" tuple, reused across records whose subject is an
action performed, desired, endorsed, or prohibited.

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `action` | string (≤256) | yes | Action identifier, resolved against `actionVocabulary`. |
| `material` | `materialSpec` | no | What is being acted on. |
| `purpose` | string (≤256) | no | The end the action serves. |
| `actor` | string (≤256) | no | Who performs or benefits. |
| `actionVocabulary` | `vocabRef` | no | Vocab the action slug resolves against. |
| `purposeVocabulary` | `vocabRef` | no | Vocab the purpose slug resolves against. |

### `materialSpec`

| Subfield | Type | Notes |
| --- | --- | --- |
| `scope` | string (≤256) | Community-defined scope (`classroom_materials`, `production_logs`, `scraped_corpus`). |
| `uri` | uri | Optional pointer to a specific dataset. |

### `lensProperty`

A union covering the seven verification kinds. Each verification
record carries one of these as its `property` field.

| Variant | Used by |
| --- | --- |
| `lpRoundtrip` | `kind: roundtrip-test` |
| `lpGenerator` | `kind: property-test` |
| `lpTheorem` | `kind: formal-proof` |
| `lpConformance` | `kind: conformance-test` |
| `lpChecker` | `kind: static-check` |
| `lpConvergence` | `kind: convergence-preserving` |
| `lpCoercionLaw` | `kind: coercion-law` |

#### `lpRoundtrip`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `domain` | string (≤512) | yes | Symbolic description of the input set. |
| `generator` | uri | no | Optional pointer to a generator that enumerates the domain. |

#### `lpGenerator`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `spec` | string (≤2000) | yes | Generator specification (proptest Strategy reference, Hypothesis strategy, QuickCheck Arbitrary). |
| `runner` | string | no | Name of the PBT runtime. |
| `seed` | integer | no | Optional seed for reproducibility. |

#### `lpTheorem`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `statement` | string (≤4000) | yes | The theorem, in the declared `system` syntax. |
| `system` | string | no | Proof system (`coq`, `lean4`, `agda`, `tlaplus`, `z3`). |
| `freeVariables` | array of strings | no | Names of free variables. |

#### `lpConformance`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `standard` | string | yes | Standard identifier (`iso-8601`, `rfc-3339`, `en-pos-v2.1`). |
| `version` | string | yes | Standard version. |
| `clauses` | array of strings | no | Optional subset of the standard's clauses. |

#### `lpChecker`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `checker` | string | yes | Static-checker identifier (`panproto-check`, `clippy`, `tsc-strict`). |
| `ruleset` | string | no | Named ruleset or configuration preset. |
| `version` | string | no | Checker version. |

#### `lpConvergence`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `property` | string (≤1000) | yes | Symbolic name or description of the preserved property. |
| `boundSteps` | integer | no | Optional bound on steps to fixpoint. |

#### `lpCoercionLaw`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `standard` | string (≤256) | yes | Identifier of the coercion-law standard. |
| `version` | string (≤64) | no | Optional version. |
| `violationThreshold` | integer | no | Cap on the violations a runner may report before falsifying. |

### `evidence`

A union of structured witnesses for retrospection findings.

| Variant | Used when finding kind is |
| --- | --- |
| `evidenceDivergence` | `merge-divergence` |
| `evidenceLoss` | `data-loss` |
| `evidenceMismatch` | `reconciliation-mismatch` |

#### `evidenceDivergence`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `pathA` | array of `lensRef` | yes | Lenses composed in path A. |
| `pathB` | array of `lensRef` | yes | Lenses composed in path B. |
| `witnessInput` | cid-link | no | Optional CID where the two paths diverge. |

#### `evidenceLoss`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `sourceField` | string | yes | Dotted path identifying the lost field. |
| `targetSchema` | `schemaRef` | no | Target schema where the loss was observed. |
| `witnessInput` | cid-link | no | Witness input. |

#### `evidenceMismatch`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `leftRecord` | cid-link | no | Left record. |
| `rightRecord` | cid-link | no | Right record. |
| `expectedEqualityOn` | string | no | Dotted path or projection under which equality was expected. |

### `caveat`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `mode` | string | yes | Short failure-mode identifier. |
| `affects` | array of strings | no | Dotted paths or field names. |
| `severity` | enum | no | `info` / `warn` / `error`. |

### `basis`

A union of structured grounds for an attitudinal claim.

| Variant | Use when |
| --- | --- |
| `basisSelfAsserted` | The holder asserts directly. The default when `basis` is omitted. |
| `basisCommunityPolicy` | Grounded in a community's published policy. Carries `community` (at-uri) and optional `policyUri`. |
| `basisExternalSignal` | Grounded in something outside ATProto. Carries `url`, optional `signalType`, optional `description`. |
| `basisDerivedFromRecord` | Grounded in another ATProto record. Carries `source` (`strongRecordRef`) and optional `inferenceRule`. |

`basisSelfAsserted` has no fields; the variant tag itself is the
content. `basisDerivedFromRecord.inferenceRule` is the canonical
hook for declaring how this record derives from another
(`classifier:purpose-v1`, `lens:v1-to-v2`, `aggregation:byte-mean`,
...).

## Concept references

- [Concepts: The dev.idiolect.* lexicon family](../../concepts/lexicon-family.md)
- [Concepts: Records as content-addressed signed data](../../concepts/atproto-records.md)
- The defs are referenced by every other lexicon in the family.

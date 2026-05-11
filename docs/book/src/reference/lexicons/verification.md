# dev.idiolect.verification

A signed assertion of a formal property of a lens. Verifications
are the *formal-channel* primitive: they coexist with the emergent
channel (encounters, corrections, observations) and neither gates
the other. `property` is a structured `lensProperty` (see
[`defs`](./defs.md)) so consumers dispatch on the specific claim:
a `Theorem` for proof checkers, a `GeneratorSpec` for PBT runners,
a `ConformanceStandard` for conformance runners, and so on.

> **Source:** [`lexicons/dev/idiolect/verification.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/verification.json)
> · **Rust:** [`idiolect_records::Verification`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Verification.html)
> · **TS:** `@idiolect-dev/schema/verification`
> · **Fixture:** `idiolect_records::examples::verification`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `lens` | `lensRef` | yes | The lens whose property is being asserted. |
| `kind` | open enum | yes | `roundtrip-test` / `property-test` / `formal-proof` / `conformance-test` / `static-check` / `convergence-preserving` / `coercion-law`. |
| `kindVocab` | `vocabRef` | no | Vocab the kind slug resolves against. |
| `verifier` | did | yes | DID of the party asserting the verification. |
| `tool` | `tool` | yes | Tool identity and version. |
| `property` | union of seven `lensProperty` shapes | yes | Structured statement of what is being asserted. |
| `result` | open enum | yes | `holds` / `falsified` / `inconclusive`. |
| `resultVocab` | `vocabRef` | no | Vocab the result slug resolves against. |
| `counterexample` | `cid-link` | no | For `result: falsified`: minimal counterexample. |
| `dependencies` | array of at-uri | no | Other verifications this one depends on (e.g. a proof assuming a lemma). |
| `proofArtifact` | `cid-link` | no | For `kind: formal-proof`: checkable proof artifact (Coq / Lean / Agda term). |
| `basis` | `basis` | no | Structured grounding when relevant. |
| `occurredAt` | datetime | yes | When the verification was recorded. |

## The seven kinds

Each kind has its own `property` shape, defined in
[`defs#lensProperty`](./defs.md). The kind plus the property
together pin exactly what was verified.

| Kind | Property shape | What the runner does |
| --- | --- | --- |
| `roundtrip-test` | `lpRoundtrip` (domain string + optional generator URI) | Run `put(get(a)) == a` on samples drawn from the domain. |
| `property-test` | `lpGenerator` (spec + runner identifier + seed) | Run an arbitrary boolean predicate over generator samples. |
| `formal-proof` | `lpTheorem` (statement in proof-system syntax + system + free variables) | Check the proof artifact in the named system. |
| `conformance-test` | `lpConformance` (standard identifier + version + clause subset) | Run the standard's conformance suite. |
| `static-check` | `lpChecker` (checker identifier + ruleset + version) | Run the checker against the lens chain. |
| `convergence-preserving` | `lpConvergence` (property + optional step bound) | Verify the property is preserved under repeated application (fixpoint, reconciliation). |
| `coercion-law` | `lpCoercionLaw` (standard + version + violation threshold) | Check panproto's coercion-law gate over samples. |

A consumer reading a verification record dispatches on `kind`,
matches against the embedded `property`, and decides whether the
specific verification meets its needs. A roundtrip-test verification
that covers `domain: "all valid v1 records with bodies ≤ 1024 bytes"`
is meaningfully different from one that covers `domain: "the
training corpus"`; both are valid, neither subsumes the other.

## Field details

### `result`

| Slug | Meaning |
| --- | --- |
| `holds` | The runner did not falsify the property within its budget. |
| `falsified` | The runner found a counterexample. |
| `inconclusive` | The runner ran out of time, the corpus was exhausted, or the proof checker bailed. |

Falsified verifications are first-class records and are how the
community learns a lens is wrong. A consumer that ignores a
falsified verification is making a routing decision; the substrate
records the falsification and lets consumers decide.

### `tool`

The `tool` field records the tool's name, version, and optional
commit. Consumers reading a verification can decide whether to
trust the tool: `panproto-check@0.39.0` plus a known-good commit
is a different signal from a tool the consumer has never heard of.

### `verifier`

The party signing the verification. The PDS commit ties the
verifier's signature to the record. Consumers maintain their own
trust list of verifiers (per kind) and ignore verifications signed
by unknown parties.

### `dependencies`

A formal proof may depend on lemmas. A property test may depend on
a generator that itself was verified. The `dependencies` array
lists at-uris of those upstream verifications. A consumer auditing
the verification follows the chain, confirms each dependency is
itself trustworthy, and adopts the result only when the entire
chain checks out.

### `proofArtifact`

For `kind: formal-proof`: a content-addressed reference to the
checkable proof. An orchestrator that has the proof checker
configured can mechanically verify; one that does not takes the
verifier's signed assertion on trust. The proof artifact is the
escape hatch from "trust the verifier" to "check the proof
yourself".

### `counterexample`

For `result: falsified`: a content-addressed reference to a
minimal counterexample (often the smallest sample the runner
found that violated the property). Consumers can fetch the
counterexample, replay the lens against it, and confirm the
falsification independently.

## Example

```json
{
  "$type": "dev.idiolect.verification",
  "lens": { "uri": "at://did:plc:lens-author/dev.panproto.schema.lens/3l5" },
  "kind": "roundtrip-test",
  "verifier": "did:plc:verifier",
  "tool": {
    "name": "panproto-check",
    "version": "0.39.0",
    "commit": "02158abb"
  },
  "property": {
    "$type": "dev.idiolect.defs#lpRoundtrip",
    "domain": "all valid v1 records with bodies ≤ 1024 bytes",
    "generator": "https://corpus.example/v1-1k-sample.zip"
  },
  "result": "holds",
  "occurredAt": "2026-04-19T00:00:00.000Z"
}
```

## Verifications and recommendations

A `dev.idiolect.recommendation` lists `requiredVerifications`. A
consumer adopting the recommendation queries the verifier registry
for verification records on each lens, accepts the records signed
by trusted verifiers with `result: "holds"`, and confirms each
required verification is covered. A recommendation with required
verifications that nobody has published is a community asking for
work to be done; a `dev.idiolect.bounty` is the canonical way to
ask for it.

## Concept references

- [Concepts: Lens semantics and laws](../../concepts/lens-laws.md)
- [Concepts: Lexicon evolution policy](../../concepts/lexicon-evolution.md)
- [Tutorial: Run a verification](../../tutorial/04-verify.md)
- [Guides: Author a verification runner](../../guide/verify.md)
- [Lexicons: defs (`#lensProperty`, `#tool`)](./defs.md) · [recommendation](./recommendation.md) · [bounty](./bounty.md)

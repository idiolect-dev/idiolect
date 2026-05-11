# Idiolect, dialect, language

The project name comes from a deliberate analogy with the
linguistic terms.

- An **idiolect** is one party's choice of schemas, lenses, and
  conventions. It is what a single PDS (or a single application
  developer) actually publishes.
- A **dialect** is the bundle of idiolects a community treats as
  canonical. It carries a list of preferred NSIDs, preferred
  lenses, endorsed vocabularies, and deprecations.
- A **language** is the federated substrate over which idiolects
  and dialects meet. There is no central registry, no single
  authority, and no global schema; the substrate is ATProto plus
  the shipped lexicons.

The three layers correspond to three things in the runtime:

| Linguistic | Runtime artifact | Lexicon |
| --- | --- | --- |
| Idiolect | A single party's records on a PDS | (any record kind) |
| Dialect | A bundle published by a community | `dev.idiolect.dialect` |
| Language | The federated network of all parties | (the whole `dev.idiolect.*` family) |

## Why this frame

A large protocol benefits from a model where parties can disagree
gracefully. Two communities can run incompatible schemas and the
substrate accommodates them; a third community can publish a lens
between the two and the network can route translations. The frame
does not promise a single canonical schema; it ships the
machinery for reasoning about plural canonicities.

The properties this gets you:

- **No global arbiter.** There is no place to file a grievance and
  no place to extract rent. A community can fork a dialect, ship
  its own, and let consumers pick.
- **Cheap experimentation.** Adding an idiolect is a record edit.
  A community can try a new schema, see who adopts, and either
  roll it into a dialect or abandon it.
- **Auditable convergence.** When two communities adopt the same
  lens, the encounter / observation / recommendation records carry
  enough structure to make the convergence visible without a
  central monitor.

## Failure modes the frame admits

The frame does not promise that the network converges, that
disagreements always resolve, or that bad actors cannot publish
records. It admits:

- **Silent fragmentation.** Two communities ship near-identical
  schemas under different NSIDs. Consumers see two records where
  there should be one. The signal is the lens-recommendation
  density between the two; if no community publishes a lens
  between them, the fragmentation is permanent.
- **Adversarial publishing.** A bad actor publishes a vocab that
  shadows a canonical slug with a different meaning. The
  containment is at the consumer's policy: prefer vocabs from
  recognised communities, treat unknown vocabs as unknown.
- **Dialect drift.** A community changes its dialect record
  without coordinating with downstream consumers. Old records keep
  validating; new lens choices route differently. The signal is
  the deprecation list and the lexicon-evolution gate.

The runtime ships primitives for each of these (recommendation,
deliberation, lens classification) but no policy. Policy lives in
the consumer.

## What is not promised

The frame does not promise a unified ontology, a global
identifier scheme, or a single authoritative type for any record
kind. It does promise that two parties using the same NSID see
records under that NSID with the same wire shape, that lenses
between schemas obey their stated laws, and that the lexicon
itself does not change shape without an auditable migration.

The chapters that follow cover what each of those promises means
in practice.

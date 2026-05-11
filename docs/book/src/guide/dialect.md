# Bundle records into a dialect

A `dev.idiolect.dialect` record is a community-published bundle
of "these are the schemas, lenses, and conventions we treat as
canonical". Consumers wanting the dialect's view of the world
fetch one record and follow the references.

The shape is documented in the
[lexicon reference](../reference/lexicons/dialect.md). The
fields most consumers care about:

- `idiolects` — schemas the community treats as canonical.
- `preferredLenses` — translations the community prefers.
- `deprecations` — entries that were once part of the dialect
  with replacement pointers.
- `version` and `previousVersion` — the dialect's revision
  chain.

## Author the bundle

The shortest path is to construct the typed record directly:

```rust
use idiolect_records::{Dialect, AtUri, Datetime};
// Plus the inline `defs` types: SchemaRef, LensRef, Deprecation.

let dialect = Dialect {
    owning_community: AtUri::parse(
        "at://did:plc:tutorial.dev/dev.idiolect.community/canonical",
    )?,
    name: "tutorial canonical".into(),
    description: Some("...".into()),
    idiolects: vec![/* schemaRef entries */],
    preferred_lenses: vec![/* lensRef entries */],
    deprecations: vec![],
    version: Some("1.0.0".into()),
    previous_version: None,
    created_at: Datetime::parse("2026-04-19T00:00:00.000Z").unwrap(),
};
```

Construct the record, then publish it via
`idiolect_lens::RecordPublisher::create`.

## What it does for consumers

Consumers reading a dialect get three things:

- A canonical NSID list. A consumer that has resolved a dialect
  knows which schemas the community treats as canonical and can
  filter incoming records accordingly.
- A vocabulary registry by reference. Open-enum slugs in records
  whose author belongs to the community resolve against the
  dialect's listed vocabularies (consumers read the vocabularies
  themselves through their `*Vocab` siblings, not through the
  dialect).
- An audit trail of deprecations. A consumer that sees a
  `Deprecation` entry can keep reading the deprecated NSID for
  some grace period and route to `replacement` afterwards.

## Use a dialect at runtime

There is no shipped `DialectClient` helper. Consumers fetch the
dialect record (via `idiolect_lens::PdsResolver`'s
`Resolver::resolve` or any other resolver) and walk the typed
fields directly. A small wrapper layer is the right shape if a
consumer wants a `dialect.preferred_lens_for(nsid)` style
accessor; the substrate ships only the record.

## Multiple dialects

Two communities can publish disjoint, overlapping, or
contradictory dialects. The substrate treats them as opinions;
nothing in the protocol prefers one over another. Consumers
pick a resolution policy in their own code:

- *first-match* — pick the first dialect listed in the
  consumer's config.
- *quorum* — accept a translation when $k$ of $n$ trusted
  dialects endorse the same lens path.
- *merge* — union the entries; on collision, fall back to a
  configured tie-breaker.

The substrate does not ship trait or implementation for any of
these: dialect resolution is a consumer decision, and the right
shape varies by deployment.

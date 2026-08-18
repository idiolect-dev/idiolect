# Bundle records into a dialect

A `dev.idiolect.dialect` record publishes a community's selected
[schemas and lenses](../glossary.md#dialect "A community-selected bundle of schemas and translations")
as one versioned bundle. A consumer fetches the record and follows
only the references its local policy accepts.

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

```text
use idiolect_records::{AtUri, Datetime, Dialect};

let dialect = Dialect {
    owning_community: AtUri::parse(
        "at://did:plc:tutorial.dev/dev.idiolect.community/canonical",
    )?,
    name: "tutorial canonical".into(),
    description: Some("...".into()),
    idiolects: Some(vec![/* SchemaRef values */]),
    preferred_lenses: Some(vec![/* LensRef values */]),
    deprecations: None,
    version: Some("1.0.0".into()),
    previous_version: None,
    created_at: Datetime::parse("2026-04-19T00:00:00.000Z")?,
};
```

Construct the record, then publish it via
`idiolect_lens::RecordPublisher::create`.

## What it does for consumers

Consumers reading a dialect get three independent signals:

- A canonical NSID list. A consumer that has resolved a dialect
  knows which schemas the community treats as canonical and can
  filter incoming records accordingly.
- A lens preference list. `preferredLenses` records which
  translations the community recommends; it does not override a
  consumer's trust policy.
- An audit trail of deprecations. A consumer that sees a
  `Deprecation` entry can keep reading the deprecated NSID for
  some grace period and route to `replacement` afterwards.

## Use a dialect at runtime

There is no shipped `DialectClient`. Fetch the record through a typed
record client and inspect its fields directly. The
`idiolect_lens::Resolver` trait resolves lens records only; it does
not resolve `dev.idiolect.dialect` records.

## Multiple dialects

Two communities may publish disjoint, overlapping, or contradictory
dialects. The protocol assigns no global priority, so consumers pick
a resolution policy in application code:

- *first-match* — pick the first dialect listed in the
  consumer's config.
- *quorum* — accept a translation when $k$ of $n$ trusted
  dialects endorse the same lens path.
- *merge* — union the entries; on collision, fall back to a
  configured tie-breaker.

The runtime ships no trait or implementation for any of
these: dialect resolution is a consumer decision, and the right
shape varies by deployment.

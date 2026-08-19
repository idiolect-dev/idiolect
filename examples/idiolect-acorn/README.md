# idiolect-acorn

This example bridges idiolect with Blacksky's
[Acorn](https://acorn.blacksky.community/) community infrastructure. It shows
how a downstream project can generate a record family from another
community's ATProto lexicons, compose that family with `IdiolectFamily`, and
translate its records through published lenses.

The directory is organized as a possible standalone package: vendored
lexicons, generated-family inputs, authored bridge records, and lens
specifications remain separate.

## Contents

- `lexicons/community/blacksky/{feed,assembly}/*.json`: a snapshot of the
  Blacksky lexicons. The upstream sources are `blacksky-algorithms/atproto`
  for feeds and `blacksky-algorithms/assembly.blacksky.community` for
  assemblies.
- `data/vocabs/blacksky-vote-stances.json`: a bridge vocabulary mapping
  Blacksky's integer-encoded votes to the canonical `vote-stances-v1` slugs
  through `equivalent_to` edges.
- `data/bridge-records/blacksky-community.json`: a
  `dev.idiolect.community` record. Its `recordHosting=community-hosted` and
  `appviewEndpoint` fields state that records live on the AppView rather than
  member PDSes.
- `data/bridge-records/blacksky-dialect.json`: a `dev.idiolect.dialect`
  listing Blacksky NSIDs and bridge lenses.
- `data/bridge-records/blacksky-appview-adapter.json`: a
  `dev.idiolect.adapter` describing the Blacksky AppView's HTTP surface.
- `lenses/*.ncl`: bridge-lens specifications:
  - `conversation-to-deliberation.ncl` (Iso after vocab default fill-in)
  - `statement-to-deliberation-statement.ncl` (Injection)
  - `vote-to-deliberation-vote.ncl` (Projection on the reverse direction)

## Integration

1. Generate `BlackskyFamily` from the vendored lexicons with
   `idiolect-codegen`, following the `layers-pub` fixture pattern.
2. Use `OrFamily<IdiolectFamily, BlackskyFamily>` as the `RecordFamily`
   parameter of `idiolect-indexer` to consume both namespaces from one
   firehose.
3. At query time, route Blacksky records through the lens chain in `lenses/`
   to read them as `dev.idiolect.deliberation*`.
4. When Blacksky revises its lexicons, run
   [`scripts/lexicon-evolve.sh`](../../scripts/lexicon-evolve.sh) against the
   new snapshot. The
   [lexicon-evolution policy](../../docs/book/src/concepts/lexicon-evolution.md)
   classifies the change before the bridge lens is republished.

## Subsumption summary

| Acorn record | Idiolect target | Optic class |
|---|---|---|
| `community.blacksky.assembly.conversation` | `dev.idiolect.deliberation` | Iso (after default fill-in) |
| `community.blacksky.assembly.statement` | `dev.idiolect.deliberationStatement` | Injection |
| `community.blacksky.assembly.vote` | `dev.idiolect.deliberationVote` | Projection (reverse drops idiolect's optional weight/rationale to complement) |
| `community.blacksky.feed.post` | identity translation through the AppView adapter | Iso |

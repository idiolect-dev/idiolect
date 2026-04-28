# idiolect-acorn

Bridge between idiolect and Blacksky's [Acorn](https://acorn.blacksky.community/) community infrastructure. Demonstrates how a downstream project lifts another community's ATProto lexicons into idiolect's framework via the standing lexicon-evolution policy.

This directory is the source-of-truth for an eventual standalone repo. Layout mirrors what would ship as `crates/blacksky-records` + authored vocab/community/dialect/adapter/lens records.

## Contents

- `lexicons/community/blacksky/{feed,assembly}/*.json` — vendored Blacksky lexicons. Snapshot only; the upstream is `blacksky-algorithms/atproto` (feed) and `blacksky-algorithms/assembly.blacksky.community` (assembly).
- `data/vocabs/blacksky-vote-stances.json` — bridge vocabulary mapping Blacksky's integer-encoded vote values onto idiolect's canonical `vote-stances-v1` slugs via `equivalent_to` edges.
- `data/bridge-records/blacksky-community.json` — `dev.idiolect.community` record that makes Blacksky a first-class participant in the idiolect catalog. Uses the new `recordHosting=community-hosted` + `appviewEndpoint` fields to disclose that records live on the AppView rather than member PDSes.
- `data/bridge-records/blacksky-dialect.json` — `dev.idiolect.dialect` listing Blacksky NSIDs and the bridging lenses.
- `data/bridge-records/blacksky-appview-adapter.json` — `dev.idiolect.adapter` exposing the Blacksky AppView's HTTP surface.
- `lenses/*.ncl` — bridging lens specifications:
  - `conversation-to-deliberation.ncl` (Iso after vocab default fill-in)
  - `statement-to-deliberation-statement.ncl` (Injection)
  - `vote-to-deliberation-vote.ncl` (Projection on the reverse direction)

## How a consumer wires this up

1. Codegen `BlackskyFamily` from the vendored lexicons via `idiolect-codegen` (same pattern as `layers-pub`).
2. Use `OrFamily<IdiolectFamily, BlackskyFamily>` as the `RecordFamily` parameter of `idiolect-indexer` to consume both namespaces from a single firehose.
3. At query time, route Blacksky records through the lens chain in `lenses/` to read them as `dev.idiolect.deliberation*`.
4. When Blacksky revises their lexicons, re-run `scripts/lexicon-evolve.sh` from the parent idiolect repo against the new vendored copy. The §7 policy classifies the change and gates re-publish of the bridge lens.

## Subsumption summary

| Acorn record | Idiolect target | Optic class |
|---|---|---|
| `community.blacksky.assembly.conversation` | `dev.idiolect.deliberation` | Iso (after default fill-in) |
| `community.blacksky.assembly.statement` | `dev.idiolect.deliberationStatement` | Injection |
| `community.blacksky.assembly.vote` | `dev.idiolect.deliberationVote` | Projection (reverse drops idiolect's optional weight/rationale to complement) |
| `community.blacksky.feed.post` | identity-lensed; ride the AppView adapter | Iso |

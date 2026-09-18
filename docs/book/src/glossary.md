# Glossary

This glossary gives the short definitions used throughout the book. Entries for
named standards link to their normative specification or the standard's official
project documentation.

## AT Protocol

[AT Protocol](https://atproto.com/specs/atp), usually shortened to **ATProto** or
**atproto**, is the federated protocol on which idiolect publishes records and
services.

## AT URI

An [AT URI](https://atproto.com/specs/at-uri-scheme) identifies an ATProto
repository or record with an authority, an optional collection, and an optional
record key. An AT URI is mutable unless paired with a CID.

## Catalog

The **catalog** is the orchestrator's indexed store of typed idiolect records.
Queries evaluate over this local read model; the catalog is not a global
registry or an authority over the records it contains.

## Change packet

A **change packet** is the local, durable account of one proposed definition
change. It keeps the old and new schema references, Panproto consequence
analysis, governance reviews, verification evidence, and release state together.

## CID

A [Content Identifier (CID)](https://github.com/multiformats/cid) is a
self-describing content address built from a cryptographic hash and format
metadata. ATProto uses CIDs for links whose target bytes must be verifiable.

## Complement

A **complement** stores source information that a lens cannot reconstruct from
its view alone. A `put` operation uses that information when it propagates an
edited view back to the source schema.

## Community control plane

The **community control plane** is Idiolect's portable set of local artifacts
and published records for governing changes, cutting releases, operating
migrations, relating peer communities, and exiting one tool without losing
history.

## Community release

A **community release** is an immutable signed cut of a community's accepted
definition changes, artifacts, and declared federation dependencies.

## Consequence layer

The **consequence layer** translates a structural schema diff into likely
effects on participants, migrations, and interoperability. It remains evidence
for deliberation rather than an automatic governance decision.

## DID

A [decentralized identifier
(DID)](https://www.w3.org/TR/did-core/) is a URI that identifies an entity and
resolves according to a DID method. ATProto accounts use DIDs as stable account
identifiers even when handles or hosting providers change.

## Dialect

A **dialect** is a community's published bundle of preferred schemas, lenses,
and deprecations. It records a collective convention without making that
convention global.

## DPoP

[Demonstrating Proof of Possession
(DPoP)](https://www.rfc-editor.org/rfc/rfc9449.html) binds an OAuth token to a
client-held key. The ATProto OAuth profile requires DPoP with server-issued
nonces.

## Encounter

An **encounter** is a signed record of one lens invocation, including the lens,
the source and target schemas, the purpose, and the observed outcome.

## Firehose

An ATProto **firehose** is the repository event stream described by the
[ATProto synchronization specification](https://atproto.com/specs/sync). PDSs
emit events for hosted accounts; relays may aggregate many upstream streams.

## Federation

A **federation** record declares how one community relates to a peer and, when
known, which explicit lenses or other mappings connect their definitions. A
relationship without a mapping does not claim semantic equivalence.

## Idiolect

An **idiolect** is one party's choice of schemas, lenses, vocabularies, and
conventions. The term names the local unit of variation that idiolect preserves.

## Language

A **language**, in this book's three-level model, is the federated substrate on
which idiolects and dialects interact. It does not denote one centrally managed
schema catalog.

## Lens

A **lens** is a bidirectional translation between a source schema and a target
schema. Its `get` operation produces a target view; its `put` operation
propagates edits to that view back to the source, subject to stated lens laws.

## Lexicon

[Lexicon](https://atproto.com/specs/lexicon) is ATProto's schema language for
records, XRPC endpoints, and event-stream messages. Each Lexicon file is named by
an NSID.

## Lexicon family

<a id="record-family"></a>

A **Lexicon family**, also called a **record family** in generated APIs, is the
set of related Lexicon records emitted and versioned together under one namespace
policy.

## Migration run

A **migration run** is the durable operational state for applying one governed
change. Its checkpoints, counters, and bounded failure samples allow work to
resume and let observers distinguish progress from completion.

## NSID

A [Namespaced Identifier (NSID)](https://atproto.com/specs/nsid) is a global
semantic identifier whose authority appears in reverse-domain order, followed by
a final name segment. `dev.idiolect.belief` is an NSID.

## Observation

An **observation** is a signed aggregate computed from a stated scope of
encounters by a named observer method. It is evidence produced by a method, not
the raw trace of a single translation.

## Observer

An **observer** consumes encounter or record data, applies a declared method, and
publishes observations. The method and its inputs remain inspectable so consumers
can evaluate the result.

## Open enum

An **open enum** accepts a known set of values while preserving unknown strings.
This representation lets older consumers retain values added by later producers.

## Ordinary exit test

The **ordinary exit test** asks whether a community can copy its definitions,
policy, change history, releases, migration state, and optional keys into a
portable verified export without depending on a running Idiolect service.

## OAuth

[OAuth](https://atproto.com/specs/oauth) is the authorization framework ATProto
clients use to obtain scoped access to PDS resources. The ATProto profile
combines OAuth with PKCE, PAR, and DPoP requirements.

## Panproto

[Panproto](https://github.com/panproto/panproto) supplies the schema graphs,
protocols, protolenses, lens runtime, compatibility checks, and parsing machinery
that idiolect uses. This book targets Panproto 0.74.4.

## PDS

A [Personal Data Server
(PDS)](https://atproto.com/specs/account) hosts ATProto accounts, repositories,
authentication, and blobs. An account may migrate between PDS providers without
changing its DID.

## Protocol

A **protocol**, in Panproto's formal model, supplies operations and laws against
which schemas and lenses can be interpreted. This use is narrower than “network
protocol.”

## Protolens

A **protolens** is Panproto's schema-level description of a bidirectional
transformation before that description is instantiated as a runtime lens.

## Recommendation

A **recommendation** is a community's signed endorsement of particular schemas or
lenses, optionally conditioned on verifications. It records social authority and
does not itself prove a mechanical property.

## Record

An ATProto **record** is a typed data object stored in an account repository. Its
`$type` value names the governing Lexicon schema, and its collection is normally
the same NSID.

## Repository

An ATProto **repository** is an account's signed, content-addressed collection
of records. A PDS stores the repository and distributes its commits through the
ATProto synchronization protocol.

## Schema

A **schema** describes the admissible structure of a record. Idiolect reads
ATProto Lexicons into Panproto schema graphs for validation, comparison, and lens
execution.

## Strong reference

An ATProto **strong reference** pairs an AT URI with the target record's CID. The
URI locates the record and the CID identifies the exact content.

## Theory

A **theory** is a named collection of formal structure and constraints used to
compose Panproto schemas. Idiolect's theory files state reusable semantic
components rather than runtime records.

## Verification

A **verification** is a signed report that a named runner checked a particular
property of a lens and obtained `holds`, `falsified`, or `inconclusive`.

## Vocabulary

A **vocabulary** is a governed graph of concepts and relations that record fields
may reference. Values remain ordinary identifiers; the vocabulary supplies their
machine-readable relations and provenance.

## XRPC

[Lexicon RPC (XRPC)](https://atproto.com/specs/xrpc) is ATProto's convention for
HTTP query and procedure endpoints named by NSIDs.

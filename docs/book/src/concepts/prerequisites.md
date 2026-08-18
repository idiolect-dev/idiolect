# What you need first

idiolect depends on ATProto for publication and on panproto for schema
translation. Readers need three ATProto objects and one panproto object. The
definitions below are sufficient for the rest of the Concepts chapters.

## ATProto repositories and records

An ATProto account controls a public
[repository](../glossary.md#repository "An account-scoped, verifiable collection of ATProto records").
A [record](../glossary.md#record "A typed data object stored at a repository path")
occupies a path of the form `collection/rkey`; an
[AT-URI](../glossary.md#at-uri "A mutable address for an ATProto record") adds the
account DID:

```text
at://did:plc:example/dev.idiolect.verification/3kexample
```

The repository is a content-addressed Merkle Search Tree whose leaves point to
record CIDs. The repository *commit* is signed; an individual record is not a
stand-alone signed document. A proof chain can connect a record CID to that
signed commit. This distinction, specified in the official
[ATProto repository format](https://atproto.com/specs/repository), must be
accounted for when evaluating provenance claims.

## Lexicons and NSIDs

A [Lexicon](../glossary.md#lexicon "ATProto's schema language for records and XRPC")
is an ATProto schema document. Its
[NSID](../glossary.md#nsid "A reverse-domain identifier for an ATProto schema or method")
names a record collection or XRPC method; `dev.idiolect.recommendation` is one
such collection. The [Lexicon specification](https://atproto.com/specs/lexicon)
defines the record, object, reference, union, and scalar forms used throughout
this repository.

Independent publishers may define different NSIDs for similar data. Lexicon
validation can determine whether a value has the declared shape, but it cannot
determine that two independently named shapes describe the same thing. idiolect
addresses that second problem.

## PDSes and event streams

A [personal data server (PDS)](../glossary.md#pds "The service that hosts an account's authoritative ATProto repository")
hosts an account's authoritative repository. Network consumers usually learn about
record changes through a synchronization stream or a derived transport such as
Jetstream or tap. The book uses *event stream* for the abstraction and
*firehose* when referring to the network-wide ATProto stream specifically.

The current indexer accepts a generic `EventStream`. Concrete adapters cover
tap and Jetstream; thus a conceptual statement about an event fold does not
imply that every process connects directly to a PDS.

## Panproto lenses

A [lens](../glossary.md#lens "A bidirectional schema translation with explicit round-trip obligations")
relates a source schema to a target schema. Its forward operation, `get`,
produces a target view and a
[complement](../glossary.md#complement "State retained so a backward lens operation can reconstruct its source")
containing source information that the view did not retain. Its backward
operation, `put`, combines a target view with that complement to reconstruct a
source value.

idiolect uses panproto 0.70.1 to instantiate and run these lenses. No category
theory is assumed: [Lens semantics and laws](./lens-laws.md) introduces the
notation before using it, while the [panproto book](https://panproto.dev/book/)
provides optional depth.

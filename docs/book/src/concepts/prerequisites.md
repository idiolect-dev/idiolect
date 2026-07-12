# What you need first

idiolect's two dependencies are prerequisites of different kinds.

The ATProto dependency is deep. idiolect is founded on ATProto's
ideas (self-owned repositories, signed content-addressed records,
schemas anyone can publish, federation without a central arbiter),
and its design decisions assume that model; without it, they will
read as arbitrary. If
you have not worked with ATProto, plan to spend time with the
[ATProto documentation](https://atproto.com/guides/overview); this
book does not substitute for it. This page states the three
ATProto ideas the book leans on hardest. If they read as familiar,
continue; if not, follow the links first.

The panproto dependency, by contrast, is shallow for readers. The runtime uses
panproto heavily, but this book requires exactly one concept from
it (the lens), stated in the last section below. You do not need
to read the panproto book, and no category theory appears anywhere
in this book.

## From ATProto: record

A *record* is a piece of JSON that lives in a repository controlled
by one account (identified by a DID). Records are signed and
content-addressed, so anyone can verify who published a record and
that it has not been altered. Every record has an address of the
form `at://did/collection/key`. Everything idiolect publishes
(lenses, verifications, recommendations, dialects) is a record.
The runtime consequences of this choice are the subject of
[Records as content-addressed signed data](./atproto-records.md).

## From ATProto: lexicon

A *lexicon* is ATProto's schema language: a JSON document that
declares the shape of a record type and names it with an *NSID*, a
reverse-domain identifier such as `app.bsky.feed.post` or
`dev.idiolect.recommendation`. Anyone who controls a domain can
publish lexicons under it. This is the source of the problem
idiolect addresses: independent parties can, and do, publish
different lexicons for the same kind of thing. The lexicons this
project ships are catalogued in [the lexicon
family](./lexicon-family.md).

## From ATProto: PDS and firehose

A *PDS* (personal data server) hosts repositories. The *firehose*
is the network-wide event stream of record creations, updates, and
deletions that consumers subscribe to. The load-bearing facts for
this book: records live on many independent servers, and there is
a stream you can watch to see all of them. idiolect's indexer,
orchestrator, and observer are consumers of that stream; the
[tutorial](../tutorial/01-install.md) has you fetch records
without running any of them.

## From panproto: lens

A *lens* is a two-way converter between two schemas. The forward
direction (`get`) translates a record from the source shape to the
target shape and sets aside whatever the target shape cannot
express; the set-aside part is called the *complement*. The
backward direction (`put`) uses the complement to reconstruct the
original exactly. A lens is not trusted on its author's word: it
obeys stated round-trip laws, and the laws can be checked
mechanically against real records. Nothing more from panproto is
assumed in this book. The laws, the classification of lenses by
what they promise, and the symmetric variant used for bridging two
communities are in [Lens semantics and laws](./lens-laws.md), and
that chapter restates each of them before use.

## Where to deepen

The [ATProto documentation](https://atproto.com/guides/overview)
is the real prerequisite if the three paragraphs above were not
already familiar. The [panproto book](https://panproto.dev/book/)
is optional depth on the schema and lens machinery. [Why idiolect
exists](./why-idiolect.md) explains why the project builds on
both.

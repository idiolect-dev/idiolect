# Why idiolect exists

idiolect addresses the **private-converter problem (PCP)**: independently
written schema converters tend to remain inside the applications that need
them, so later consumers cannot inspect, reuse, or evaluate the translation
knowledge they contain.

## Two event schemas

Suppose two communities build event-planning applications on ATProto. One
publishes `garden.seedling.event`, with `title`, `startsAt`, and a free-text
`where` field. The other publishes `club.lantern.gathering`, with `name`,
`beginsAt`, and a structured `venue` object. Each schema fits its application.

A calendar aggregator that consumes both collections now has to answer three
questions: which fields correspond, what happens to information that exists on
only one side, and whose answer should be trusted? ATProto supplies publication,
identity, and repository proofs. It does not supply the correspondence between
these two Lexicons.

## Why local fixes accumulate poorly

The aggregator can hard-code two adapters. That solves its immediate problem,
but another consumer must repeat the work, and neither consumer has a common
object on which to publish tests or corrections. Supporting one schema only
avoids translation by excluding data. Asking both communities to adopt a third
schema may be appropriate in some settings, though it shifts the disagreement
to standard selection.

These responses differ operationally, but they leave the PCP intact: knowledge
of the relationship is either private or absent.

## The publication move

idiolect makes a translation publishable. A panproto
[lens](../glossary.md#lens "A bidirectional schema translation with explicit round-trip obligations")
record names its source and target schemas and carries a schema-parameterized
translation body. A consumer can resolve that record, instantiate it under the
ATProto protocol, and apply it to a source value. If `get` drops `where` while
constructing `venue`, its complement retains the discarded state for `put`.

Publication creates a shared object, not automatic trust. The record family
thus separates four kinds of claim:

1. A `dev.idiolect.verification` reports the result of running a named check on
   a lens.
2. A `dev.idiolect.recommendation` endorses a lens path under stated conditions
   and caveats.
3. A `dev.idiolect.encounter` records an invocation; a correction or later
   observation can qualify that evidence.
4. A `dev.idiolect.dialect` bundles the schema references that constitute a
   community's idiolect set, along with preferred lenses and deprecations.

This separation is the **evidence split (ES)**. A lens body says how to
translate; a verification says what a particular runner observed; a
recommendation says who advises using the lens and when. Under the ES, no one
record stands in for the others.

## What the model can promise

The runtime can preserve unfamiliar open-enum slugs, reject values that its
typed decoders cannot parse, apply a resolved lens, and publish the resulting
evidence records. A corpus-backed verification may show that a round-trip law
held for the tested corpus. It does not prove that the law holds for every
possible record, and a signed repository commit proves authorship and integrity
rather than semantic correctness.

idiolect consequently does not promise a global schema, universal convergence,
or trustworthy publishers. It provides objects over which communities can make
translation, evidence, and policy disagreements explicit. The
[idiolect/dialect/language frame](./idiolect-dialect-language.md) names the three
levels at which those disagreements arise.

## From the example to the runtime

| Question from the example | Runtime object |
| --- | --- |
| How do the two event shapes correspond? | `dev.panproto.schema.lens` and [lens semantics](./lens-laws.md) |
| Did a check find a counterexample? | `dev.idiolect.verification` |
| Who recommends this path, and under which conditions? | `dev.idiolect.recommendation` |
| What happened when a consumer used it? | `dev.idiolect.encounter`, `correction`, and `observation` |
| Which choices does a community currently prefer? | `dev.idiolect.dialect` |
| How can a community debate a choice before adopting it? | The [deliberation records](./deliberation.md) |

For the operational loop, continue with the [tutorial](../tutorial/index.md).
For the conceptual division of responsibility, continue with
[Idiolect, dialect, language](./idiolect-dialect-language.md).

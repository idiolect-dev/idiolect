# Why idiolect exists

This chapter states the problem the project addresses and how the
machinery responds to it, without formalism. The rest of the
Concepts section assumes it.

## Two apps, one firehose

Suppose two communities build event-planning apps on ATProto.

One community ships a lexicon called `garden.seedling.event`. Its
records have a `title`, a `startsAt`, and a free-text `where`
field. The other community ships `club.lantern.gathering`. Its
records have a `name`, a `beginsAt`, and a structured `venue`
object. Neither community did anything wrong. Each schema fits its
app, each shipped when it was ready, and neither had any reason to
wait for the other.

Now a third developer builds a calendar aggregator, whose premise
is that every event on the network appears in it. The
aggregator subscribes to the firehose and immediately faces the
problem this project exists for: the network has two shapes for
the same kind of thing, and nothing on the network says how they
relate.

## The options that do not scale

The aggregator developer has three obvious moves, and each one
fails in a characteristic way.

**Hardcode both.** Write an internal converter from each lexicon
into the aggregator's own model. This works for the aggregator,
for these two lexicons, for now. But the work is private: the next
consumer rewrites the same converters, every schema revision
breaks them silently, and the effort grows with the product of
consumers and lexicons.

**Pick a winner.** Support `garden.seedling.event` and ignore the
other. Half the network's events vanish, and the aggregator has
quietly become a standards body, the thing a federated network
was supposed to avoid.

**Wait for a standard.** Petition both communities to converge on
a shared lexicon. Sometimes this works. Usually it produces a
third lexicon.

The pattern in all three failures is the same: the *knowledge of
how two schemas relate* stays private, so the network cannot
accumulate it.

## The move idiolect makes

idiolect's answer is to make that knowledge a first-class,
published, verifiable object. Anyone (either community, the
aggregator developer, or an uninvolved third party) can publish a
*lens*: a two-way translation between the two lexicons, itself a
signed record on the network. The aggregator resolves the lens and
applies it instead of hand-writing a converter. So does every
consumer after it.

A published translation raises an obvious question: why trust it?
idiolect's answer is more records. A *verification* record is a
machine-checked run of the lens against real data: evidence, not
prose. A *recommendation* record is a party endorsing a lens for a
purpose. A *dialect* record is a community bundling its preferred
lexicons and lenses so consumers can adopt its choices wholesale.
None of these requires a central registry. Trust accumulates the
way it does between people: through use, attestation, and
reputation, all of it auditable.

The linguistic framing follows from this picture. Each party's
schema choices are its *idiolect*;
a community's endorsed bundle is a *dialect*; the federated
substrate where they meet and negotiate is the *language*. The
[next chapter](./idiolect-dialect-language.md) develops the frame
and, importantly, the failure modes it admits.

## The example, mapped to the machinery

Each element of the example above corresponds to one lexicon or
concept in the runtime:

| In the example | Machinery |
| --- | --- |
| The published translation | A panproto lens record; see [Lens semantics and laws](./lens-laws.md) |
| Evidence the translation works | `dev.idiolect.verification`; see [the tutorial's verification step](../tutorial/04-verify.md) |
| An endorsement | `dev.idiolect.recommendation` |
| A community's bundled choices | `dev.idiolect.dialect`; see [Bundle records into a dialect](../guide/dialect.md) |
| Noticing that two schemas coexist | `dev.idiolect.encounter` / `dev.idiolect.observation`; see [Observer protocol](./observer.md) |
| Arguing about what to converge on | The deliberation lexicons; see [Deliberation](./deliberation.md) |

## What idiolect does not promise

idiolect does not promist: (i) a single canonical schema; (ii) a global arbiter; 
or (iii) a guarantee of convergence. Two communities may stay incompatible forever; the
frame makes that visible (no lens between them, no
recommendations) rather than impossible. What is promised is
narrower and checkable: published lenses obey stated laws, records
under one NSID share one wire shape, and lexicon revisions ship
with auditable migrations. [Idiolect, dialect,
language](./idiolect-dialect-language.md) states these promises
and non-promises precisely.

## Where next

- [What you need first](./prerequisites.md) gives the assumed
  background.
- The [tutorial](../tutorial/index.md) walks the full loop on real
  records: fetch, validate, apply a lens, verify, publish.

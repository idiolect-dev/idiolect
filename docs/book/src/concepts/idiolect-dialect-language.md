# Idiolect, dialect, language

The project borrows three linguistic terms to distinguish individual practice,
community policy, and shared infrastructure. The analogy is organizational; it
does not claim that schema systems have every property of natural languages.

## Three levels

An [idiolect](../glossary.md#idiolect "One publisher's actual choices of schemas, lenses, and conventions")
is one publisher's practice: the record types it emits, the lenses it uses, and
the conventions it follows. There is no `dev.idiolect.idiolect` record. An
idiolect is inferred from records; a dialect's `idiolects` field lists the
schema references that constitute its idiolect set.

A [dialect](../glossary.md#dialect "A community-published bundle of schema and translation policy")
is an explicit community bundle. A `dev.idiolect.dialect` record names its
owning community and may carry schema references in `idiolects`, along with
`preferredLenses`, `deprecations`, and version links. The dialect is policy
expressed as data; consuming it remains a local choice.

The **language level (LL)** is the shared ATProto substrate plus the schema and
lens records published on it. The LL has no single runtime record and no
requirement that all participants use one schema. Local catalogs, indexers, and
orchestrators construct views over this level.

| Level | Concrete representation | Scope |
| --- | --- | --- |
| Idiolect | A publisher's observed records and choices | One publisher or application |
| Dialect | `dev.idiolect.dialect` | A community's stated schema and lens policy |
| Language | ATProto repositories, event streams, Lexicons, and lenses | The federated substrate |

## Plural canonicity

The model permits more than one community to call a different bundle canonical.
We call this **plural canonicity (PC)**. A consumer resolves PC locally by
choosing which community records, recommendations, vocabularies, and observers
it trusts. The orchestrator's catalog supports that selection; it does not make
the selection universal.

PC has two consequences. First, a new schema can be published without a
network-wide approval step. Second, a lens can connect established schemas
without forcing either publisher to rename its collection. But the same freedom
allows incompatible or malicious records, so policy cannot be derived from
federation alone.

## Failure modes

Three failure modes recur at different levels:

1. **Unlinked duplication.** Two NSIDs describe similar data, but no lens or
   recommendation relates them. Consumers must either treat them separately or
   author the missing relationship.
2. **Vocabulary collision.** Two communities reuse a slug with different
   intended meanings. A `*Vocab` reference can disambiguate the vocabulary, but
   an omitted reference may leave policy to a canonical default outside the wire
   record.
3. **Dialect drift.** A community updates a dialect's preferred lenses or
   deprecations. An AT-URI may then resolve to a new record CID while consumers
   holding an older strong reference continue to see the pinned revision.

A potential worry about PC is that it merely renames fragmentation. That worry
is justified when publishers supply no lenses, evidence, or policy records. The
model does not prevent that outcome. It makes the missing relationships visible
and gives later publishers a common place to add them.

## The actual guarantees

An NSID identifies an intended Lexicon shape, but repositories remain untrusted
input; invalid records may appear on an event stream. A generated decoder can
reject a record it cannot deserialize, though not every semantic constraint is
enforced by deserialization alone. Likewise, a lens carries law claims, but
those claims require checking on concrete instances or stronger external
evidence.

Thus, the three levels distribute responsibility rather than truth. Publishers
choose an idiolect, communities state a dialect, and consumers decide how to
interpret the language-level record population. The
[`dev.idiolect.*` family](./lexicon-family.md) supplies the records used in that
exchange.

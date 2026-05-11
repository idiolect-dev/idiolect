# dev.idiolect.dialect

A community's bundle of idiolect references and preferred
translations. Dialects are declared, not imposed: downstream
consumers may adopt, adapt, or ignore them.

> **Source:** [`lexicons/dev/idiolect/dialect.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/dialect.json)
> · **Rust:** [`idiolect_records::Dialect`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Dialect.html)
> · **TS:** `@idiolect-dev/schema/dialect`
> · **Fixture:** `idiolect_records::examples::dialect`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `owningCommunity` | at-uri | yes | The community that owns this dialect. |
| `name` | string (≤128) | yes | Human-readable dialect name. |
| `description` | string (≤4000 graphemes) | no | Purpose and scope. |
| `idiolects` | array of `schemaRef` | no | Schemas that constitute the dialect's idiolect set. |
| `preferredLenses` | array of `lensRef` | no | Translations the community prefers. |
| `deprecations` | array of `Deprecation` | no | Deprecated entries with replacement pointers. |
| `version` | string | no | Dialect version (semver when applicable). |
| `previousVersion` | at-uri | no | Predecessor revision in a version chain. |
| `createdAt` | datetime | yes | Publication timestamp. |

### `Deprecation`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `ref` | at-uri | yes | The deprecated idiolect or lens. |
| `replacement` | at-uri | no | Optional successor. |
| `deprecatedAt` | datetime | yes | When the deprecation took effect. |
| `reason` | string (≤1000 graphemes) | yes | Why it was deprecated. |

## Field details

### What a dialect is

A dialect is a *bundle*. It does not introduce new lexicons; it
collects existing ones into a coherent set the community treats
as canonical. A consumer that adopts the dialect routes
translations through `preferredLenses`, validates incoming records
against the schemas in `idiolects`, and treats the deprecation
list as a redirect table.

The dialect record is data, not configuration. Adding an entry is
a record edit; deprecating one is another record edit on the same
dialect with a `Deprecation` entry. Two dialects from different
communities can list the same NSID with different preferred
lenses; consumers pick a dialect (or a quorum of dialects) and
follow it.

### `previousVersion` and the version chain

A dialect revision points at its predecessor via `previousVersion`.
A consumer reading the head dialect can walk the chain back
through prior versions, confirm that deprecations were announced
at the right time, and audit the change history without trusting
the orchestrator's catalog.

The chain is not enforced: a community can publish a dialect with
no `previousVersion` (a fresh start) or skip versions (publishing
v3 with `previousVersion = v1`). The substrate records what was
done; consumers decide whether to trust it.

### `deprecations`

Each entry records an idiolect or lens that was once part of the
dialect and is now superseded. The `ref` field points at the
deprecated artifact; `replacement` optionally points at the
successor. Consumers reading a record at the deprecated `ref` can
follow `replacement` to the new one, with the `reason` field
explaining why.

The lexicon-evolution policy generates deprecation entries
automatically when a non-Iso lens revision ships. See
[Lexicon evolution policy](../../concepts/lexicon-evolution.md).

## Example

```json
{
  "$type": "dev.idiolect.dialect",
  "owningCommunity": "at://did:plc:community/dev.idiolect.community/canonical",
  "name": "tutorial canonical",
  "description": "The canonical dialect for the tutorial community.",
  "idiolects": [
    { "uri": "at://did:plc:community/dev.panproto.schema.schema/post-v1" }
  ],
  "preferredLenses": [
    { "uri": "at://did:plc:community/dev.panproto.schema.lens/post-v1-to-v2" }
  ],
  "deprecations": [
    {
      "ref": "at://did:plc:community/dev.panproto.schema.schema/post-v0",
      "replacement": "at://did:plc:community/dev.panproto.schema.schema/post-v1",
      "deprecatedAt": "2026-04-01T00:00:00.000Z",
      "reason": "Replaced by v1 with structured `body` field; lens preserves all v0 records."
    }
  ],
  "version": "1.2.0",
  "previousVersion": "at://did:plc:community/dev.idiolect.dialect/1.1.0",
  "createdAt": "2026-04-19T00:00:00.000Z"
}
```

## Multiple dialects

Two communities can publish disjoint, overlapping, or
contradictory dialects. The substrate treats them as opinions;
nothing in the protocol prefers one over another. Consumers pick
a resolution policy:

- `first-match` — pick the first dialect listed in the consumer's
  config.
- `quorum` — accept a translation when $k$ of $n$ trusted
  dialects endorse the same lens path.
- `merge` — union the entries; on collision, fall back to a
  configured tie-breaker.

See [Bundle records into a dialect](../../guide/dialect.md).

## Concept references

- [Concepts: Idiolect, dialect, language](../../concepts/idiolect-dialect-language.md)
- [Concepts: Lexicon evolution policy](../../concepts/lexicon-evolution.md)
- [Guides: Bundle records into a dialect](../../guide/dialect.md)
- [Lexicons: community](./community.md) · [vocab](./vocab.md)

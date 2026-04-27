# Changelog

All notable changes to this project are recorded in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
idiolect is pre-1.0: the `0.x` series may include arbitrary breaking
changes between minor releases — Rust APIs, lexicon shapes, wire
formats, and CLI surfaces are all in scope. Pin to an exact version
if you depend on this project, and read this file before bumping.

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

<!--
The release pipeline extracts this section between `[Unreleased]`
and the first versioned heading below. Keep it current: PRs that
ship user-visible changes should append a bullet to the matching
subsection under `[Unreleased]`, and the release cut moves these
lines into the new versioned section.
-->

## [0.4.2] — 2026-04-26

### Fixed

- `@idiolect-dev/schema`'s build now copies the workspace-root
  `lexicons/` tree into `packages/schema/lexicons/` before
  publish, so the directory the package's `files` array claims
  is actually included in the npm tarball. Previously the path
  was workspace-relative-only — `loadLexiconDocs()` worked from
  the source repo but threw `ENOENT` for any consumer importing
  `@idiolect-dev/schema` from `node_modules/`. Also retargets
  `LEXICONS_DIR` to `../lexicons` (sibling of `dist/`) so the
  same path resolves cleanly in dev (off `src/`) and after
  publish (off `dist/`).

## [0.4.1] — 2026-04-26

### Fixed

- The release pipeline's `publish-npm` job now runs `bun run build`
  before `npm publish`, so the published `@idiolect-dev/schema`
  tarball includes the compiled `dist/` tree the package's
  `"main"` / `"types"` / `"exports"` entries point at. v0.4.0
  shipped without `dist/` (only `src/` and a stale
  `dist/.tsbuildinfo` from the typecheck step) — consumers
  importing from `@idiolect-dev/schema` saw "Cannot find module"
  at typecheck time. The earlier v0.3.0 publish was driven from a
  developer machine where the build step ran implicitly; v0.4.0
  was the first fully-CI-driven release and exposed the missing
  step. Republish via the v0.4.1 tag picks up the fix.

## [0.4.0] — 2026-04-26

### Added

- `idiolect_records::Datetime` and `idiolect_records::Uri` typed
  newtypes alongside the v0.3.0 `Nsid`, `AtUri`, and `Did`. Each
  validates at parse time (`time::OffsetDateTime` for RFC 3339,
  `url::Url` for URIs) and exposes the standard `Deref<Target=str>`,
  `Borrow<str>`, `AsRef<str>`, `Display`, `FromStr`, `Serialize`,
  and `Deserialize` impls.
- A walk-up disambiguation pass in `idiolect-codegen`'s Rust and
  TypeScript record re-exports. Two records under different parent
  namespaces that share a leaf TypeName (e.g.
  `pub.layers.changelog.entry::Entry` vs
  `pub.layers.resource.entry::Entry`) now alias as
  `ChangelogEntry` / `ResourceEntry`. Records with unique leaf
  TypeNames keep their unaliased `pub use` lines unchanged.
- `notes/dependent-optics-codegen.md` — forward-looking design
  note for the panproto v0.40 emission migration. Fixes
  vocabulary (focus-edge → optic kind dispatch via
  `panproto_lens::scoped`); does not change v0.4 emission.

### Changed

- **Breaking (Rust API).** Codegen now emits typed values for
  every format-declared lexicon field. `format: "at-uri"` →
  `idiolect_records::AtUri`, `format: "did"` →
  `idiolect_records::Did`, `format: "datetime"` →
  `idiolect_records::Datetime`, `format: "uri"` →
  `idiolect_records::Uri`, `format: "nsid"` →
  `idiolect_records::Nsid`. Previously every format collapsed to
  `String`. Callsites that read these fields now see the typed
  value; serialization shape is unchanged. The 196 format
  declarations across `lexicons/dev/idiolect/**` and the vendored
  `lexicons/dev/panproto/**` are all in scope.
- TypeScript codegen continues to emit format-declared fields as
  `string` for v0.4. Branded-string wrappers and the matching
  runtime validator helpers are deferred until panproto v0.40's
  upstream emitter lands.

### Fixed

- Closes #31. With v0.3.0's nested file layout, two records under
  different parent namespaces that ended in the same leaf
  TypeName produced colliding `pub use … as TypeName;` lines at
  the crate root. Phase C's walk-up disambiguation produces
  unique aliases only for the colliding groups; consumers like
  `layers-pub` that depend on the generated record set now
  compile clean.

### Stability

- v0.4 ships zero new emission machinery in anticipation of
  panproto v0.40's upstream schema-to-target-language emission.
  When v0.40 lands, `idiolect-codegen` will migrate to that
  surface. The typed-format boundary, the walk-up alias contract,
  and the dependent-optics design note are intentionally chosen
  to make that migration mechanical.

## [0.3.0] — 2026-04-25

### Added

- Typed `Nsid`, `AtUri`, and `Did` in `idiolect-records`. The atproto
  NSID spec (authority + name segments, ASCII rules, length cap) is
  enforced at parse time so malformed identifiers cannot reach the
  firehose decoder, indexer, observer, codegen, or orchestrator.
  `AtUri` and `Did` compose the typed `Nsid` and move out of
  `idiolect-lens` and `idiolect-identity` respectively, with
  re-exports keeping the familiar import path on each crate.
- `coercion-law` verification kind plus the `CoercionLawRunner` that
  drives `dev.panproto.translate.verifyCoercionLaws` and folds the
  returned violation list into a `Holds` / `Falsified` `Verification`
  record. The runner is generic over a `CoercionLawClient` so unit
  tests stub the xrpc while deployments wire up an http-backed client.
- New vendored panproto lexicons: `dev.panproto.schema.theory` and
  `dev.panproto.schema.protocol`.

### Changed

- Generated Rust and TypeScript trees mirror the `lexicons/`
  directory layout 1:1: `lexicons/dev/panproto/schema/lens.json`
  emits `crates/idiolect-records/src/generated/dev/panproto/schema/lens.rs`
  and `packages/schema/src/generated/dev/panproto/schema/lens.ts`.
  Per-directory `mod.rs` (Rust) and `index.ts` (TypeScript) barrels
  stitch the tree into compilable module graphs. The change unblocks
  consumers with non-flat namespace trees that previously collided
  on a flat last-segment filename (issue #21).
- `PanprotoVcsClient` broadens from a single `fetch_object` to the
  full `dev.panproto.sync.*` xrpc surface: `get_object`, `get_ref`,
  `set_ref`, `list_refs`, `list_commits`, `get_head`,
  `get_schema_tree`, `list_theories`, `list_alignments`. The mutable
  ref table moves out of `PanprotoVcsResolver` and into the client.
- Vendored `dev.panproto.*` lexicons re-vendored against the upstream
  pin recorded in `lexicons/dev/panproto/VENDORED.md`. `commit.json`
  picks up `protocolHash`, `theoryIds`, `dataHashes`,
  `complementHashes`, `editLogHashes`, `cstComplementHashes`,
  `timestamp`, and the `#namedHash` def; `protolens.json` adds the
  `droppedEdge` constructor.
- Workspace `Cargo.toml` pins the `panproto-*` crates to the matching
  upstream release.
- `IndexerEvent::collection` is now `Nsid` (was `String`); jetstream
  and other adapters parse-and-validate at the stream-decode boundary.
- `idiolect-records::generated` no longer flat-re-exports per-lexicon
  modules. Submodules are reached via the nested tree path
  (`idiolect_records::generated::dev::idiolect::adapter` etc.); the
  per-record-type re-exports (`idiolect_records::Encounter`,
  `idiolect_records::PanprotoLens`, …) at the crate root are
  preserved and now generated rather than hand-edited.
- `SchemaLoader::load`'s contract is documented as scope-agnostic:
  the loader returns whatever panproto `Schema` is content-addressed
  by `object_hash` regardless of whether it came from a single source
  file or a project-scope union.

### Removed

- `PanprotoVcsClient::fetch_object` (replaced by `get_object` plus
  the broader xrpc surface).
- The flat module re-exports at `idiolect_records::adapter`,
  `idiolect_records::encounter`, … (consumers move to the nested
  `idiolect_records::generated::dev::*` paths).

### Fixed

- Codegen file-name collisions for NSIDs that share their last
  segment across distinct authority chains (closes #21).
- Outdated panproto pin and missing xrpc surface that blocked the
  coercion-law runner (closes #22).

## [0.2.0] — 2026-04-23

### Added

- `dev.idiolect.belief` record for nested third-party attitude
  attribution (labeler publishing claims *about* another record).
- `dev.idiolect.vocab` record — community-published action / purpose
  vocabularies with a declared subsumption world (`closed-with-default`,
  `open`, `hierarchy-closed`) and optional attitudinal `class` per
  entry.
- `holder` and `basis` fields on every attitudinal record (encounter,
  correction, bounty, recommendation, verification, observation,
  retrospection, belief), distinguishing first-party from third-party
  attribution and grounding the assertion in a structured basis
  variant (self-asserted / community-policy / external-signal /
  derived-from-record).
- `ThTarget`, `ThEvidential`, `ThIllocutionary`, and `ThCharter`
  content theories; a factored-out `ThPredicate` shared substrate
  for `ThCondition` and `ThEligibility`; `extends` + `_extract` +
  coherence equations on every theory YAML.
- `morphisms/idiolect/` directory for inter-theory functors, split
  from the lens-as-data-transformation directory.
- `lenses/vocab/` — worked vocabulary-translation lens example with a
  dedicated README, illustrating the action / purpose translation
  pattern end-to-end.
- Orchestrator: catalog tracks `Belief` + `Vocab`; theory-resolver
  gained `sync_from_catalog` and `class_of`; new `predicate_eval`
  module evaluates recommendation condition/precondition/bounty
  eligibility trees via a postfix stack machine with Kleene
  three-valued logic (`Holds` / `DoesNotHold` / `Unresolved`).
- Observer: `purpose-distribution`, `basis-distribution`,
  `attribution-chains` bundled methods.
- XRPC facade for every catalog query — each `orchestrator-spec/queries.json`
  entry is mounted at both its REST path and
  `/xrpc/dev.idiolect.query.<camelName>`, with auto-generated
  `dev.idiolect.query.*` lexicons under `lexicons/dev/idiolect/query/`.
- Four new catalog queries: `beliefs_about_record`,
  `beliefs_by_holder`, `vocabularies_with_world`,
  `vocabularies_by_name`.
- `encounter record` CLI subcommand with structured purpose prompts.

### Changed

- Free-text decision fields across the attitudinal records replaced
  with structured content-theory references. Narrative prose moves
  to companion `*_text` fields where it's still useful; the
  machine-actionable shape is now a tagged-union of theory
  primitives that consumers dispatch on.
- `dev.idiolect.encounter` structured content theory renamed from
  `ThPurpose` to `ThUse`, splitting the previous single `purpose`
  string into `use.action` + `use.purpose` with separate
  `action_vocabulary` and `purpose_vocabulary` references; the
  theory-resolver was ported to operate on this shape end-to-end.
- Recommendation `required_verifications` is now a structured
  `LensProperty` value rather than a bare `VerificationKind`;
  `sufficient_verifications_for` uses a structural match with
  empty-field wildcarding instead of kind-only equality.
- Compositions rewritten with explicit colimit steps; vocabulary
  entries annotated with `composition class` so consumers can
  respect both subsumption hierarchy and attitudinal shape.
- Codegen `GeneratedFile` builder replaces the prior `push_str`-based
  file-header assembly; emitters supply lint allow-lists as typed
  slices.
- Claimer eligibility uses typed `Vec<LensProperty>` instead of a
  stringly-encoded `Vec<String>` tag list; eligibility evaluator
  matches structurally.
- Basis-distribution method keys internally as
  `(RecordKind, BasisTag)` tuples; strings appear only at the
  JSON snapshot boundary.

### Fixed

- Sqlite catalog store's `kind_tag`, `serialize_record_body`, and
  `deserialize_record_body` now handle the new `Belief` and `Vocab`
  variants (previously would have panicked on ingest).
- `/v1/stats` and `/metrics` count beliefs and vocabularies.
- Generated HTTP handlers validate `world`, `verification kind`, and
  `adapter invocation protocol` query-string tokens at the boundary
  via dedicated parsers.
- Codegen now collects imports for tagged-union variant payloads
  emitted under nested `defs`, fixing a missing-import regression
  that surfaced when theory primitives grew union-of-union shapes.

### Removed

- Dead `required_kind_to_verification_kind` helper (superseded by
  structural `requirement_matches`).

### Security

- Bumped `rustls-webpki` to 0.103.13 to pick up the fix for
  [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104).


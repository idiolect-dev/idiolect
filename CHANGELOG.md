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

## [0.7.0] — 2026-04-29

### Added

- `dev.idiolect.deliberation`, `dev.idiolect.deliberationStatement`, `dev.idiolect.deliberationVote`, and `dev.idiolect.deliberationOutcome` lexicons. The four together model community-scoped deliberation as a process-shaped concept distinct from settled belief or recommendation, and subsume Acorn's `community.blacksky.assembly.*` records (conversation / statement / vote) losslessly. Stance, classification, and status fields are open-enum slugs resolved through community-published vocabularies; the canonical defaults seed `agree`/`pass`/`disagree`, `claim`/`proposal`/`dissent`/`clarification`/`question`, and `open`/`closed`/`tabled`/`adopted`/`rejected`. `deliberationOutcome` is observer-published and carries per-statement per-stance tallies plus optional weighted aggregates so consumers reading a closed deliberation can fetch the resolution without re-folding every vote.
- Knowledge-graph shape on `dev.idiolect.vocab`. Vocabularies now carry typed `nodes` (with open-enum `kind` discriminator: `concept`, `relation`, `instance`, `type`, `collection`) and typed `edges` (`{source, target, relationSlug}`), in addition to the legacy single-relation tree (`actions` + `parents`). The legacy shape stays valid; consumers normalise both into the same read-side view. Modeled on `pub.chive.graph.{node,edge}`.
- Full OWL Lite property-characteristic set on `relationMetadata`: `symmetric`, `asymmetric`, `transitive`, `reflexive`, `irreflexive`, `functional`, `inverseFunctional`, plus `inverseOf` and a per-relation `world` override. `RelationProperties::contradictions` flags `symmetric+asymmetric` and `reflexive+irreflexive`; `VocabGraph::validate` and `VocabRegistry::validate` walk authored relations and emit concrete `VocabViolation` values for `FunctionalEdgeViolation`, `InverseFunctionalEdgeViolation`, `IrreflexiveSelfLoop`, `AsymmetricMutualEdge`, and `PropertyContradiction`. The flags are enforceable, not advisory.
- Full SKOS Core annotation set on `vocabNode`: `label` (prefLabel), `alternateLabels` (altLabel), `hiddenLabels`, `description` (definition), `scopeNote`, `example`, `historyNote`, `editorialNote`, `changeNote`, and `notation` (non-text classification codes like Dewey decimals). `kind="collection"` plus `member_of` edges express SKOS Collection. `externalIds` mappings carry SKOS-style match types (`exact` / `close` / `broader` / `narrower` / `related`) for cross-system grounding into Wikidata, ROR, ORCID, ISNI, VIAF, LCSH, FAST, SKOS, Dublin Core, Schema.org, MeSH, AAT.
- `idiolect_records::vocab::VocabGraph`: normalised read-only view over a `Vocab` record. Lifts both legacy tree and new graph shapes into a uniform indexed map. Exposes `walk_relation(source, relation, reflexive)` as the canonical traversal primitive; `is_subsumed_by`, `subsumed_by`, `equivalent_in`, `top` / `top_with`, `relation_properties`, `direct_targets` / `direct_sources`, and `validate` are all built on top. Symmetric relations declared via `relationMetadata.symmetric` walk both directions; the legacy `subsumed_by` semantic (transitive + reflexive) applies uniformly.
- `idiolect_records::vocab::VocabRegistry`: long-lived cache keyed by AT-URI with three core query verbs: `is_subsumed_by(uri, x, y)`, `satisfies(uri, x, relation, y)` (generalised forward-reachability under any directed relation), and `translate(from_uri, to_uri, slug)` for cross-vocab equivalence walking. Plus `validate` for batch OWL Lite consistency checks across every registered vocab.
- Open-enum convention applied to `dev.idiolect.adapter` (`invocationProtocol.kind`, `isolation.kind`, `isolation.networkPolicy`, `isolation.filesystemPolicy`), `dev.idiolect.bounty` (`status`, `wantVerification.kind`, `constraintConformance.kind`), `dev.idiolect.correction` (`reason`), `dev.idiolect.encounter` (`downstreamResult`, `kind`), `dev.idiolect.observation` (`scope.encounterKinds`), `dev.idiolect.retrospection` (`finding.kind`), `dev.idiolect.verification` (`kind`, `result`), and `dev.idiolect.community` (`conventionVerificationReq.kind`). Each opened field becomes `knownValues` plus an optional `*Vocab: vocabRef` sibling resolving the slug through a published vocabulary record. Wire-compatible: existing records continue to validate.
- Codegen support for open enums in both Rust and TypeScript. `idiolect-codegen` reads `knownValues` and emits Rust enums with one variant per known value plus an `Other(String)` fallback (or `Extended` / `Custom` / `Variant` / `Other<n>` when known values collide), with hand-written `Serialize` / `Deserialize`, `From<String>`, `From<&str>`, and `as_str()` impls. Identifier collisions for distinct slugs that pascal-case to the same Rust ident (e.g. `foo-bar` and `foo_bar`) get a numeric suffix on the second occurrence. TypeScript emits `'a' | 'b' | (string & {})` to keep IntelliSense surfacing while admitting any string at the type level.
- Codegen-emitted helper methods on every open-enum type: `is_subsumed_by(&VocabGraph, ancestor)`, `satisfies(&VocabGraph, relation, target)`, and `translate_to::<T: From<String>>(src_uri, tgt_uri, &VocabRegistry)`. Lets consumers reason about open-enum slugs against a vocab without manually calling `walk_relation`. Cross-vocab translation routes through `VocabRegistry::translate`; missing paths return `None` so callers can fall back to passing the slug verbatim.
- `idiolect-lens` gains the `map_enum` module: `map_enum(from, to, slug)` and `map_enum_graphs(g_from, g_to, slug)` for cross-vocab slug translation via `equivalent_to` walks. Engine for bridging community vocabularies (e.g. translating an Acorn vote stance into the canonical idiolect vote-stances).
- `dev.idiolect.community` gains `roleAssignments` (sparse `[{did, role}]` list paired with `memberRoleVocab`), `recordHosting` (open-enum: `member-hosted` / `community-hosted` / `hybrid`), and `appviewEndpoint`. Lets a consumer reading a community record know to route XRPC reads through an AppView (Acorn's mode) rather than crawling member PDSes.
- Eight reference seed vocabularies under `lexicons/dev/idiolect/examples/vocab/`: `invocation-protocols-v1`, `isolation-kinds-v1`, `network-policies-v1`, `verification-kinds-v1` (with `stronger_than` edges so `formal-proof` satisfies a `property-test` requirement transitively), `vote-stances-v1` (with `polar_opposite_of` edges), `community-roles-v1` (with `member` as top), `deliberation-statuses-v1`, `statement-classifications-v1`. Authored as graph-shape vocabularies; demonstrate every relation property the runtime supports.
- `scripts/lexicon-evolve.sh` and `.github/workflows/lexicon-evolution.yml`: standing six-stage policy for lexicon revisions (diff, auto-derive protolens chain, classify Iso / Injection / Projection / Affine / General, coercion-law check, roundtrip verification against live corpus, lens publication). Each stage maps to a panproto primitive; nothing bespoke. The classification gates merge: Iso / Injection auto-merge, Projection requires complement disclosure, Affine / General require manual lens authoring plus governance sign-off.
- `idiolect_observer::DeliberationTallyMethod`: reference observer fold over `dev.idiolect.deliberationVote` records emitting per-statement per-stance tallies in the `deliberationOutcome.statementTallies` shape. Optional cross-dialect canonicalisation through `with_canonical_stance_vocab(uri, registry)`: every incoming vote's stance translates via `equivalent_to` edges into the canonical vocab before counting, so a vote authored as `endorse` against a community vocab declaring `endorse equivalent_to agree` lands in the same `agree` bucket as a canonical-vocab vote. Untranslatable slugs pass through verbatim.
- `idiolect_orchestrator::query::get_deliberation(catalog, uri)`: composed read returning `DeliberationView { deliberation, statements, votes, outcomes }` filtered by strong-ref equality. `Catalog` now persists deliberation, statement, vote, and outcome records (the previous `_ => {}` arm silently dropped them); `catalog_stats` reports their counts.
- `examples/idiolect-acorn/`: scaffold demonstrating the bridge pattern. Vendored `community.blacksky.assembly.{conversation,statement,vote}` lexicons, a Blacksky vote-stance vocabulary with `equivalent_to` edges into the canonical idiolect vocab, `dev.idiolect.community` / `dialect` / `adapter` records describing Blacksky communities (using the new `recordHosting=community-hosted` + `appviewEndpoint` fields), and three Nickel lens specs mapping Acorn assembly records into `dev.idiolect.deliberation*` with predicted optic classes (Iso / Injection / Projection).
- `docs/vocab-expressiveness.md`: design document characterising what `dev.idiolect.vocab` can and cannot represent. Places the vocab in the landscape of established standards (RDFS, OWL Lite, SKOS Core, property graphs), documents the two distinctive features (per-relation world discipline, ATProto-native federation), and outlines computational profile and practical ceiling guidance.
- `crates/idiolect-indexer/tests/or_family.rs`: integration test demonstrating the canonical `OrFamily<IdiolectFamily, OtherFamily>` wiring pattern. Drives a mixed firehose carrying both `dev.idiolect.*` and a stub `community.example.*` family through `drive_indexer`. Fills in for the `firehose-or-family.rs` example bin from the original plan: in production the `StubCommunityFamily` is replaced verbatim with a codegen-emitted family.
- `crates/idiolect-codegen/tests/parser_known_values_drift.rs`: drift-detection test that loads each lexicon JSON, walks to the enum-bearing field referenced by the spec-driven orchestrator's `parser_known_values`, and asserts the hardcoded list matches the lexicon's actual `knownValues` / `enum`. A lexicon edit that changes those values now fails the test instead of silently desyncing the XRPC parameter validators.

### Changed

- `dev.idiolect.vocab` required-fields list shrinks from `["name", "world", "top", "actions", "occurredAt"]` to `["name", "world", "occurredAt"]`. `top` and `actions` are now optional, since graph-shape vocabularies populate `nodes` + `edges` instead. Wire-compatible: existing tree-shape vocab records continue to validate.
- Closed `enum` lists across the lexicon family converted to `knownValues` + sibling `*Vocab` pointers (see Added section for the per-lexicon list). Wire form unchanged; downstream consumers gain the `Other(String)` extension surface for community-extended slugs without forking the lexicon.
- Generated TypeScript open-enum types render as `'a' | 'b' | (string & {})` (was a closed string-literal union when the field was a closed `enum`). The trailing `string & {}` intersection keeps known-value IntelliSense surfacing while permitting any string at the type level.
- `idiolect_observer` reference methods that previously hardcoded `_key` helpers (`reason_key`, `kind_key`, `result_key`) for closed-enum-to-string mapping now route through the codegen-emitted `as_str()` on each open-enum type. Histogram maps switched from `BTreeMap<&'static str, u64>` to `BTreeMap<String, u64>` to carry community-extended slugs without erasing them.
- Theory resolver and the `action_distribution` / `purpose_distribution` observer methods now build their internal ancestor maps via `VocabGraph::from_vocab` rather than iterating `Vocab.actions` directly. Graph-shape vocabs (no legacy `actions`) now produce correct subsumption closures; previously they silently produced empty maps.
- `validators.test.ts`: the `correction.reason` rejection test reframed for the open-enum convention. Closed enums (e.g. `visibility`) still reject unknown values; open enums (e.g. `correction.reason`) accept them as wire-compatible community extensions.

### Removed

- Hardcoded enum-to-wire-form helper functions in observer methods (`correction_rate::reason_key`, `encounter_throughput::kind_key` / `result_key`, `verification_coverage::kind_key` / `result_key`, `idiolect-cli::encounter_kind_wire`). Replaced by the codegen-emitted `as_str()` method on each open-enum type.
- "Currently advisory" semantics on `RelationProperties::functional` and `inverseFunctional`. Both flags are now enforced by `VocabGraph::validate` (and `VocabRegistry::validate`) which emits concrete `VocabViolation` values for sources with multiple outbound functional edges or targets with multiple inbound inverse-functional edges.

### Fixed

- Open-enum codegen variant naming collisions resolved at emit time. When two distinct slugs pascal-case to the same Rust identifier (e.g. `foo-bar` and `foo_bar` both yielding `FooBar`), the second and subsequent occurrences get a numeric suffix; the wire form preserves the original slugs in match arms. Previously this produced duplicate-variant Rust enums that failed to compile.
- Open-enum codegen fallback variant collision with the `Other` slug. When `knownValues` already contains `"other"`, the fallback `Other(String)` would name-clash with the known variant; the codegen now picks the first non-colliding name from `Other` / `Extended` / `Custom` / `Variant` and falls through to a numeric suffix.
- Identifier sanitisation for slugs containing characters illegal in Rust identifiers (dots, slashes, colons, Unicode characters, leading digits, all-non-alphanumeric strings). The layers-pub fixture's `chive.pub` slug now produces a valid `ChivePub` variant.
- Symmetric-relation walks. `VocabGraph::walk_relation` honours `relationMetadata.symmetric` by traversing both outbound and inbound edges as one set. Previously walked outbound only, which silently lost reachability under symmetric relations like `equivalent_to`.
- `top` derivation for graph-shape vocabs. `VocabGraph::top()` recovers the unique non-relation root by scanning for nodes with no outbound `subsumed_by` edge. The `closed-with-default` rollup logic in observer methods previously fell back to an empty string when `Vocab.top` was unset, polluting the empty-string bucket; it now uses `top_with(vocab.top.as_deref())` which prefers the explicit field but falls through to graph derivation.
- `subsumed_by` legacy semantic now applies uniformly across `Default::default()` and `from_vocab`-constructed `VocabGraph`s. The previous seed only ran in `from_vocab`; default graphs returned all-false properties for the relation, which broke transitive ancestor walks against newly-built registries.
- Reference seed vocabularies now use AT-URI references without fragments. The previous form (`at://...#subsumed_by`) parsed-rejected because `idiolect_records::AtUri` does not accept fragments per ATProto.

### Security

## [0.6.1] — 2026-04-28

### Fixed

- `idiolect-codegen`'s family emitter named `AnyRecord` variants and `crate::<TypeName>` paths from the unqualified record type, while `mod.rs`'s walk-up disambiguation aliased colliding leaf names. Two records sharing a leaf TypeName produced duplicate enum variants and dangling `crate::<TypeName>` references that the crate-root re-export never declared. The family emitter now reuses the same disambiguation pass and reaches per-record types through their full `crate::generated::<…>::<TypeName>` path with a local `use … as <UniqueIdent>` binding, so families with cross-prefix leaf-name collisions (e.g. `pub.layers.changelog.entry::Entry` vs `pub.layers.resource.entry::Entry`) emit unique variants and compile. Closes #44.

## [0.6.0] — 2026-04-28

### Added

- `idiolect_records::Cid` newtype with parse-time multibase + multihash validation via the `cid` crate. Codegen now emits `dev.idiolect.*` `cid-link` fields as `Option<Cid>` (was `Option<String>`). The wrapper preserves the canonical wire form so byte-for-byte fixture round-trips stay stable.
- `idiolect_records::Language` newtype with parse-time BCP 47 validation via the `language-tags` crate. Codegen now emits `format: "language"` fields as the typed wrapper. Validation is structural (the IANA registry is not consulted) and the wire form is preserved verbatim.

### Changed

- **Breaking.** `idiolect-codegen`'s `FamilyConfig` fields are now `Cow<'static, str>` (was `&'static str`), so downstream codegen consumers can construct one from runtime-owned strings without `Box::leak`. Static-literal call sites stay zero-allocation through `Cow::Borrowed`. Add `FamilyConfig::new(impl Into<Cow<'static, str>>, ...)` so callers don't have to name `Cow` at the construction site.
- **Breaking.** `idiolect-codegen`'s `IDIOLECT_FAMILY` const is replaced by an `idiolect_family()` fn. Update direct references accordingly.
- **Breaking.** `idiolect-codegen`'s `emit::emit_rust`, `emit::emit_typescript`, and `target::TargetEmitter::emit` now take a `&FamilyConfig` argument. The CLI binary continues to pass `&idiolect_family()` so its surface is unchanged; downstream consumers (e.g. `layers-codegen` over `pub.layers.*`) construct their own `FamilyConfig::new(...)` rather than vendoring the target loop. Closes #41.
- **Breaking.** The `@idiolect-dev/schema` package now exports `family.ts` in place of `records.ts`. `AnyRecord`, `NSID`, `RecordTypes`, `isKind`, `is{Type}` guards, `tagRecord`, and `RECORD_NSIDS` keep their signatures; new exports are `FAMILY_ID`, `FAMILY_NSID_PREFIX`, `FamilyMarker` type, `familyContains(nsid)`, `decodeRecord(value)` (returns a loose `DecodedRecord` because TypeScript has no runtime structural validator for the wire form), and `toTypedJson(r)`. The TS family is now scoped to records matching `FAMILY_NSID_PREFIX`, dropping the vendored `dev.panproto.*` records that used to bundle into the unscoped `records.ts`. External consumers reach the package through `index.ts` which re-exports the new module, so the file rename is invisible at the public surface.

<!--
The release pipeline extracts this section between `[Unreleased]`
and the first versioned heading below. Keep it current: PRs that
ship user-visible changes should append a bullet to the matching
subsection under `[Unreleased]`, and the release cut moves these
lines into the new versioned section.
-->

## [0.5.0] — 2026-04-27

### Added

- `idiolect_records::family::RecordFamily` trait. Every record-set
  scoped boundary in the workspace (indexer, handlers, soon
  orchestrator + verify) parameterises over a family rather than
  hardcoding the `dev.idiolect.*` set. Closes #38.
- `idiolect_records::OrFamily<F1, F2>` composer plus `OrAny` tagged
  union, so dialect bundles (curated cross-family record sets) are
  first-class. Includes `detect_or_family_overlap` for boot-time
  configuration audits.
- `idiolect_records::IdiolectFamily` marker, with the `RecordFamily`
  impl produced by `idiolect-codegen`'s new family emitter
  (`crates/idiolect-codegen/src/emit/family.rs`). The hand-written
  `AnyRecord` enum and `decode_record` function are gone; the
  identical surface is re-exported from `generated::family`.
  Adding a record to the family is now a one-line lexicon change.
- `drive_idiolect_indexer` convenience entry point. Runs
  `drive_indexer` against `IdiolectFamily` without an explicit
  type argument.

### Changed

- **Breaking (Rust API).** `idiolect_indexer::IndexerEvent`,
  `RecordHandler`, and `drive_indexer` are now generic over
  `F: RecordFamily` (default `IdiolectFamily`). Existing call
  sites that don't name the type parameter keep working through
  the default, but signatures with explicit `IndexerEvent` /
  `RecordHandler` bounds need to add the family parameter.
  Closes #39.
- **Breaking (Rust API).** `IndexerConfig::nsid_prefix` is gone.
  Family membership lives in the family's `RecordFamily::contains`
  predicate; one source of truth. The default `IndexerConfig`
  shrinks to just `subscription_id`.
- **Breaking (Rust API).** An unknown `dev.idiolect.*` NSID (one
  whose authority+name match `dev.idiolect.*` but isn't a known
  record type at codegen time) used to halt the loop with
  `IndexerError::Decode(UnknownNsid)`. It now flows through
  `IdiolectFamily::contains` and is dropped silently as
  out-of-family. The previous behaviour was brittle to upstream
  PDS additions landing ahead of our codegen; the new behaviour
  absorbs them gracefully.
- `idiolect_observer::driver` runs against `IdiolectFamily`
  explicitly. Observers are domain-coupled to the encounter /
  correction / observation set by construction; the bound is
  written into the type signature now instead of being implicit.

## [0.4.3] — 2026-04-27

### Added

- `@idiolect-dev/schema` exports `bundledLexiconDocs` and
  `bundledLexicons()` for browser consumers. The build pipeline
  bakes every lexicon JSON document under `lexicons/dev/**` into
  `src/generated-lexicons.ts` (via `scripts/copy-lexicons.ts`),
  and the bundler inlines it into `dist/index.js`. Browser apps
  import the bundled docs as plain ES module data without needing
  `node:fs`. The Node-side `loadLexiconDocs()` and
  `defaultLexicons()` helpers are unchanged.

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

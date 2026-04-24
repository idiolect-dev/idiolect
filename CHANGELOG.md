# Changelog

All notable changes to this project are recorded in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Breaking lexicon changes take a minimum six-month deprecation window
per the project's stewardship process.

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


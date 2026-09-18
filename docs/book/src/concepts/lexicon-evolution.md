# Lexicon evolution policy

A Lexicon revision can preserve validation compatibility while changing what
generated programs can safely assume. idiolect's intended response is the
**evolution evidence chain (EEC)**: classify the schema change, derive or author
a lens when migration is needed, verify its stated properties, publish it, and
connect community policy to that record.

The current checkout implements parts of the EEC in Rust and describes the rest
in a shell script and CI workflow. It does not yet enforce the entire chain.

## Compatibility and migration are different

`idiolect-migrate` begins with `panproto_check::diff` and
`panproto_check::classify`. If a diff has no breaking changes, `plan_auto`
returns `OnlyNonBreaking`: old records remain readable under the new schema, so
the library does not manufacture a migration plan.

If breaking changes exist, `plan_auto` asks panproto 0.74.4's `auto_generate`
for a `ProtolensChain`. Its default `Balanced` configuration uses the exact
valued-CSP optimizer and requires a total morphism. Success returns a
`MigrationPlan` containing the source and target schema identifiers supplied by
the caller, the chain, and an alignment-quality score. This score ranks
alignments for one source schema; it is not a confidence measure with a stable
threshold across unrelated pairs. Failure returns the breaking changes that
need manual attention.

This yields three distinct outcomes:

1. no structural change;
2. a compatible change that needs no record migration; or
3. a breaking change that needs an auto-derived or hand-authored lens.

Compatibility is thus a read-side property, while migration is an
operation over existing records.

## From a plan to a published lens

A `MigrationPlan` is not an ATProto record. The caller must serialize its
protolens chain into a `dev.panproto.schema.lens` `blob`, provide the schema
record references and object hash expected by the vendored Lexicon, publish the
record, and retain its AT-URI. `idiolect-migrate::migrate_record` can then apply
that published lens through the normal `idiolect-lens` runtime.

Verification remains a separate step. `RoundtripTestRunner` can check GetPut on
a nonempty corpus; `PropertyTestRunner` can generate a finite set of cases;
`StaticCheckRunner` validates the two schema graphs. These checks may falsify a
claim. A passing finite run does not establish the corresponding universal law.

Finally, a community may add the lens to a dialect's `preferredLenses`, add a
deprecation entry for the prior object, or publish a recommendation. None of
those policy records is created by `plan_auto`.

## Optic kinds are not governance classes

panproto 0.74.4 classifies transforms as `Iso`, `Lens`, `Prism`, `Affine`, or
`Traversal`. Earlier versions of this chapter described a different five-way
set, `Iso`/`Injection`/`Projection`/`Affine`/`General`, and assigned automatic
merge policy to it. That set is not the current `OpticKind` API.

A project can still define review rules over current optic kinds, complement
requirements, compatibility reports, alignment quality, and verification
evidence. Such rules are idiolect governance policy; they should not be
presented as classifications returned by panproto.

## Current automation boundary

The repository contains `scripts/lexicon-evolve.sh` and
`.github/workflows/lexicon-evolution.yml` implement the first part of the EEC
against panproto 0.74.4. The workflow installs the versioned CLI artifact,
materializes the base revision of each changed Lexicon, and retains two forms
of evidence: the JSON result of `schema compat` and the textual result of
`schema diff --optic-kind`. The gate admits `iso` and `prism`, requires review
for `lens`, and holds `affine` and `traversal` changes for a hand-authored chain
and governance sign-off.

This gate does not yet serialize the auto-derived chain. The released `diff`
and `compat` commands load manifest-backed ATProto Lexicon projects, while the
file-path chain-writing route does not yet share that loader. Verification and
publication thus begin after the CI gate, using `idiolect-verify`,
`idiolect-lens`, and an authenticated PDS writer. We call this remaining
discrepancy the **chain-publication gap (CPG)**.

## Opportunities opened by panproto 0.74.4

The newer Panproto surface suggests four concrete tooling improvements beyond
the dependency bump:

1. The canonical protocol registry could replace direct calls to the ATProto
   parser and protocol constructor in schema loading, code generation, and
   verification. This would give those tools one capability-discovery path and
   make support for another registered protocol an explicit configuration
   choice.
2. Shared `ResourceLimits` and `Budget` values could bound schema parsing,
   morphism search, migration, and verification at every untrusted-input
   boundary. Idiolect currently has service-level limits, but it does not yet
   pass one Panproto budget through the entire operation.
3. Panproto's three-valued verification result should flow into
   `idiolect-verify` reports and published attestations. `Incomplete` is
   evidence that a check did not finish, not a passing result; CI and CLI exit
   codes should preserve that distinction.
4. Cross-document ATProto parsing and the corrected VCS staged-data path could
   make repository-wide Lexicon checks operate on one resolved project rather
   than on files independently. This is the natural route to closing the CPG:
   stage both revisions, derive a chain over the assembled schemas, then retain
   its object identity with the compatibility and corpus evidence.

## A defensible gate

Until the CPG is closed, reviewers can apply the rest of the EEC as an explicit
checklist after the automated compatibility and optic gate:

1. compare the old and new parsed schemas with `idiolect-migrate::classify`;
2. require a reviewed `MigrationPlan` or a hand-authored chain for each
   breaking change;
3. inspect the current `OpticKind` and complement requirements;
4. run the applicable verification runners on versioned inputs;
5. publish the lens and verification records through an authenticated writer;
6. update dialect, deprecation, and recommendation records deliberately; and
7. retain the artifacts that identify the two schema revisions and test corpus.

Two points follow:

1. The EEC does not make every migration reversible.
2. It makes the remaining assumptions and evidence inspectable.

This leaves two live questions: which additional evidence would justify a
stronger reversibility claim, and how should a resolved project be converted
into a publishable chain without splitting the loader path? The
[migration guide](../guide/migrate.md) covers the current library path, while
[Lens semantics and laws](./lens-laws.md) explains the obligations being tested.

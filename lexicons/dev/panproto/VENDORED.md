# Vendored `dev.panproto.*` lexicons

These `.json` files are verbatim copies of upstream panproto lexicons.
Do not edit them in place — update the pin below and re-vendor.

| field           | value                                      |
|-----------------|--------------------------------------------|
| upstream        | `panproto/panproto` (git)                  |
| commit          | `02158abb80252378a21bb1a9bee839d053a21795` |
| workspace ver.  | `0.39.0`                                   |
| source path     | `lexicons/dev/panproto/`                   |

## Vendored set

Scoped to what the idiolect lens runtime (`crates/idiolect-lens`) needs to
resolve and apply lenses end-to-end, plus the vcs records the
`PanprotoVcsResolver` reads:

- `schema/schema.json` — a panproto schema (`dev.panproto.schema.schema`)
- `schema/lens.json` — a lens between two schemas
- `schema/protolens.json` — a protolens (optic) definition
- `schema/protolensChain.json` — a multi-hop composition
- `schema/complement.json` — the residual from an asymmetric get
- `schema/lensAttestation.json` — governance stance toward a lens
- `schema/theory.json` — a logical theory the protocol is parametric in
- `schema/protocol.json` — a panproto protocol (signature + axioms)
- `vcs/commit.json` — a panproto vcs commit
- `vcs/repo.json` — a panproto vcs repository
- `vcs/refUpdate.json` — a ref-update envelope

## Deliberately not vendored yet

Procedures / queries (`translate/applyLens.json`, `schema/findLenses.json`,
`schema/resolveChain.json`, …) are the *contract* the idiolect-lens runtime
implements, not storage records; they live in upstream panproto and aren't
regenerated here.

Out-of-scope-for-now record types (`editLens`, `symmetricLens`, `theory`,
`theoryMorphism`, `protocol`, `migration`, `expr`) can be vendored when a
downstream feature needs them; the vendor step is mechanical.

## How to refresh

1. bump the commit in this file.
2. `cp` the new upstream `.json` files over the ones listed above.
3. `cargo run -p idiolect-codegen` to regenerate the typed bindings.
4. `cargo test` + `pnpm test` to confirm no downstream break.

# idiolect documentation

This directory contains the source for `idiolect.dev/book/`. The build uses
[mdBook](https://rust-lang.github.io/mdBook/) plus
[mdbook-katex](https://github.com/lzanini/mdbook-katex) for math
and [mdbook-mermaid](https://github.com/badboy/mdbook-mermaid) for
diagrams.

## Build

```bash
cargo install mdbook mdbook-katex mdbook-mermaid
mdbook build
```

The command writes the rendered site to `book/`.

## Validate

Run the three checkers before building. The structural checker parses data and
shell fences, checks local links and anchors, resolves footnotes, enforces
American spelling, and rejects banned prose patterns. The Rust checker compiles
each `rust` fence as an independent binary against the current checkout, while
the Mermaid checker parses and renders every diagram.

```bash
python3 ../../scripts/check_book.py
python3 ../../scripts/check_book_rust.py
bun ../../scripts/check_book_mermaid.mjs
mdbook build
```

## Serve locally

```bash
mdbook serve --open
```

Watches `src/` and `theme/` and live-reloads.

## Deploy

`.github/workflows/book.yml` runs after a push to `main` changes
`docs/book/` or the workflow. It builds the book and syncs the rendered tree
to `/book/` in `idiolect-dev/idiolect-dev.github.io`. That repository serves
the `idiolect.dev` custom domain from its `main` branch, so the next Pages
refresh publishes the result at `idiolect.dev/book/`.

Operator setup (one-time):

1. Create a fine-grained personal access token scoped to
   `idiolect-dev/idiolect-dev.github.io` with `contents: write`.
2. Add it as a repo secret on `idiolect-dev/idiolect` named
   `LANDING_PAGE_PAT`. The workflow reads it to push back into
   the landing-page repo.

## Structure

The book follows the [Diátaxis](https://diataxis.fr/) structure:

| Section | Purpose |
| --- | --- |
| `src/tutorial/` | Linear walkthrough of one example. |
| `src/guide/` | Task-oriented "how do I X?" guides. |
| `src/concepts/` | Conceptual explanation of the model. |
| `src/reference/` | Per-symbol detail (crates, lexicons, CLI, HTTP API). |

The landing page and `src/paths.md` provide cross-quadrant beginner,
project-integration, and advanced/formal routes. `src/glossary.md` supplies the
stable terminology anchors used by each quadrant.

## Style

Use direct, factual prose. Name each mechanism before reasoning over it, scope
claims to what the implementation supports, and end procedural sections with
the resulting state rather than a generic summary. The reference points are
[didactic](https://github.com/aaronstevenwhite/didactic) and
[quivers](https://github.com/FACTSlab/quivers).

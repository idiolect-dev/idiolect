# idiolect documentation

This is the source for `idiolect.dev/book/`. Built with
[mdBook](https://rust-lang.github.io/mdBook/) plus
[mdbook-katex](https://github.com/lzanini/mdbook-katex) for math
and [mdbook-mermaid](https://github.com/badboy/mdbook-mermaid) for
diagrams.

## Build

```bash
cargo install mdbook mdbook-katex mdbook-mermaid
mdbook build
```

Output lands in `book/`.

## Serve locally

```bash
mdbook serve --open
```

Watches `src/` and `theme/` and live-reloads.

## Deploy

`.github/workflows/book.yml` builds on every push to `main` that
touches `docs/book/` (or the workflow itself) and syncs the
rendered tree into `idiolect-dev/idiolect-dev.github.io` under
`/book/`. That sibling repo owns the `idiolect.dev` custom domain
on its GitHub Pages site and serves from its `main` branch root,
so a commit lands as `idiolect.dev/book/...` on the next Pages
refresh.

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

## Style

Hard-nosed and factual. Mirrors the style of
[didactic](https://github.com/aaronstevenwhite/didactic) and
[quivers](https://github.com/FACTSlab/quivers).

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
touches `docs/book/` (or the workflow itself) and deploys to
GitHub Pages. The artifact is packed so the rendered tree is
served at `idiolect.dev/book/`, with the bare custom-domain root
redirecting there.

For the workflow to actually publish:

1. **Enable Pages** for this repo: Settings → Pages → "Build and
   deployment" source = "GitHub Actions".
2. **Point DNS** at `<owner>.github.io` per GitHub's
   [custom-domain docs](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site).
   The workflow's artifact carries a `CNAME` file with
   `idiolect.dev`, so Pages will pick it up on first deploy.

If something other than GitHub Pages currently serves
`idiolect.dev`, the workflow as written would conflict; route only
`idiolect.dev/book/*` to the Pages CNAME at the DNS / edge layer
instead, or change the workflow to push the built tree somewhere
else.

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

<!--
Thanks for contributing. Before filling this out:

- If your change edits files under `lexicons/`, run
  `cargo run -p idiolect-codegen -- generate` locally and commit the
  regenerated sources. The drift gate in CI will reject the PR
  otherwise.
- If your change edits files under `<crate>-spec/`, the same
  applies — codegen regenerates the corresponding crate's
  `src/generated/` output.
- Lexicon edits also face `check-compat` in CI; breaking schema
  changes need an explicit ratification step per the project's
  stewardship process.
-->

## Summary

<!-- One or two sentences on what and why. -->

## Change class

<!-- Tick the one that matches. Influences review cadence and which
     CI gates run. -->

- [ ] bug fix
- [ ] feature (non-breaking)
- [ ] refactor (no behavior change)
- [ ] documentation only
- [ ] release scaffolding / CI / build
- [ ] schema change (lexicon edit — will be classified by check-compat)

## Testing

<!-- Summarize what you ran locally. For a schema change: list the
     affected crates and confirm `cargo test -p <crate>` passes for
     each. -->

## Architecture notes

<!-- Optional. For changes that touch architectural primitives
     (lexicons, spec files, traits at crate boundaries), note any
     design concerns worth flagging to reviewers. Skip for small
     bug fixes and internal refactors. -->

## Checklist

- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` pass locally.
- [ ] `cargo test --workspace` passes locally.
- [ ] `bun run lint && bun run typecheck && bun run test` pass if the change touches TypeScript.
- [ ] Generated sources are up-to-date (for lexicon / spec changes).
- [ ] Relevant sections of `CHANGELOG.md` under `[Unreleased]` updated.

# Idiolect for a new community

You do not need to know ATProto, category theory, or Rust to begin. Idiolect helps a group answer a practical question: **how can we change shared definitions without hiding who decided, what existing data is affected, or how people can leave?**

Four terms carry most of the work:

- A **community** is a group plus the rules and authorities responsible for its shared definitions.
- A **change packet** keeps one proposal together with its compatibility report, participant-facing consequences, reviews, verification evidence, and rollback plan.
- A **lens** is a checked translation between two data shapes. It may preserve all information, preserve only one direction, or require review for cases it cannot translate.
- A **release** is an immutable index of approved changes and exact artifacts, signed by accountable authorities.

This gives us the **community release thread (CRT)**: define the community rules, propose a change, inspect its consequences, review and verify it, sign a release, migrate records, and retain a portable export. Each later step cites evidence from earlier steps.

## Choose an interface

Use [Fieldwork](https://idiolect.dev/fieldwork/) if you want a browser interface. Open **Community workbench** and follow its numbered release thread. It starts with plain-language tasks; **Inspect** rows reveal evidence and operations, while **Interoperate** rows reveal schema JSON, digests, signatures, and protocol records.

Use the `idiolect` command if you want files that can be reviewed in Git, automated in CI, or operated without a browser. The next four pages use that route and explain every artifact as it appears.

## What stays on your machine

A workspace is an ordinary directory. Its manifest, change history, migration checkpoints, release bundles, keys, and exports remain under your control. Nothing in `idiolect init`, `propose`, `review`, `release`, `migrate`, or `export` publishes to a PDS by itself.

## The path ahead

First, [create a workspace](./first-workspace.md). Second, [make Panproto explain a change](./first-change.md). Third, [review and sign a release](./review-release.md). Fourth, [operate the migration and test the exit](./migrate-export.md).

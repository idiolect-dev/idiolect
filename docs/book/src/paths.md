# Choose a reading path

The book retains four [Diátaxis](https://diataxis.fr/) sections because each
serves a different kind of work: the tutorial teaches through a sequence, the
guides give procedures, the concepts explain the model, and the reference pages
record exact contracts. The paths below cross those sections without merging
their functions.

## Beginner: resolve and validate a record

Start with [Install the CLI and fetch a lens](./tutorial/01-install.md). It produces
a concrete result with the checked-out CLI, then points to
[Validate a typed record](./tutorial/02-validate.md). Continue through the
tutorial only when you want to apply and verify a lens or publish a
recommendation.

## Intermediate: integrate a project

Begin with the task closest to your project:

- [Run codegen](./guide/codegen.md) to derive Rust and TypeScript surfaces from
  a Lexicon family.
- [Author a community vocabulary](./guide/vocabulary.md) to add governed,
  machine-readable terms.
- [Publish and resolve a lens](./guide/publish-lens.md) to put a translation on
  the network.
- [Index a firehose](./guide/index-firehose.md) to consume repository events.
- [Run the orchestrator HTTP API](./guide/orchestrator.md) to query indexed
  records.

Use the corresponding [crate](./reference/crates/index.md),
[Lexicon](./reference/lexicons/index.md), [CLI](./reference/cli.md), or
[HTTP API](./reference/http-api.md) page when implementation work requires an
exact signature or wire contract.

## Advanced: extend or analyze the system

Read [Lens semantics and laws](./concepts/lens-laws.md) before changing lens
execution or verification, and read [The vocabulary knowledge
graph](./concepts/vocab-graph.md) before extending vocabulary inference. The
[Observer protocol](./concepts/observer.md) and [Lexicon evolution
policy](./concepts/lexicon-evolution.md) connect those formal objects to runtime
behavior. Then use the [crate reference](./reference/crates/index.md) to find
extension traits and the [stability policy](./reference/stability.md) to decide
which contracts downstream code may rely on.

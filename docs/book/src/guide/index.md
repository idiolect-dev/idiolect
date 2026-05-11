# Guides

Each guide answers one question of the form "how do I do X?". They
assume you have read the [Tutorial](../tutorial/index.md) (or are
comfortable working without it) and have the runtime installed.

| Guide | When to reach for it |
| --- | --- |
| [Index a firehose](./index-firehose.md) | You want to stream commits from a PDS firehose into your own indexer. |
| [Run the orchestrator HTTP API](./orchestrator.md) | You want a read-only query surface over cataloged records. |
| [Run the observer daemon](./observer.md) | You want to fold encounter-family records into observation records. |
| [Author a verification runner](./verify.md) | You want to add a new property kind to the verifier. |
| [Publish and resolve a lens](./publish-lens.md) | You have a panproto lens and want it on the network. |
| [Migrate records across a revision](./migrate.md) | A schema you depend on changed; you want to lift records across the change. |
| [Configure OAuth sessions](./oauth.md) | You want a session store the publishing path can use. |
| [Run codegen](./codegen.md) | You edited a lexicon or a spec and need the generated tree refreshed. |
| [Author a community vocabulary](./vocabulary.md) | You want to extend an open enum or publish a typed knowledge graph. |
| [Bundle records into a dialect](./dialect.md) | You want to ship a coherent set of idiolects as one canonical bundle. |

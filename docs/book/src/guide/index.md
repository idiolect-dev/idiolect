# Guides

The guides start from an integration task and end with an operational
result. If you have not yet resolved and translated a record, begin
with the [tutorial](../tutorial/index.md).

## Project-integration path

For a first application, take these steps in order:

1. [Index a firehose](./index-firehose.md) into your own handler and
   choose a cursor store.
2. [Run the orchestrator HTTP API](./orchestrator.md) over the indexed
   catalog.
3. [Configure authenticated sessions](./oauth.md) before adding a
   record-publishing path.

This path connects the read side, query side, and authenticated write
side without changing the function of any individual guide.

## Task index

| Guide | When to reach for it |
| --- | --- |
| [Index a firehose](./index-firehose.md) | You want to stream commits from a PDS firehose into your own indexer. |
| [Run the orchestrator HTTP API](./orchestrator.md) | You want a read-only query surface over cataloged records. |
| [Run the observer daemon](./observer.md) | You want to fold encounter-family records into observation records. |
| [Author a verification runner](./verify.md) | You want to add a new property kind to the verifier. |
| [Publish and resolve a lens](./publish-lens.md) | You have a panproto lens and want it on the network. |
| [Migrate records across a revision](./migrate.md) | A schema you depend on changed, and you want to lift records across the change. |
| [Configure OAuth sessions](./oauth.md) | You want a session store the publishing path can use. |
| [Run codegen](./codegen.md) | You edited a lexicon or a spec and need the generated tree refreshed. |
| [Author a community vocabulary](./vocabulary.md) | You want to extend an open enum or publish a typed knowledge graph. |
| [Bundle records into a dialect](./dialect.md) | You want to ship a coherent set of idiolects as one canonical bundle. |

For extension traits, feature flags, wire shapes, and endpoint
parameters, use the [advanced reference path](../reference/index.md#extension-and-api-path).

# dev.idiolect.adapter

A published [record](../../glossary.md#record) that declares how to
invoke a framework wrapper and what isolation it requests. The
shipped orchestrator catalogs these declarations; it does not execute
adapters or enforce their isolation policies. A separate adapter host
must interpret and enforce the contract.

> **Source:** [`lexicons/dev/idiolect/adapter.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/adapter.json)
> · **Rust:** [`idiolect_records::Adapter`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Adapter.html)
> · **TS:** `@idiolect-dev/schema/adapter`
> · **Fixture:** `idiolect_records::examples::adapter`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `framework` | string (at most 128 characters) | yes | Canonical framework name (e.g. `hasura`, `prisma`, `coq`). |
| `versionRange` | string | yes | Semver range supported. |
| `invocationProtocol` | object | yes | How the adapter is invoked. |
| `isolation` | object | yes | Sandboxing requirements an adapter host is expected to enforce. |
| `author` | did | yes | DID of the adapter author. |
| `verification` | at-uri | no | Optional verification record demonstrating conformance. |
| `occurredAt` | datetime | yes | Publication timestamp. |

### `invocationProtocol`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | open enum | yes | `subprocess` / `http` / `wasm`. |
| `kindVocab` | `vocabRef` | no | Vocab the kind slug resolves against. |
| `entryPoint` | string | no | Binary name (subprocess), URL (http), or WASM module reference. |
| `inputSchema` | `schemaRef` | no | Schema of the adapter's input. |
| `outputSchema` | `schemaRef` | no | Schema of the adapter's output. |

### `isolation`

| Subfield | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | open enum | yes | `none` / `process` / `container` / `vm` / `wasm-sandbox`. |
| `kindVocab` | `vocabRef` | no | Vocab the kind slug resolves against. |
| `networkPolicy` | open enum | no | `none` / `egress-denylist` / `egress-allowlist` / `full`. |
| `networkPolicyVocab` | `vocabRef` | no | Vocab the policy slug resolves against. |
| `filesystemPolicy` | open enum | no | `readonly` / `scratch` / `writable-subtree` / `full`. |
| `filesystemPolicyVocab` | `vocabRef` | no | Vocab the policy slug resolves against. |
| `resourceLimits` | `{ maxMemoryBytes?, maxCpuSeconds?, maxWallSeconds? }` | no | Requested ceilings for the adapter host. |

## Field details

### Contract semantics

The publisher asserts that a framework in `versionRange` supports
the declared invocation protocol under the requested isolation
policy. Cataloging the record does not verify that assertion.
Consumers may use linked verification records to evaluate specific
conformance claims before passing the record to an adapter host.

### `invocationProtocol.kind`

The transport an adapter host uses:

| Slug | What it means |
| --- | --- |
| `subprocess` | Fork `entryPoint` as a child process and exchange JSON over standard input and output. |
| `http` | Send JSON to the URL at `entryPoint`. |
| `wasm` | Instantiate the WebAssembly module at `entryPoint` and call the host-defined export. |

The slug is open-enum: a community publishing a vocab with an
additional kind (e.g. `nats-rpc`, `grpc-stream`) extends the
transport set without modifying the lexicon.

### `isolation.kind`

The sandboxing posture requested from an adapter host:

| Slug | What it means |
| --- | --- |
| `none` | Run in the orchestrator's own process. Only safe for fully-trusted code. |
| `process` | Fork into a separate process; OS-level isolation. |
| `container` | Run in a container (Docker, Podman, Firecracker microVM). |
| `vm` | Run in a full VM. |
| `wasm-sandbox` | Run in a WASM runtime with capability-based access. |

The lexicon does not define an ordering among these values or a
minimum-isolation policy. A host that treats them as ordered must
document that local policy.

### Network and filesystem policies

Orthogonal axes layered on top of the kind:

| `networkPolicy` | What it means |
| --- | --- |
| `none` | No network access. |
| `egress-denylist` | Network access except to listed denied hosts. |
| `egress-allowlist` | Network access only to listed allowed hosts. |
| `full` | Unrestricted. |

| `filesystemPolicy` | What it means |
| --- | --- |
| `readonly` | The adapter sees a read-only mount. |
| `scratch` | The adapter writes to a scratch directory cleaned up after each invocation. |
| `writable-subtree` | The adapter writes to a designated subtree. |
| `full` | Unrestricted. |

These fields declare requested capabilities; enforcement depends on
the adapter host and its isolation runtime. Consumers should not
infer enforcement from the existence of the record.

### `resourceLimits`

Requested ceilings for a host that supports resource accounting:

| Field | Unit |
| --- | --- |
| `maxMemoryBytes` | RAM, in bytes. |
| `maxCpuSeconds` | CPU time, in seconds. |
| `maxWallSeconds` | Wall-clock time, in seconds. |

A consumer running an untrusted adapter sets all three.

### `verification`

An optional pointer to a `dev.idiolect.verification` record
demonstrating conformance. A consumer that wants to trust an
adapter's claim about its `inputSchema` / `outputSchema` looks for
a `conformance-test` verification (see
[`verification`](./verification.md)).

## Example

```json
{
  "$type": "dev.idiolect.adapter",
  "framework": "hasura",
  "versionRange": "^2.30",
  "invocationProtocol": {
    "kind": "http",
    "entryPoint": "https://hasura.example/v1/graphql",
    "inputSchema":  { "uri": "at://did:plc:adapter-author/dev.panproto.schema.schema/hasura-input" },
    "outputSchema": { "uri": "at://did:plc:adapter-author/dev.panproto.schema.schema/hasura-output" }
  },
  "isolation": {
    "kind": "container",
    "networkPolicy": "egress-allowlist",
    "filesystemPolicy": "scratch",
    "resourceLimits": {
      "maxMemoryBytes": 1073741824,
      "maxCpuSeconds": 30,
      "maxWallSeconds": 60
    }
  },
  "author": "did:plc:adapter-author",
  "occurredAt": "2026-04-19T00:00:00.000Z"
}
```

## Concept references

- [Concepts: The dev.idiolect.* lexicon family](../../concepts/lexicon-family.md)
- [Lexicons: verification](./verification.md) · [bounty](./bounty.md) (`wantAdapter`)

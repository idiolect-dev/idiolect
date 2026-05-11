# dev.idiolect.adapter

A subprocess or HTTP-endpoint wrapper for a framework's tooling,
authored by incentive-aligned parties. Adapters are how idiolect
glues existing frameworks (Hasura, Prisma, Datomic, FHIR, Coq,
Meilisearch, ...) without forking them. The substrate publishes
the adapter declaration; the orchestrator runs the adapter under
the declared isolation policy.

> **Source:** [`lexicons/dev/idiolect/adapter.json`](https://github.com/idiolect-dev/idiolect/blob/main/lexicons/dev/idiolect/adapter.json)
> · **Rust:** [`idiolect_records::Adapter`](https://docs.rs/idiolect-records/latest/idiolect_records/struct.Adapter.html)
> · **TS:** `@idiolect-dev/schema/adapter`
> · **Fixture:** `idiolect_records::examples::adapter`

## Shape

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `framework` | string (≤128) | yes | Canonical framework name (e.g. `hasura`, `prisma`, `coq`). |
| `versionRange` | string | yes | Semver range supported. |
| `invocationProtocol` | object | yes | How the adapter is invoked. |
| `isolation` | object | yes | Sandboxing requirements the orchestrator must honour. |
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
| `resourceLimits` | `{ maxMemoryBytes?, maxCpuSeconds?, maxWallSeconds? }` | no | Hard ceilings the orchestrator enforces. |

## Field details

### Why an adapter is a record

An adapter is a *declared contract*: the publisher asserts that
"this framework, at this version range, can be invoked via this
protocol under this isolation policy". The orchestrator running
the adapter trusts the contract only as far as it trusts the
publisher's signature; verification records can pin specific
conformance claims.

The alternative (each orchestrator hand-coding adapter wrappers
per framework) does not scale. The adapter record is the
declarative replacement: a community with framework expertise
publishes the wrapper once; orchestrators pick it up from the
network.

### `invocationProtocol.kind`

The transport over which the orchestrator drives the adapter:

| Slug | What it means |
| --- | --- |
| `subprocess` | The orchestrator forks `entryPoint` as a child process and pipes JSON over stdin/stdout. |
| `http` | The orchestrator POSTs JSON to the URL at `entryPoint`. |
| `wasm` | The orchestrator instantiates the WASM module at `entryPoint` and calls a designated export. |

The slug is open-enum: a community publishing a vocab with an
additional kind (e.g. `nats-rpc`, `grpc-stream`) extends the
transport set without modifying the lexicon.

### `isolation.kind`

The sandboxing posture the orchestrator must honour:

| Slug | What it means |
| --- | --- |
| `none` | Run in the orchestrator's own process. Only safe for fully-trusted code. |
| `process` | Fork into a separate process; OS-level isolation. |
| `container` | Run in a container (Docker, Podman, Firecracker microVM). |
| `vm` | Run in a full VM. |
| `wasm-sandbox` | Run in a WASM runtime with capability-based access. |

The orchestrator's policy is to refuse any adapter whose
`isolation.kind` is weaker than its configured floor. An
orchestrator configured for `container` minimum will not run an
adapter declaring `process`.

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

The orchestrator's enforcement is best-effort and depends on the
underlying isolation runtime; e.g. `wasm-sandbox` makes
`egress-allowlist` cheap and exact, `process` makes it harder.

### `resourceLimits`

Hard ceilings. The orchestrator kills the adapter if it exceeds
any of:

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

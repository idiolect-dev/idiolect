# The community control plane

Idiolect 0.13 adds a **community control plane (CCP)**: a portable set of records and local artifacts that connects definition changes to authority, evidence, release, migration, federation, and exit.

The name distinguishes two kinds of work. The data plane carries community records and applies lenses. The control plane answers who may change the definitions, what effects the change has, whether required evidence holds, which release contains it, and how records move or leave.

## One lifecycle, three representations

The same community process appears in three representations:

1. A local workspace stores `idiolect.toml`, schema packages, change packets, migration runs, keys, releases, and exports.
2. Four `dev.idiolect.*` records publish the interoperable state: `changeProposal`, `communityRelease`, `migrationRun`, and `federation`.
3. Fieldwork presents these objects as a progressively disclosed release thread for participants who should not need to begin with protocol JSON.

The representations overlap, but they are not identical. A local change packet may include source paths and draft evidence that should not be published. The protocol record carries stable identifiers, digests, consequences, review state, and references. The Fieldwork workspace additionally retains form state and raw definitions so a browser-only community can work before it publishes.

## Consequences before consent

Panproto supplies structural evidence: a full schema diff, protocol-specific compatibility class, and, when derivable, a protolens with an optic class. Idiolect adds a **consequence layer (CL)** that restates this evidence in terms of existing records, old readers, required participant input, complements, manual migration, and possible data loss.

This separation matters. Technical classifications remain available for inspection, but a participant can evaluate a proposal without first learning the representation Panproto uses internally.

## Three-state verification

Verification outcomes are `verified`, `refuted`, and `incomplete`. The third state is not a softer success. It represents a check that did not establish its claim, for instance because a job timed out or only part of a corpus ran.

The release gate rejects both `refuted` and `incomplete` evidence. It also requires at least one verified result for each verification kind named in the governance policy.

## Plural governance over common evidence

The workspace supports maintainer, consent, vote, steward, and hybrid decision models. A common packet and review log thus do not force a single political structure. The policy states how evidence becomes a decision; the packet preserves the inputs and the resulting reason.

Breaking or data-loss-risking changes may require steward approval even when the primary model would otherwise approve them. This is the **consequence escalation rule (CER)**.

## Federation without equivalence by assertion

A federation relationship names a social or operational relation such as `follows`, `extends`, `bridges`, or `forked-from`. Mapping or lens URIs are separate. A relationship with no mappings is recognition or lineage, not evidence that two communities mean the same thing.

## Exit as a routine operation

Portable export rejects symbolic links and non-empty destinations, copies only the material selected by the exit policy, and writes an inventory of SHA-256 digests. Communities can thus rehearse exit while relations are healthy. The [ordinary exit test](../start/migrate-export.md) makes portability a property that can be checked rather than a promise that matters only during conflict.

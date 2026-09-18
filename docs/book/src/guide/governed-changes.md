# Govern definition changes

The change packet is the unit of review. It keeps intent, exact endpoints, consequences, governance state, and verification evidence together so a release cannot cite approval without its basis.

## Analysis boundary

`idiolect propose` reads the source and target within the workspace's schema-byte budget. It obtains the protocol through Panproto's canonical registry, parses both documents through `parse_schema_document_within`, computes forward and reverse compatibility, attempts automatic protolens construction, classifies the optic, and hashes the original bytes.

Automatic lens construction may fail while the compatibility report remains useful. The packet then sets `requiresManualChain` and explains that a curator must supply or review a migration chain.

## Consequence report

The report exposes:

- `compatibility`: fully compatible, backward compatible, or breaking;
- `opticClass`: iso, lens, prism, affine, or traversal when derived;
- whether existing records remain readable;
- whether old readers accept new records;
- whether reversal requires a complement;
- whether a manual chain is required;
- whether the change may lose information;
- machine-detected changes and participant-facing messages.

Treat these as evidence, not a decision. A backward-compatible addition may still violate a privacy convention. A breaking rename may be acceptable when the community has a complete, reversible migration.

## Review identity and roles

The CLI accepts reviews only from DIDs listed under `[[authorities]]`, and the submitted role must appear in that authority's role list. A later review from the same DID and role replaces the earlier one, so a participant can revise a decision without manufacturing extra votes.

## Governance models

| Model | Approval rule |
|---|---|
| `maintainer` | `minApprovals` approving maintainers |
| `consent` | quorum plus `minApprovals`, with no rejection |
| `vote` | quorum, minimum approvals, and `approvalThreshold` among decisive votes |
| `steward` | one approving steward |
| `hybrid` | maintainer threshold and steward approval |

Abstentions count as participation for quorum but not as an approval or rejection. If consequences trigger `stewardReviewFor`, an approving steward is required before the primary rule runs to completion.

## Verification gate

Attach evidence with `idiolect verify change`. Repeating the same verification kind and tool replaces the earlier result. This allows a failed or incomplete run to be superseded by a later complete run without losing the claim's identity.

The release gate applies two rules. First, any refuted or incomplete evidence blocks release. Second, every kind in `governance.requiredVerifications` must have a verified result. A custom check should thus use a stable kind and a tool identifier that includes its semantic version.

## Deliberation links

The local decision object has an optional deliberation link, and the published `changeProposal` record may cite a `dev.idiolect.deliberation` at-uri. Use it when discussion itself should be federated. The change packet remains the authoritative release input; the deliberation supplies the inspectable conversational record.

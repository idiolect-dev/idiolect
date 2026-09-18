# Sign releases and run migrations

Releases and migration runs solve different problems. A release fixes what the community approved. A migration run records how existing material moves toward that approved state.

## Canonical releases

`create_release` rejects packets that are not approved or released and applies the verification gate. It includes:

- a copy of the manifest used for the decision;
- packet identifiers and their content digests;
- artifact locations, media types, and SHA-256 digests;
- federation dependencies and selected peer releases;
- a digest of the canonical unsigned bundle;
- detached signatures.

The signed bytes exclude the signatures themselves. This permits additional authorities to sign the same release without changing the subject of earlier signatures.

## Keys and signatures

`idiolect keygen` creates a P-256 signing key. `idiolect release` signs with ES256 and embeds the verifying key. The signer DID must be a manifest authority, and verification counts distinct valid signers rather than raw signature entries.

The current CLI creates one signature per invocation. A policy requiring multiple signatures may assemble additional signatures through the library or a signing coordinator, then call `verify_release`. Do not copy private key material into the release bundle or portable export.

## Durable migration state

A migration run contains its change identifier, optional lens, total when known, processed and failed counts, bounded failure samples, checkpoints, timestamps, and status.

`migrate advance` adds to the processed count. The cursor names the last durable checkpoint, not merely the last record attempted in memory. Resume after that cursor only when downstream writes through the checkpoint are durable.

Status transitions are intentionally explicit:

- `planned` means no records have been committed;
- `running` means work may advance;
- `paused` preserves a resumable checkpoint;
- `completed` asserts that the intended corpus finished;
- `failed` records a terminal unsuccessful run;
- `rolled-back` records that applied effects were reversed.

## Failure samples

The protocol record describes its `failures` as bounded diagnostic samples rather than a complete failed-record corpus. Large or sensitive failure sets should live in community-controlled storage, with the run carrying a digest or location under a local convention. This keeps published operational state useful without turning it into an accidental data leak.

## Observing operations

`MigrationHealthMethod` retains the latest body at each run at-uri and publishes per-community totals for active, completed, and failed runs, processed records, failed records, and known corpus size. `ReleaseAdoptionMethod` publishes release, signed-release, artifact, dependency, and distinct-signer counts. Updates replace prior bodies; deletes remove them from the aggregate.

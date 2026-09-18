# Review and release it

This page records an attributable decision, adds verification evidence, and signs an immutable release.

Set a shell variable so the examples remain readable:

```console
PACKET=neighborhood-archive/.idiolect/changes/<packet>.json
```

## 1. Record a review

```console
idiolect review "$PACKET" \
  --workspace neighborhood-archive \
  --reviewer did:plc:replace-me \
  --role maintainer \
  --stance approve \
  --comment "Optional addition; tested against existing profiles."
```

The command re-evaluates the complete review log under the manifest's governance model. A vote policy applies quorum and approval threshold. Consent additionally remains blocked while any rejection is unresolved. Steward, maintainer, and hybrid policies evaluate the configured roles rather than treating every review as interchangeable.

## 2. Record the missing evidence honestly

Suppose representative-record testing has finished successfully:

```console
idiolect verify change "$PACKET" \
  --kind representative-records \
  --outcome verified \
  --tool profile-fixture-suite/1.0 \
  --evidence reports/profile-v2-fixtures.json
```

If the run stopped halfway, use `--outcome incomplete`. Do not use `verified` to mean that a job started or that some cases passed.

## 3. Generate a release key

```console
idiolect keygen --workspace neighborhood-archive
```

On Unix, Idiolect writes the private key with mode `0600`. The release bundle contains only the public key and detached ES256 signature. Keep `.idiolect/keys/community.json` out of version control and backups shared with participants.

## 4. Sign the release

```console
idiolect release \
  --workspace neighborhood-archive \
  --version 0.1.0 \
  --change "$PACKET" \
  --artifact neighborhood-archive/lexicons/profile-v2.json \
  --key neighborhood-archive/.idiolect/keys/community.json \
  --signer did:plc:replace-me
```

The command rejects unapproved or insufficiently verified changes, signs the canonical unsigned bundle digest, enforces the minimum number of distinct valid authorities, writes the release under `releases/`, and marks included packets as released.

Anyone with the bundle can check it:

```console
idiolect release verify neighborhood-archive/releases/0.1.0.json
```

A changed artifact digest, packet, manifest, dependency, public key, or signature makes verification fail. Next, [operate the migration and make an exit export](./migrate-export.md).

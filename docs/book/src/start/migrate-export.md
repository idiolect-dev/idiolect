# Migrate safely and keep an exit

A release says what the community approved. A migration run records how existing material reaches that release.

## 1. Plan a durable run

```console
idiolect migrate plan \
  --workspace neighborhood-archive \
  --change "$PACKET" \
  --total 250
```

The resulting file under `.idiolect/migrations/` begins in `planned` state. If Panproto derived a published lens, add `--lens at://…`; a manual procedure may omit it.

## 2. Advance by checkpoint

```console
RUN=neighborhood-archive/.idiolect/migrations/<run>.json

idiolect migrate advance "$RUN" \
  --processed 100 \
  --cursor record-100 \
  --status running
```

If one record fails, keep the run moving and retain a bounded diagnostic sample:

```console
idiolect migrate advance "$RUN" \
  --processed 75 \
  --cursor record-175 \
  --failure 'at://did:plc:member/org.example.neighborhood.profile/bad=displayName is absent' \
  --status running
```

Inspect the current totals and last checkpoint:

```console
idiolect migrate status "$RUN"
```

Processed counts may only advance. Failure samples are bounded by the workspace resource policy. Terminal states are explicit: `completed`, `failed`, or `rolled-back`.

## 3. Test the ordinary exit

Choose a new, empty destination:

```console
idiolect export \
  --workspace neighborhood-archive \
  --release neighborhood-archive/releases/0.1.0.json \
  --out neighborhood-archive-export
```

The export rejects symbolic links and non-empty destinations. It copies the manifest, selected definitions, change history, migration runs, releases, and the requested current bundle, then writes `export-inventory.json` with the files and their digests.

Open the export without Idiolect. A participant should be able to identify the community, approved change, evidence, migration state, signed release, and exact artifacts from ordinary files. This is the **ordinary exit test (OET)**: exit works before a conflict makes it urgent.

You have now completed the community release thread. The [community workspace guide](../guide/community-workspaces.md) explains policy variants and automation in more depth.

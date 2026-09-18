# Explain your first change

A change packet begins with intent, not a diff. The diff matters because it tests that intent against existing data.

## 1. Put two definitions in the workspace

Create `neighborhood-archive/lexicons/profile-v1.json`:

```json
{
  "lexicon": 1,
  "id": "org.example.neighborhood.profile",
  "defs": {
    "main": {
      "type": "record",
      "key": "tid",
      "record": {
        "type": "object",
        "required": ["displayName"],
        "properties": {
          "displayName": { "type": "string", "maxLength": 120 }
        }
      }
    }
  }
}
```

Create `profile-v2.json` beside it. Add one optional field under `properties`:

```text
"pronouns": { "type": "string", "maxLength": 80 }
```

## 2. Build the packet

```console
idiolect propose \
  --workspace neighborhood-archive \
  --title "Let profile authors share pronouns" \
  --summary "Add an optional pronouns field. Existing profiles remain valid." \
  --author did:plc:replace-me \
  --old lexicons/profile-v1.json \
  --new lexicons/profile-v2.json \
  --affected "profile readers,member directory" \
  --rollback "Publish profile-v1 as the next release and stop the migration."
```

Idiolect parses both documents through Panproto's canonical `atproto` protocol, applies the workspace resource budget, records content digests, computes the full schema diff, classifies compatibility, and attempts to derive a protolens and optic class.

## 3. Read the participant-facing report

The command prints a short report and writes a JSON packet under `.idiolect/changes/`. Preview it again at any time:

```console
idiolect preview neighborhood-archive/.idiolect/changes/<packet>.json
```

An optional field should be classified as compatible and described as new capability. Try making `pronouns` required. The report should instead explain that older records lack required information and that the community needs a default, participant input, or an explicit set of records that cannot migrate automatically.

This translation is the **consequence layer (CL)**: technical evidence remains intact, but participants receive statements about records, readers, information retention, and operational work.

## 4. Check what the packet does not prove

Schema compatibility is only one verification kind. It does not prove that descriptions are accurate, UI copy is usable, permissions are appropriate, or a migration works on representative records. The packet thus starts with explicit verification evidence, and later checks may append `verified`, `refuted`, or `incomplete` outcomes. Incomplete never passes the release gate.

Next, [review and release the packet](./review-release.md).

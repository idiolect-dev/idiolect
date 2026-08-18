# Publish a recommendation

A [recommendation](../glossary.md#recommendation "Conditional lens endorsement")
records that a community endorses a lens path under stated conditions. The
tutorial publisher creates a small community record for your account, then
publishes a recommendation whose `conditionSourceIs` points at the source
schema used in Chapters 3 and 4.

This chapter writes two public records to your PDS. The earlier chapters were
read-only.

## Create an app password

Use an ATProto account intended for this exercise. Create an app password in
your account settings, then set the PDS URL, handle, and password in your
shell. The example below uses Bluesky's hosted PDS; replace the URL if your
account uses another server.

```bash
export PDS_URL='https://bsky.social'
export ATPROTO_HANDLE='your-handle.bsky.social'
export ATPROTO_PASSWORD='xxxx-xxxx-xxxx-xxxx'
```

Avoid placing a real password in a shared shell history. An app password can
be revoked without changing the account's main password.

## Publish both records

From the repository root:

```bash
cargo run --quiet \
  --manifest-path scripts/publish-tutorial-lens/Cargo.toml \
  --bin publish-tutorial-recommendation
unset ATPROTO_PASSWORD
```

The PDS assigns fresh record keys, so the command prints different
[AT-URIs](../glossary.md#at-uri "AT Protocol record address") for each run:

```text
community      at://did:plc:.../dev.idiolect.community/...
recommendation at://did:plc:.../dev.idiolect.recommendation/...
```

The recommendation contains three load-bearing references: (i) the community
record just published, (ii) the tutorial's source schema as its applicability
condition, and (iii) the tutorial lens as its one-step lens path. The generated
Rust types validate those fields before the publisher sends the record.

## Read the result back

Copy the printed recommendation AT-URI into the CLI:

```bash
idiolect fetch at://did:plc:.../dev.idiolect.recommendation/...
```

The returned JSON should contain `issuingCommunity`, `conditions`, `lensPath`,
and `occurredAt`. This output confirms that the recommendation was published.
Continue with [Publish a lens](../guide/publish-lens.md) when you need to author
the schemas and lens rather than reuse the tutorial records.

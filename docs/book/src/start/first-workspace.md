# Create your first workspace

This page creates a local home for one community's definitions and decision rules.

## 1. Install the command

From a checkout of Idiolect:

```console
cargo install --path crates/idiolect-cli
idiolect version
```

The second command should print `idiolect 0.13.0`.

## 2. Name the community

Use a DID controlled by a person or service accountable to the community. The example DID is deliberately fake; replace it before publishing anything.

```console
mkdir neighborhood-archive
idiolect init \
  --workspace neighborhood-archive \
  --name "Neighborhood Archive" \
  --did did:plc:replace-me
```

The command creates `neighborhood-archive/idiolect.toml` and the managed
directories used for definitions, changes, migrations, releases, and keys.

## 3. Read the manifest as a promise

Open `idiolect.toml`. The important sections are:

- `[community]` names the accountable community identity.
- `[[authorities]]` names who may sign releases and which role they hold.
- `[governance]` selects a decision model, quorum, approval threshold, and review period.
- `[resources]` bounds schema bytes, consequence items, and retained failure samples.
- `[release]` sets the release directory and minimum distinct signatures.
- `[exit]` selects the material copied into a portable export.

The defaults use consent with one maintainer authority. They are a starting point, not a claim that every community should govern this way. Edit the policy before the first consequential decision.

For this local walkthrough only, set `reviewPeriodDays = 0` under
`[governance]` so you can release the change immediately. Keep a nonzero review
period when participants need time to inspect and contest a real proposal.

## 4. Ask the workspace to diagnose itself

```console
idiolect doctor --workspace neighborhood-archive
idiolect check --workspace neighborhood-archive
```

`doctor` explains absent directories, unreadable definitions, unsafe links, and incomplete policy. `doctor --repair` may recreate missing managed directories; it does not invent authorities, rewrite definitions, or overwrite history.

The workspace is ready when `check` exits successfully. Next, [explain a definition change](./first-change.md).

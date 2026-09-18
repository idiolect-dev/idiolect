# Federate and export

Federation lets one community depend on, extend, bridge, or fork another without assigning a global authority over both.

## Relationship and mapping are separate

Each dependency records:

- the peer community record or DID;
- `follows`, `extends`, `bridges`, or `forked-from`;
- an optional exact release or version requirement;
- `review`, `follow-compatible`, or `pinned` update policy;
- zero or more published mapping or lens URIs.

An empty mapping list is valid. It says that a social, operational, or lineage relationship exists without claiming semantic interoperability. `doctor` reports this as information so operators do not confuse recognition with a working bridge.

## Update policies

`review` brings every peer update through the local change process. `follow-compatible` may automate updates that the local compatibility policy accepts, though it should still record the imported release and evidence. `pinned` keeps an exact peer release until the community changes the manifest.

## Release dependencies

Community releases copy federation dependency metadata into the signed bundle. Thus a consumer can reconstruct which peer state the release assumed. Prefer an exact digest for reproducible builds; a version range is useful for policy but does not by itself identify bytes.

## Portable export

`idiolect export` copies material according to `[exit]` and writes two guideposts:

- `README.md` names the community DID and the first diagnostic command;
- `export-inventory.json` maps each copied relative path to a SHA-256 digest.

The inventory omits its own digest because including it would be self-referential. A selected current release may also be written as `community-release.json` at the export root.

Exports do not follow symbolic links, and they refuse to write into a non-empty destination. These rules prevent an apparently narrow export from reading outside the workspace or mixing new evidence with an older export.

## Forking

To fork, copy the export, change the community DID and authorities, set a `forked-from` federation entry, and run `idiolect doctor`. Preserve the source release and change history unless participants have a documented reason to omit them. The new community may adopt different governance without rewriting the evidence that preceded the fork.

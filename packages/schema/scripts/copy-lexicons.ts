#!/usr/bin/env bun
// Copy the workspace-root lexicons/ tree into packages/schema/lexicons/
// so the package directory has a local copy at the path
// `loadLexiconDocs` resolves via `import.meta.url`-relative paths.
//
// Idempotent: clears the destination first so a stale lexicon left
// over from a prior cut doesn't survive a removal upstream.
//
// Used both by `bun run build` (full publish path) and by
// `bun run pretest` (so `bun test` works against `src/` without
// having to run a full bundle first).

import { cpSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(HERE, "..");
const WORKSPACE_LEXICONS = resolve(ROOT, "..", "..", "lexicons");
const LOCAL_LEXICONS = resolve(ROOT, "lexicons");

rmSync(LOCAL_LEXICONS, { recursive: true, force: true });
cpSync(WORKSPACE_LEXICONS, LOCAL_LEXICONS, { recursive: true });

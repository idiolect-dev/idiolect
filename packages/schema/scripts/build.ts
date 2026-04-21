#!/usr/bin/env bun
// compile the schema package.
//
// uses bun's built-in bundler for js and tsc for declaration files.
// bun is ~10x faster than tsc for transpilation but does not yet emit
// .d.ts files (as of bun 1.2), so the two-step split is deliberate.

import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const ROOT = resolve(HERE, "..");
const DIST = resolve(ROOT, "dist");

rmSync(DIST, { recursive: true, force: true });
mkdirSync(DIST, { recursive: true });

// step 1: bun-bundle js from src/index.ts.
const bundle = spawnSync(
  "bun",
  [
    "build",
    "src/index.ts",
    "--outdir",
    "dist",
    "--target",
    "node",
    "--format",
    "esm",
    "--external",
    "@atproto/lexicon",
    "--external",
    "@atproto/syntax",
  ],
  { cwd: ROOT, stdio: "inherit" },
);
if (bundle.status !== 0) process.exit(bundle.status ?? 1);

// step 2: emit .d.ts via tsc.
const tsc = spawnSync(
  "bunx",
  ["tsc", "--emitDeclarationOnly", "--outDir", "dist"],
  { cwd: ROOT, stdio: "inherit" },
);
process.exit(tsc.status ?? 1);

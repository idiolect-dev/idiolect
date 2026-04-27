import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { type LexiconDoc, Lexicons } from "@atproto/lexicon";

// resolve the lexicons/ directory relative to this module.
//
// The package ships the raw json so consumers can rebuild the
// lexicons instance with their own extensions without re-fetching.
// Build-time `scripts/build.ts` copies the workspace-root
// `lexicons/` tree into `packages/schema/lexicons/` so the path
// math is the same in dev (running off `src/`) and after publish
// (running off `dist/`):
//
//   - dev:       src/lexicons.ts → ../lexicons → packages/schema/lexicons/
//   - published: dist/index.js   → ../lexicons → package/lexicons/
//
// `..` from the bundled `dist/index.js` resolves to the package
// root because `import.meta.url` points at the bundled file's
// location, not at the original source path.
const HERE = fileURLToPath(new URL(".", import.meta.url));
const LEXICONS_DIR = join(HERE, "..", "lexicons");

/**
 * Recursively enumerate every .json file under `root`.
 */
function collectJsonFiles(root: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(root)) {
    const path = join(root, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      out.push(...collectJsonFiles(path));
    } else if (stat.isFile() && path.endsWith(".json")) {
      out.push(path);
    }
  }
  return out;
}

/**
 * Load every lexicon json under the given directory and parse it.
 *
 * @param root - root of a lexicons tree. defaults to the tree shipped
 *   with this package.
 */
export function loadLexiconDocs(root: string = LEXICONS_DIR): LexiconDoc[] {
  return collectJsonFiles(root)
    .map((path) => JSON.parse(readFileSync(path, "utf8")) as Record<string, unknown>)
    .filter((doc): doc is LexiconDoc => doc["lexicon"] === 1 && typeof doc["id"] === "string");
}

/**
 * Build a `Lexicons` instance seeded with every dev.idiolect.* schema.
 *
 * The instance is mutable; callers may `.add(...)` extensions or
 * external lexicons after construction.
 */
export function buildLexicons(root?: string): Lexicons {
  const lexicons = new Lexicons();
  for (const doc of loadLexiconDocs(root)) {
    lexicons.add(doc);
  }
  return lexicons;
}

/**
 * Convenience singleton for the default dev.idiolect.* lexicon set.
 *
 * Lazy so that importing this module does not perform disk i/o unless
 * the singleton is actually used.
 */
let cachedLexicons: Lexicons | null = null;
export function defaultLexicons(): Lexicons {
  if (cachedLexicons === null) {
    cachedLexicons = buildLexicons();
  }
  return cachedLexicons;
}

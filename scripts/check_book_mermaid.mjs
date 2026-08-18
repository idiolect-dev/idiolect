#!/usr/bin/env node
/** Parse every Mermaid fence with the exact browser bundle shipped by mdBook. */

import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { JSDOM } from "jsdom";

const root = resolve(dirname(new URL(import.meta.url).pathname), "..");
const bookSource = join(root, "docs", "book", "src");
const mermaidBundle = join(root, "docs", "book", "mermaid.min.js");

function markdownFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return markdownFiles(path);
    return entry.isFile() && entry.name.endsWith(".md") ? [path] : [];
  });
}

const assignment = "globalThis.mermaid = globalThis.__esbuild_esm_mermaid.default;";
const replacement = "globalThis.mermaid = __esbuild_esm_mermaid.default;";
const source = readFileSync(mermaidBundle, "utf8");
if (!source.includes(assignment)) {
  throw new Error(`unexpected Mermaid bundle wrapper in ${mermaidBundle}`);
}

const temporary = mkdtempSync(join(tmpdir(), "idiolect-mermaid-"));
const modulePath = join(temporary, "mermaid.mjs");
writeFileSync(
  modulePath,
  `${source.replace(assignment, replacement)}\nexport default globalThis.mermaid;\n`,
);

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "https://idiolect.test/",
});
for (const name of [
  "document",
  "Element",
  "HTMLElement",
  "SVGElement",
  "Node",
  "DOMParser",
  "navigator",
  "localStorage",
]) {
  Object.defineProperty(globalThis, name, {
    configurable: true,
    value: dom.window[name],
  });
}
Object.defineProperty(globalThis, "window", {
  configurable: true,
  value: dom.window,
});
dom.window.SVGElement.prototype.getBBox = () => ({
  height: 20,
  width: 100,
  x: 0,
  y: 0,
});
dom.window.SVGElement.prototype.getComputedTextLength = () => 100;

try {
  const { default: mermaid } = await import(pathToFileURL(modulePath).href);

  let count = 0;
  const errors = [];
  const fencePattern = /^```mermaid\s*\n([\s\S]*?)^```\s*$/gm;
  for (const path of markdownFiles(bookSource).sort()) {
    const markdown = readFileSync(path, "utf8");
    for (const match of markdown.matchAll(fencePattern)) {
      count += 1;
      const line = markdown.slice(0, match.index).split("\n").length;
      try {
        await mermaid.parse(match[1], { suppressErrors: false });
        const { svg } = await mermaid.render(`book-mermaid-${count}`, match[1]);
        if (!svg.startsWith("<svg")) {
          throw new Error("Mermaid renderer did not return an SVG document");
        }
        dom.window.document.body.replaceChildren();
      } catch (error) {
        errors.push(`${path.slice(root.length + 1)}:${line}: ${error}`);
      }
    }
  }

  process.stdout.write(`Mermaid fences: ${count}\n`);
  if (errors.length > 0) {
    for (const error of errors) process.stderr.write(`${error}\n`);
    process.exitCode = 1;
  } else {
    process.stdout.write("Mermaid parsing and SVG rendering passed\n");
  }
} finally {
  dom.window.close();
  rmSync(temporary, { recursive: true, force: true });
}

#!/usr/bin/env python3
"""Run deterministic structural checks over the mdBook source."""

from __future__ import annotations

import argparse
import collections
import json
import re
import shlex
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1]
BOOK = ROOT / "docs" / "book" / "src"


@dataclass(frozen=True)
class Fence:
    path: Path
    line: int
    info: str
    body: str

    @property
    def language(self) -> str:
        return self.info.split(",", 1)[0].split(None, 1)[0].lower()


BANNED = (
    r"\bdelve\b",
    r"\btapestry\b",
    r"\brealm\b",
    r"\blandscape\b",
    r"\bseamless(?:ly)?\b",
    r"\bpivotal\b",
    r"\bnuanced\b",
    r"\bharness(?: the power)?\b",
    r"\bfoster\b",
    r"\bunderscore(?:s|d|ing)?(?: the importance)?\b",
    r"\bshed(?:s|ding)? light on\b",
    r"\bmyriad\b",
    r"\bplethora\b",
    r"\bmultifaceted\b",
    r"\bcutting-edge\b",
    r"\bgame-changer\b",
    r"\butili[sz]e(?:s|d|ing)?\b",
    r"\bit(?:'s| is) (?:worth|important) to note\b",
    r"\bthis section explores\b",
    r"\bthis distinction matters\b",
    r"\bnavigate the complexities\b",
    r"\bpav(?:e|es|ed|ing) the way\b",
    r"\btestament to\b",
    r"\btreasure trove\b",
    r"\bin today's world\b",
    r"\bever-evolving\b",
    r"\bwithout a doubt\b",
    r"\bit goes without saying\b",
    r"\bclearly\b",
    r"\bobviously\b",
    r"\bundoubtedly\b",
)

BRITISH = {
    "behaviour": "behavior",
    "behaviours": "behaviors",
    "catalogue": "catalog",
    "catalogues": "catalogs",
    "colour": "color",
    "colours": "colors",
    "factorisation": "factorization",
    "fibre": "fiber",
    "fibres": "fibers",
    "judgement": "judgment",
    "judgements": "judgments",
    "labelled": "labeled",
    "materialised": "materialized",
    "modelled": "modeled",
    "neighbour": "neighbor",
    "neighbours": "neighbors",
    "normalised": "normalized",
    "optimisation": "optimization",
    "optimisations": "optimizations",
    "organisation": "organization",
    "organisations": "organizations",
    "parameterised": "parameterized",
    "pluralised": "pluralized",
    "recognised": "recognized",
    "serialised": "serialized",
    "summarising": "summarizing",
}


def display(path: Path) -> str:
    return str(path.relative_to(ROOT))


def extract_fences(path: Path, text: str, errors: list[str]) -> list[Fence]:
    fences: list[Fence] = []
    open_line: int | None = None
    info = ""
    body: list[str] = []
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("```"):
            if open_line is None:
                open_line = number
                info = line[3:].strip()
                body = []
            elif line.strip() == "```":
                fences.append(Fence(path, open_line, info, "\n".join(body) + "\n"))
                open_line = None
                info = ""
                body = []
            else:
                body.append(line)
        elif open_line is not None:
            body.append(line)
    if open_line is not None:
        errors.append(f"{display(path)}:{open_line}: unclosed code fence")
    return fences


def prose_without_code(text: str) -> str:
    text = re.sub(r"^```.*?^```\s*$", "", text, flags=re.MULTILINE | re.DOTALL)
    text = re.sub(r"`[^`\n]+`", "", text)
    text = re.sub(r"https?://\S+", "", text)
    return text


def slugify(heading: str) -> str:
    heading = re.sub(r"!?(?:\[([^]]+)\])\([^)]+\)", r"\1", heading)
    heading = re.sub(r"[`*_~]", "", heading).strip().lower()
    heading = re.sub(r"[^\w\- ]", "", heading)
    return re.sub(r"[ ]+", "-", heading)


def anchors(path: Path, text: str) -> set[str]:
    result = {
        slugify(match.group(1))
        for match in re.finditer(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.MULTILINE)
    }
    result.update(re.findall(r"<(?:a|span)\s+(?:name|id)=[\"']([^\"']+)", text))
    return result


def validate_link(
    source: Path,
    raw_destination: str,
    anchor_map: dict[Path, set[str]],
    errors: list[str],
) -> None:
    try:
        parts = shlex.split(raw_destination)
    except ValueError as exc:
        errors.append(f"{display(source)}: malformed link destination {raw_destination!r}: {exc}")
        return
    if not parts:
        return
    destination = unquote(parts[0].strip("<>"))
    if re.match(r"^(?:https?|mailto|tel):", destination) or destination.startswith("/"):
        return
    target_part, _, fragment = destination.partition("#")
    target = source if not target_part else (source.parent / target_part).resolve()
    if target.suffix == ".html":
        target = target.with_suffix(".md")
    if target.is_dir():
        target /= "index.md"
    if not target.exists():
        errors.append(f"{display(source)}: missing local link target {destination!r}")
        return
    if fragment and target.suffix == ".md" and fragment not in anchor_map.get(target, set()):
        errors.append(
            f"{display(source)}: missing anchor #{fragment} in {display(target)}"
        )


def validate_fence(fence: Fence, errors: list[str]) -> None:
    label = f"{display(fence.path)}:{fence.line}"
    language = fence.language
    if language == "rust" and "ignore" in {
        flag.strip() for flag in fence.info.split(",")[1:]
    }:
        errors.append(f"{label}: rust,ignore is prohibited")
    try:
        if language == "json":
            json.loads(fence.body)
        elif language == "toml":
            tomllib.loads(fence.body)
        elif language in {"bash", "sh", "shell"}:
            completed = subprocess.run(
                ["bash", "-n"],
                input=fence.body,
                text=True,
                capture_output=True,
                check=False,
            )
            if completed.returncode:
                raise ValueError(completed.stderr.strip())
        elif language == "python":
            compile(fence.body, label, "exec")
        elif language in {"yaml", "yml"}:
            try:
                import yaml  # type: ignore[import-not-found]
            except ImportError as exc:
                raise ValueError("PyYAML is required to validate YAML fences") from exc
            yaml.safe_load(fence.body)
    except (ValueError, json.JSONDecodeError, tomllib.TOMLDecodeError, SyntaxError) as exc:
        errors.append(f"{label}: invalid {language} fence: {exc}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--require-no-rust",
        action="store_true",
        help="fail when a Rust fence remains outside the compiled-example harness",
    )
    args = parser.parse_args()

    errors: list[str] = []
    markdown = sorted(BOOK.rglob("*.md"))
    texts = {path.resolve(): path.read_text(encoding="utf-8") for path in markdown}
    anchor_map = {path: anchors(path, text) for path, text in texts.items()}
    fences: list[Fence] = []

    for path, text in texts.items():
        file_fences = extract_fences(path, text, errors)
        fences.extend(file_fences)
        prose = prose_without_code(text)

        for pattern in BANNED:
            for match in re.finditer(pattern, prose, re.IGNORECASE):
                line = prose.count("\n", 0, match.start()) + 1
                errors.append(
                    f"{display(path)}:{line}: banned prose {match.group(0)!r}"
                )

        for british, american in BRITISH.items():
            for match in re.finditer(rf"\b{british}\b", prose, re.IGNORECASE):
                line = prose.count("\n", 0, match.start()) + 1
                errors.append(
                    f"{display(path)}:{line}: use American {american!r}, not {match.group(0)!r}"
                )

        for match in re.finditer(r"(?<!!)\[[^]]+\]\(([^)]+)\)", text):
            validate_link(path, match.group(1), anchor_map, errors)

        display_delimiters = [
            number
            for number, line in enumerate(text.splitlines(), 1)
            if "$$" in line
        ]
        for number in display_delimiters:
            if text.splitlines()[number - 1].strip() != "$$":
                errors.append(
                    f"{display(path)}:{number}: display-math delimiter must be on its own line"
                )
        if len(display_delimiters) % 2:
            errors.append(f"{display(path)}: unbalanced display-math delimiters")

        definitions = set(re.findall(r"^\[\^([^]]+)\]:", text, re.MULTILINE))
        uses = set(re.findall(r"\[\^([^]]+)\]", re.sub(r"^\[\^[^]]+\]:.*$", "", text, flags=re.MULTILINE)))
        for key in sorted(uses - definitions):
            errors.append(f"{display(path)}: unresolved footnote citation [^{key}]")
        for key in sorted(definitions - uses):
            errors.append(f"{display(path)}: unused footnote citation [^{key}]")

    for fence in fences:
        validate_fence(fence, errors)

    counts = collections.Counter(fence.language or "unlabeled" for fence in fences)
    rust_count = counts.get("rust", 0)
    if args.require_no_rust and rust_count:
        errors.append(
            f"{rust_count} Rust fences remain; move complete examples to the compiled harness "
            "and mark genuinely schematic fragments as text"
        )

    citation_keys = set()
    for text in texts.values():
        citation_keys.update(re.findall(r"\[@([A-Za-z0-9_.:-]+)", text))

    print(f"Markdown files: {len(markdown)}")
    print("Fences: " + ", ".join(f"{lang}={count}" for lang, count in sorted(counts.items())))
    print(f"Pandoc-style citation keys: {len(citation_keys)}")
    print(f"Rust fences awaiting compile harness: {rust_count}")

    if errors:
        print(f"book check failed with {len(errors)} error(s):", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("book check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

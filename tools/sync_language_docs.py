#!/usr/bin/env python3
"""Synchronize generated manual indexes with src/language/catalog.tsv."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "src" / "language" / "catalog.tsv"


@dataclass(frozen=True)
class Topic:
    context: str
    topic: str
    aliases: tuple[str, ...]


@dataclass(frozen=True)
class Error:
    number: int
    english: str
    spanish: str


def load_catalog() -> tuple[list[Topic], list[Error]]:
    section = ""
    topics: list[Topic] = []
    errors: list[Error] = []
    for line_number, raw in enumerate(
        CATALOG.read_text(encoding="utf-8-sig").splitlines(), start=1
    ):
        line = raw.rstrip("\r")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped in {"[keywords]", "[topics]", "[errors]"}:
            section = stripped
            continue
        fields = line.split("\t")
        if section == "[topics]":
            if len(fields) != 9:
                raise ValueError(f"catalog line {line_number}: expected 9 topic fields")
            topics.append(
                Topic(
                    context=fields[1].strip(),
                    topic=normalize_name(fields[2]),
                    aliases=tuple(
                        normalize_name(alias)
                        for alias in fields[3].split(",")
                        if alias.strip()
                    ),
                )
            )
        elif section == "[errors]":
            if len(fields) != 5:
                raise ValueError(f"catalog line {line_number}: expected 5 error fields")
            errors.append(
                Error(
                    number=int(fields[1]),
                    english=fields[2].strip(),
                    spanish=fields[3].strip(),
                )
            )

    expected_numbers = list(range(1, len(errors) + 1))
    if [error.number for error in errors] != expected_numbers:
        raise ValueError("error numbers must be contiguous and ordered")
    return topics, errors


def normalize_name(value: str) -> str:
    return " ".join(value.split()).upper()


def context_index(topics: list[Topic], context: str) -> str:
    names = {
        name
        for topic in topics
        if topic.context == context
        for name in (topic.topic, *topic.aliases)
    }
    return " | ".join(sorted(names))


def replace_single_line_after_heading(text: str, heading: str, value: str) -> str:
    pattern = re.compile(rf"({re.escape(heading)}\n\n)\+ [^\n]*")
    updated, count = pattern.subn(rf"\g<1>+ {value}", text, count=1)
    if count != 1:
        raise ValueError(f"manual heading not found exactly once: {heading!r}")
    return updated


def replace_error_table(text: str, heading: str, rows: list[str]) -> str:
    pattern = re.compile(rf"({re.escape(heading)}\n).*?(\n9\.)", re.S)
    rows_text = "\n".join(rows)
    replacement = rf"\g<1>{rows_text}\n\g<2>"
    updated, count = pattern.subn(replacement, text, count=1)
    if count != 1:
        raise ValueError(f"error table heading not found exactly once: {heading!r}")
    return updated


def render_manual(path: Path, topics: list[Topic], errors: list[Error]) -> str:
    text = path.read_text(encoding="utf-8-sig")
    spanish = path.name == "MANUAL.es.txt"
    if spanish:
        text = text.replace("`ERROR 14` provoca un error de sintaxis", "`ERROR 15` provoca un error de sintaxis")
        text = replace_error_table(
            text,
            "Tabla completa de códigos de error",
            [f"{error.number:<2} {error.spanish}" for error in errors],
        )
        text = replace_single_line_after_heading(
            text,
            "9.\u202fComandos “solo inmediato”",
            context_index(topics, "immediate"),
        )
        text = replace_single_line_after_heading(
            text,
            "9.1 Comandos prohibidos en inmediato:",
            context_index(topics, "program"),
        )
    else:
        text = text.replace("`ERROR 14` raises a syntax error", "`ERROR 15` raises a syntax error")
        text = replace_error_table(
            text,
            "Complete table of error codes",
            [f"{error.number:<2} {error.english}" for error in errors],
        )
        text = replace_single_line_after_heading(
            text,
            '9. "Immediate mode only" commands',
            context_index(topics, "immediate"),
        )
        text = replace_single_line_after_heading(
            text,
            "9.1 Commands forbidden in immediate mode:",
            context_index(topics, "program"),
        )
    return text


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    topics, errors = load_catalog()
    changed: list[Path] = []
    for path in [ROOT / "MANUAL.txt", ROOT / "MANUAL.es.txt"]:
        current = path.read_text(encoding="utf-8-sig")
        rendered = render_manual(path, topics, errors)
        if current != rendered:
            changed.append(path)
            if not args.check:
                path.write_text(rendered, encoding="utf-8", newline="\n")
    if args.check and changed:
        for path in changed:
            print(f"out of date: {path.relative_to(ROOT)}")
        return 1
    print(f"language docs ok: {len(topics)} topics, {len(errors)} errors")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Check the generated Rust error contract against the Python oracle."""

from __future__ import annotations

import argparse
import ast
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "src" / "language" / "catalog.tsv"
DEFAULT_PY_REPO = ROOT.parents[1] / "Python" / "AVL-BASIC"


def catalog_errors() -> list[tuple[str, int, str]]:
    errors: list[tuple[str, int, str]] = []
    section = ""
    for line_number, raw in enumerate(
        CATALOG.read_text(encoding="utf-8-sig").splitlines(), start=1
    ):
        line = raw.strip()
        if line in {"[keywords]", "[topics]", "[errors]"}:
            section = line
            continue
        if section != "[errors]" or not line or line.startswith("#"):
            continue
        fields = raw.split("\t")
        if len(fields) != 5:
            raise ValueError(f"catalog line {line_number}: expected 5 error fields")
        errors.append((fields[4].strip(), int(fields[1]), fields[2].strip()))
    return errors


def python_errors(basic_py: Path) -> list[tuple[str, int, str]]:
    tree = ast.parse(basic_py.read_text(encoding="utf-8-sig"), filename=str(basic_py))
    error_class = next(
        (
            node
            for node in tree.body
            if isinstance(node, ast.ClassDef) and node.name == "ErrorCode"
        ),
        None,
    )
    if error_class is None:
        raise ValueError(f"ErrorCode class not found in {basic_py}")

    errors: list[tuple[str, int, str]] = []
    for statement in error_class.body:
        if not isinstance(statement, ast.Assign) or len(statement.targets) != 1:
            continue
        if not isinstance(statement.targets[0], ast.Name):
            continue
        value = statement.value
        if not isinstance(value, ast.Tuple) or len(value.elts) != 2:
            continue
        number = ast.literal_eval(value.elts[0])
        message = ast.literal_eval(value.elts[1])
        if isinstance(number, int) and isinstance(message, str):
            errors.append((statement.targets[0].id, number, message))
    return errors


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--py-repo",
        type=Path,
        default=DEFAULT_PY_REPO,
        help="Path to the Python AVL-BASIC repository",
    )
    args = parser.parse_args()

    expected = python_errors(args.py_repo.resolve() / "basic.py")
    actual = catalog_errors()
    if actual != expected:
        for index, (rust_error, python_error) in enumerate(
            zip(actual, expected, strict=False), start=1
        ):
            if rust_error != python_error:
                raise SystemExit(
                    f"error {index} differs: Rust catalog={rust_error!r}, "
                    f"Python oracle={python_error!r}"
                )
        raise SystemExit(
            f"error catalog length differs: Rust={len(actual)}, Python={len(expected)}"
        )
    print(f"Python/Rust error catalog parity ok: {len(actual)} errors")


if __name__ == "__main__":
    main()

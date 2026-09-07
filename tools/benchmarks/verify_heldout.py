"""Independently verify powermul and Unicode outputs in a benchmark JSON."""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

from representative import TIMING


def unicode_expected(passes: int) -> tuple[int, int, list[str]]:
    originals = [("aéΩñ" * 512)[:32 * 4**k] for k in range(4)]
    results = [""] * 4
    total = changes = 0
    for turn in range(1, passes + 1):
        for k, original in enumerate(originals):
            position = 2 + turn * 17 % (len(original) - 5)
            row = original[:position - 1] + "ñΩé" + original[position + 2:]
            total += row.find("ñΩé") + 1 + row.find("xyz") + 1
            total += ord(row[position - 1]) + len(row)
            changes += row != original
            results[k] = row
    return changes, total, results


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text(encoding="utf-8"))
    if hasattr(sys, "set_int_max_str_digits"):
        sys.set_int_max_str_digits(0)
    expected = {}
    for name, workload in report["workloads"].items():
        if name == "powermul":
            expected[name] = str(7 ** workload["spec"]["exponent"])
        elif name == "unicode":
            expected[name] = unicode_expected(workload["spec"]["passes"])
        else:
            raise ValueError(f"Unsupported independent oracle: {name}")
    for run in report["runs"]:
        name = run["workload"]
        text = TIMING.sub("", run["stdout"]).strip()
        if name == "powermul":
            assert re.fullmatch(r"[0-9\s]+", text), text[:120]
            assert "".join(text.split()) == expected[name]
        else:
            lines = text.splitlines()
            changes, total, rows = expected[name]
            fields = lines[0].split()
            assert fields[0] == "UNICODE" and [int(float(x)) for x in fields[1:]] == [changes, total]
            assert lines[1:] == rows
        assert not run["stderr"]
    print(f"Independent Python oracles match all {len(report['runs'])} executions.")


if __name__ == "__main__":
    main()

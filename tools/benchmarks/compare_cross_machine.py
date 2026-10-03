"""Compare two complete cross_machine.ps1 result sets after validating identity."""
from __future__ import annotations

import argparse
import json
import statistics
import zipfile
from pathlib import Path


def read_result(path: Path) -> dict:
    if path.is_dir():
        return json.loads((path / "raw.json").read_text(encoding="utf-8-sig"))
    if path.suffix.lower() == ".zip":
        with zipfile.ZipFile(path) as archive:
            matches = [name for name in archive.namelist() if name.endswith("/raw.json") or name == "raw.json"]
            if len(matches) != 1:
                raise ValueError(f"Expected one raw.json in {path}, found {matches}")
            return json.loads(archive.read(matches[0]).decode("utf-8-sig"))
    return json.loads(path.read_text(encoding="utf-8-sig"))


def groups(report: dict) -> dict:
    meta = report["metadata"]
    if meta["status"] != "complete" or meta["smoke"]:
        raise ValueError("Both inputs must be complete, non-smoke measurements")
    result = {}
    identities = {}
    for row in report["records"]:
        if not row["valid"]:
            raise ValueError(f"Invalid observation: {row['directory']}")
        identity = (row["program_sha256"], row["correctness_sha256"])
        previous = identities.setdefault(row["workload"], identity)
        if identity != previous:
            raise ValueError(f"Work/output differs within {row['workload']}")
        if not row["warmup"]:
            result.setdefault((row["workload"], row["mode"]), []).append(row)
    for key, rows in result.items():
        if len(rows) != meta["runs"]:
            raise ValueError(f"Incomplete repetitions for {key}")
    return result


def stats(values: list[float]) -> dict:
    med = statistics.median(values)
    return {"median": med, "mad": statistics.median(abs(v-med) for v in values),
            "min": min(values), "max": max(values), "n": len(values)}


def compare(left: dict, right: dict) -> dict:
    a, b = groups(left), groups(right)
    for key in ("executable_sha256", "manifest_sha256", "runner_sha256"):
        if left["metadata"][key] != right["metadata"][key]:
            raise ValueError(f"Different {key}; cannot isolate machine differences")
    if a.keys() != b.keys():
        raise ValueError("Different workload/mode sets; review CPU topology classification")
    output = {"baseline": left["metadata"]["machine_label"],
              "candidate": right["metadata"]["machine_label"], "rows": [],
              "scope": "System comparison; these timings alone do not identify IPC/cache/frequency causality."}
    for workload, mode in sorted(a):
        first, second = a[(workload, mode)], b[(workload, mode)]
        if first[0]["correctness_sha256"] != second[0]["correctness_sha256"]:
            raise ValueError(f"Different calculated result for {workload}/{mode}")
        row = {"workload": workload, "mode": mode}
        for metric in ("wall_s", "running_wall_s", "cpu_s", "body_s"):
            left_values = [r[metric] for r in first if metric != "body_s" or r["body_valid"]]
            right_values = [r[metric] for r in second if metric != "body_s" or r["body_valid"]]
            if not left_values or not right_values:
                row[metric] = None
                continue
            ls, rs = stats(left_values), stats(right_values)
            row[metric] = {"baseline": ls, "candidate": rs,
                           "candidate_speedup_pct": (ls["median"]/rs["median"]-1)*100,
                           "candidate_time_reduction_pct": (1-rs["median"]/ls["median"])*100}
        output["rows"].append(row)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = compare(read_result(args.baseline), read_result(args.candidate))
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print("workload           mode                    base_s   candidate_s   speedup    MAD_base / MAD_candidate")
    for row in result["rows"]:
        metric = row["wall_s"]
        print(f"{row['workload']:18} {row['mode']:23} {metric['baseline']['median']:8.4f} "
              f"{metric['candidate']['median']:11.4f} {metric['candidate_speedup_pct']:8.2f}% "
              f" {metric['baseline']['mad']:.4f} / {metric['candidate']['mad']:.4f}")


if __name__ == "__main__":
    main()

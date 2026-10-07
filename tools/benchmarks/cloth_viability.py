"""Frozen, serial AB/BA cloth study using external stdout-marker timestamps.

Every process runs the actual six-substep frame body followed by its renderer,
one warmup frame plus five measured frames by default. Startup, initialization,
checkpoint loading and exports are outside the primary frame metric. This tool
never builds executables, changes runtime source, or runs interpreters in parallel.
Relative input and output paths are resolved from the checkout root.

Example (paths may contain spaces):
  python tools/benchmarks/cloth_viability.py --output target/cloth-study-headless \
    --variant control /path/control.exe /path/baseline.bas \
    --variant gtri /path/gtri.exe /path/g-cloth-gtri.bas \
    --variant numeric /path/numeric.exe /path/g-cloth-gtri.bas --window 0
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import queue
import re
import statistics
import struct
import subprocess
import sys
import threading
import time
import zlib


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT.parent if ROOT.name == "study" and ROOT.parent.name == "cloth-perf" else ROOT / "target/cloth-perf"
MARKER = re.compile(r"^__CLV_(READY|BEGIN|PH_DONE|STAGE|RENDER_DONE|END|DONE)__\s*\|\s*(\d+)\s*\|\s*([A-Za-z0-9_-]+)\s*$")
LABEL = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")
STAGES = [(6109, "normals_camera"), (6139, "geometry_copy"),
          (6159, "projection"), (6199, "shadow"), (6229, "shading"),
          (6239, "background"), (6259, "raster"), (6269, "hud"),
          (6279, "present")]
EXPORTS = ("state.csv", "edges.csv", "bends.csv", "scalars.csv")


def repo_path(value: str | Path) -> Path:
    """Resolve portable benchmark paths against this checkout, not caller cwd."""
    path = Path(value)
    return (path if path.is_absolute() else ROOT / path).resolve()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def parse_source(data: bytes) -> dict[int, str]:
    code: dict[int, str] = {}
    for row in data.decode("utf-8-sig").splitlines():
        if not row.strip():
            continue
        match = re.fullmatch(r"\s*(\d+)\s+(.*)", row)
        if not match or int(match[1]) in code:
            raise ValueError(f"Malformed or duplicate BASIC line: {row!r}")
        code[int(match[1])] = match[2]
    return code


def source_contract(code: dict[int, str], baseline: dict[int, str]) -> None:
    for number in [130, 150, 160, 170, 200, 220, 224, 450, 460, 470, 480, 490,
                   6110, 6140, 6160, 6200, 6230, 6240, 6260, 6270, 6280, 7000]:
        if number not in code:
            raise ValueError(f"Source is missing benchmark anchor line {number}")
    for pattern in [r"\bMODE\s+800\b", r"\bNX\s*=\s*40\b", r"\bNY\s*=\s*34\b",
                    r"\bH\s*=\s*1\s*/\s*180\b", r"\bSTEPS\s*=\s*6\b"]:
        if not re.search(pattern, "\n".join(code[n] for n in [130, 150]), re.I):
            raise ValueError(f"Frozen resolution/solver configuration changed: {pattern}")
    # Only replacing the software Gouraud routine is an allowed BASIC change.
    stable = lambda c: {n: body for n, body in c.items() if not 7000 <= n < 8000}
    if stable(code) != stable(baseline):
        differences = sorted(n for n in set(code) | set(baseline)
                             if not 7000 <= n < 8000 and code.get(n) != baseline.get(n))
        raise ValueError(f"Variant source changes the frozen workload outside raster routine: {differences}")
    if any(re.search(r"\bMAT\s+(DISTANCE|BEND|STRAIN|CONTACT|REPULSE)\b", body, re.I)
           for body in code.values() if not body.upper().startswith("REM ")):
        raise ValueError("Physical MAT APIs are outside the authorized generic-interpreter study")


def marker(kind: str, phase: str = "frame") -> str:
    return f'PRINT "__CLV_{kind}__|";CV_FRAME;"|{phase}"'


def make_program(code: dict[int, str], frames: int, warmup: int, gtriangle: bool,
                 scene: str) -> bytes:
    code = dict(code)
    if scene == "hang":
        code[150] = code[150].replace("SCENE=1", "SCENE=0").replace("PINMODE=0", "PINMODE=2")
        code[170] = code[170].replace("DIST=4.3", "DIST=7.5").replace("TARGETY=1.3", "TARGETY=2")
    code[170] = code[170].replace("HUD=1", "HUD=0")
    code[190] = "REM No introductory text or presentation in the timed clone."
    code[210] = "REM Reuse frozen state; do not repeat the 600-substep drape."
    code[220] = ('OPEN "checkpoint.csv" FOR INPUT AS #1 : FOR CV_K=0 TO N-1 : '
                 'INPUT #1,X(CV_K),Y(CV_K),Z(CV_K),PH_VX(CV_K),PH_VY(CV_K),PH_VZ(CV_K) : NEXT CV_K : CLOSE #1')
    code[222] = "REM No live keyboard input during the frozen study."
    code[224] = "REM Settling replaced by checkpoint loading."
    code[225] = "MAT OX=X : MAT OY=Y : MAT OZ=Z : T=600*H : PH_TIME=T : FRAMES=0 : CV_FRAME=0"
    code[230] = marker("READY", "ready")
    for number in list(code):
        if 240 <= number < 450:
            del code[number]
    code[240] = f"FOR CV_FRAME=1 TO {frames + warmup}"
    code[250] = marker("BEGIN")
    code[485] = marker("PH_DONE", "physics")
    code[490] = code[490].replace(" : GOTO 300", "")
    if "GOTO" in code[490].upper() or "GOSUB 6100" not in code[490].upper():
        raise ValueError("Could not retain the actual frame render statement")
    code[491] = marker("RENDER_DONE", "render")
    code[492] = marker("END")
    code[495] = "NEXT CV_FRAME"
    code[600] = 'OPEN "state.csv" FOR OUTPUT AS #1'
    code[610] = ('FOR CV_K=0 TO N-1 : WRITE #1,X(CV_K),Y(CV_K),Z(CV_K),PH_VX(CV_K),PH_VY(CV_K),PH_VZ(CV_K),'
                 'IM(CV_K),PH_CN(CV_K),PH_CT(CV_K),PH_NX(CV_K),PH_NY(CV_K),PH_NZ(CV_K) : NEXT CV_K : CLOSE #1')
    code[620] = 'OPEN "edges.csv" FOR OUTPUT AS #1 : FOR CV_K=0 TO PH_NE-1 : WRITE #1,PH_EA(CV_K),PH_EB(CV_K),PH_EL(CV_K),PH_EC(CV_K),PH_ELAM(CV_K) : NEXT CV_K : CLOSE #1'
    code[630] = 'OPEN "bends.csv" FOR OUTPUT AS #1 : FOR CV_K=0 TO PH_NB-1 : WRITE #1,PH_BA(CV_K),PH_BB(CV_K),PH_BC(CV_K),PH_BG(CV_K),PH_BLX(CV_K),PH_BLY(CV_K),PH_BLZ(CV_K) : NEXT CV_K : CLOSE #1'
    code[640] = 'OPEN "scalars.csv" FOR OUTPUT AS #1 : WRITE #1,N,PH_NE,PH_NB,PH_NTRI,NX,NY,H,STEPS,T,PH_TIME,SCENE,PINMODE,WIND,MATERIAL,BX0,BY0,BZ0,BRAD : CLOSE #1'
    code[650] = 'BSAVE "frame.png" : ' + marker("DONE", "done") + ' : SCREEN CLOSE : END'
    for line, phase in STAGES:
        if line in code:
            raise ValueError(f"Instrumentation line {line} collides with source")
        code[line] = marker("STAGE", phase)
    if gtriangle:
        for number in list(code):
            if 7000 <= number < 8000:
                del code[number]
        code[7000] = "GTRIANGLE ZBUF,AX,AY,AQ,AR,AG,AB,BX,BY,BQ,BR,BG,BB,CX,CY,CQ,CR,CG,CB : RETURN"
    return "".join(f"{n} {body}\n" for n, body in sorted(code.items())).encode("utf-8")


def median_mad(values: list[float]) -> dict[str, float | int]:
    med = statistics.median(values)
    return {"median_s": med, "mad_s": statistics.median(abs(v - med) for v in values),
            "min_s": min(values), "max_s": max(values), "samples": len(values)}


def parse_events(events: list[dict], total: int, warmup: int) -> tuple[dict, dict]:
    expected = [("READY", 0, "ready")]
    for frame in range(1, total + 1):
        expected += [("BEGIN", frame, "frame"), ("PH_DONE", frame, "physics")]
        expected += [("STAGE", frame, phase) for _, phase in STAGES]
        expected += [("RENDER_DONE", frame, "render"), ("END", frame, "frame")]
    expected += [("DONE", total + 1, "done")]
    actual = [(event["kind"], event["frame"], event["phase"]) for event in events]
    if actual != expected:
        first = next((i for i, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        raise ValueError(f"Marker sequence mismatch at {first}: observed={actual[first:first+2]}, expected={expected[first:first+2]}")
    spans: dict[str, list[float]] = {name: [] for name in ["total", "physics", "render", "frame_other", *[p for _, p in STAGES]]}
    for frame in range(1, total + 1):
        selected = [event for event in events if event["frame"] == frame]
        lookup = {event["kind"]: event["s"] for event in selected if event["kind"] != "STAGE"}
        spans["total"].append(lookup["END"] - lookup["BEGIN"])
        spans["physics"].append(lookup["PH_DONE"] - lookup["BEGIN"])
        spans["render"].append(lookup["RENDER_DONE"] - lookup["PH_DONE"])
        spans["frame_other"].append(lookup["END"] - lookup["RENDER_DONE"])
        stages = [event for event in selected if event["kind"] == "STAGE"]
        for index, event in enumerate(stages):
            end = stages[index + 1]["s"] if index + 1 < len(stages) else lookup["RENDER_DONE"]
            spans[event["phase"]].append(end - event["s"])
    if any(not math.isfinite(value) or value < 0 for values in spans.values() for value in values):
        raise ValueError("Nonfinite or negative externally measured interval")
    if any(value <= 0 for value in spans["total"]):
        raise ValueError("Nonpositive frame time")
    summary = {phase: median_mad(values[warmup:]) for phase, values in spans.items()}
    summary["total"]["fps"] = 1.0 / summary["total"]["median_s"]
    return spans, summary


def png_pixels(data: bytes) -> tuple[int, int, bytes]:
    """Decode standard RGB/RGBA 8-bit noninterlaced PNG without extra packages."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("Invalid PNG signature")
    position, payload, header = 8, bytearray(), None
    while position < len(data):
        size = struct.unpack(">I", data[position:position + 4])[0]
        kind = data[position + 4:position + 8]
        body = data[position + 8:position + 8 + size]
        crc = data[position + 8 + size:position + 12 + size]
        if len(body) != size or len(crc) != 4 or struct.unpack(">I", crc)[0] != zlib.crc32(kind + body) & 0xffffffff:
            raise ValueError("Truncated PNG or CRC mismatch")
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            payload.extend(body)
        elif kind == b"IEND":
            break
        position += size + 12
    if header is None:
        raise ValueError("Missing PNG header")
    width, height, bits, color, compression, filtering, interlace = header
    if bits != 8 or color not in (2, 6) or compression or filtering or interlace:
        raise ValueError(f"Unsupported PNG encoding: {header}")
    bpp = 3 if color == 2 else 4
    stride = width * bpp
    raw = zlib.decompress(payload)
    if len(raw) != height * (stride + 1):
        raise ValueError("PNG raster length differs from dimensions")
    previous, output = bytearray(stride), bytearray()
    for y in range(height):
        offset = y * (stride + 1)
        mode, row = raw[offset], bytearray(raw[offset + 1:offset + 1 + stride])
        if mode > 4:
            raise ValueError(f"Unsupported PNG row filter {mode}")
        for x in range(stride):
            left, up = row[x - bpp] if x >= bpp else 0, previous[x]
            corner = previous[x - bpp] if x >= bpp else 0
            if mode == 1:
                row[x] = (row[x] + left) & 255
            elif mode == 2:
                row[x] = (row[x] + up) & 255
            elif mode == 3:
                row[x] = (row[x] + (left + up) // 2) & 255
            elif mode == 4:
                p = left + up - corner
                da, db, dc = abs(p - left), abs(p - up), abs(p - corner)
                predictor = left if da <= db and da <= dc else up if db <= dc else corner
                row[x] = (row[x] + predictor) & 255
        for x in range(0, stride, bpp):
            output.extend(row[x:x + 3])
        previous = row
    return width, height, bytes(output)


def read_csv(path: Path, rows: int, columns: int) -> list[list[float]]:
    result = [[float(cell) for cell in row] for row in csv.reader(path.read_text(encoding="utf-8").splitlines())]
    if len(result) != rows or any(len(row) != columns for row in result):
        raise ValueError(f"Unexpected {path.name} shape: {len(result)} rows, expected {rows} x {columns}")
    if any(not math.isfinite(value) for row in result for value in row):
        raise ValueError(f"Nonfinite state export in {path}")
    return result


def check_exports(directory: Path, total: int, scene: str) -> dict:
    scalar = read_csv(directory / "scalars.csv", 1, 18)[0]
    expected = {0: 1435, 1: 5514, 2: 2718, 3: 2720, 4: 40, 5: 34, 6: 1 / 180,
                7: 6, 10: int(scene == "drape"), 11: 0 if scene == "drape" else 2,
                12: 1, 13: 0, 14: 0, 15: 1.45, 16: -.25, 17: 1.03}
    for index, value in expected.items():
        if not math.isclose(scalar[index], value, rel_tol=0, abs_tol=1e-12):
            raise ValueError(f"Frozen configuration scalar {index} changed: {scalar[index]} != {value}")
    for index in (8, 9):
        if not math.isclose(scalar[index], (600 + total * 6) / 180, rel_tol=0, abs_tol=1e-10):
            raise ValueError("The actual six-substep frame clock did not advance as expected")
    read_csv(directory / "state.csv", 1435, 12)
    read_csv(directory / "edges.csv", 5514, 5)
    read_csv(directory / "bends.csv", 2718, 7)
    width, height, pixels = png_pixels((directory / "frame.png").read_bytes())
    if (width, height) != (800, 600):
        raise ValueError(f"Unexpected screenshot size {width} x {height}")
    return {**{name + "_sha256": sha256((directory / name).read_bytes()) for name in EXPORTS},
            "png_sha256": sha256((directory / "frame.png").read_bytes()), "pixels_sha256": sha256(pixels)}


def run_one(executable: Path, directory: Path, checkpoint: bytes, program: bytes,
            frames: int, warmup: int, window: int, timeout: float, scene: str,
            metadata: dict) -> dict:
    directory.mkdir()
    (directory / "checkpoint.csv").write_bytes(checkpoint)
    (directory / "bench.bas").write_bytes(program)
    environment = {**os.environ, "AVL_BASIC_WINDOW": str(window), "AVL_BASIC_AUDIO": "off"}
    started = time.perf_counter()
    process = subprocess.Popen([str(executable), str(directory / "bench.bas")], cwd=directory,
                               env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    messages: queue.Queue = queue.Queue()
    def reader(stream, channel):
        try:
            for raw in iter(stream.readline, b""):
                messages.put((channel, time.perf_counter() - started, raw))
        finally:
            messages.put((channel, time.perf_counter() - started, None))
    for stream, channel in [(process.stdout, "stdout"), (process.stderr, "stderr")]:
        threading.Thread(target=reader, args=(stream, channel), daemon=True).start()
    streams, events, finished = {"stdout": bytearray(), "stderr": bytearray()}, [], set()
    error = None
    try:
        while len(finished) < 2:
            remaining = timeout - (time.perf_counter() - started)
            if remaining <= 0:
                raise TimeoutError(f"Interpreter timeout after {timeout:g} seconds")
            try:
                channel, elapsed, raw = messages.get(timeout=min(remaining, .5))
            except queue.Empty:
                continue
            if raw is None:
                finished.add(channel)
                continue
            streams[channel].extend(raw)
            if channel == "stdout":
                row = raw.decode("utf-8-sig", errors="strict").strip()
                if match := MARKER.fullmatch(row):
                    events.append({"kind": match[1], "frame": int(match[2]), "phase": match[3], "s": elapsed})
                elif row:
                    raise ValueError(f"Unexpected interpreter stdout: {row!r}")
        process.wait(timeout=max(.01, timeout - (time.perf_counter() - started)))
        if process.returncode or streams["stderr"].strip():
            raise ValueError(f"Interpreter failed: status={process.returncode}, stderr={bytes(streams['stderr'])!r}")
        spans, summary = parse_events(events, frames + warmup, warmup)
        hashes = check_exports(directory, frames + warmup, scene)
    except Exception as exc:
        error = str(exc)
        if process.poll() is None:
            process.kill()
        process.wait()
    finally:
        for channel, raw in streams.items():
            (directory / f"{channel}.txt").write_bytes(raw)
    result = {**metadata, "directory": str(directory), "process_seconds": time.perf_counter() - started,
              "returncode": process.returncode, "events": events,
              "program_sha256": sha256(program), "validated": error is None}
    if error is None:
        result.update({"spans_per_frame": spans, "summary_after_frame_warmup": summary, "hashes": hashes})
    else:
        result["error"] = error
    write_json(directory / "result.json", result)
    if error:
        raise RuntimeError(f"{directory}: {error}")
    return result


def equality(left: dict, right: dict) -> dict:
    by_file = {key: left["hashes"][key] == right["hashes"][key] for key in left["hashes"]}
    numeric_diff = {}
    for name in EXPORTS:
        l = [[float(c) for c in row] for row in csv.reader((Path(left["directory"]) / name).read_text().splitlines())]
        r = [[float(c) for c in row] for row in csv.reader((Path(right["directory"]) / name).read_text().splitlines())]
        values = [abs(a - b) for la, rb in zip(l, r) for a, b in zip(la, rb)]
        numeric_diff[name] = {"max_abs_difference": max(values, default=0),
                              "different_values": sum(a != b for la, rb in zip(l, r) for a, b in zip(la, rb))}
    _, _, lp = png_pixels((Path(left["directory"]) / "frame.png").read_bytes())
    _, _, rp = png_pixels((Path(right["directory"]) / "frame.png").read_bytes())
    return {"files_equal": by_file, "csv_exact": all(by_file[name + "_sha256"] for name in EXPORTS),
            "png_bytes_exact": by_file["png_sha256"], "pixels_exact": by_file["pixels_sha256"],
            "csv_differences": numeric_diff, "different_pixels": sum(lp[i:i + 3] != rp[i:i + 3] for i in range(0, len(lp), 3)),
            "max_rgb_component_difference": max((abs(a - b) for a, b in zip(lp, rp)), default=0)}


def summarize(results: list[dict], comparisons: list[tuple[str, str]]) -> dict:
    variants = {}
    for label in dict.fromkeys(result["variant"] for result in results):
        runs = [r for r in results if r["variant"] == label and r["phase"] == "measured"]
        metrics = {}
        for metric in runs[0]["spans_per_frame"]:
            per_run = [r["summary_after_frame_warmup"][metric]["median_s"] for r in runs]
            metrics[metric] = median_mad(per_run)
        metrics["total"]["fps"] = 1 / metrics["total"]["median_s"]
        variants[label] = metrics
    compared = []
    for comparison_index, (left, right) in enumerate(comparisons):
        selected = [r for r in results if r["comparison"] == comparison_index and r["phase"] == "measured"]
        samples = {label: [r["summary_after_frame_warmup"]["total"]["median_s"] for r in selected if r["variant"] == label]
                   for label in (left, right)}
        a, b = statistics.median(samples[left]), statistics.median(samples[right])
        paired = []
        for pair in sorted({r["pair"] for r in selected}):
            rows = {r["variant"]: r for r in selected if r["pair"] == pair}
            at = rows[left]["summary_after_frame_warmup"]["total"]["median_s"]
            bt = rows[right]["summary_after_frame_warmup"]["total"]["median_s"]
            paired.append({"pair": pair, "order": [r["variant"] for r in selected if r["pair"] == pair],
                           "baseline_s": at, "candidate_s": bt, "speedup": at / bt,
                           "time_reduction_pct": (1 - bt / at) * 100})
        compared.append({"baseline": left, "candidate": right, "baseline_total": median_mad(samples[left]),
                         "candidate_total": median_mad(samples[right]), "time_reduction_pct": (1 - b / a) * 100,
                         "speedup": a / b, "pairs": paired,
                         "paired_speedup_median": statistics.median(row["speedup"] for row in paired),
                         "paired_time_reduction_pct_median": statistics.median(row["time_reduction_pct"] for row in paired)})
    return {"variants": variants, "comparisons": compared,
            "aggregation": "median/MAD of per-process measured-frame medians; FPS=1/median TOTAL frame seconds"}


def spanish_report(result: dict) -> str:
    config = result["configuration"]
    lines = ["# Estudio de viabilidad de g-cloth", "",
             f"Modo: {'ventana real' if config['window'] else 'sin ventana'}. Misma malla 40×34, imagen 800×600 y seis subpasos H=1/180 por frame. Cada proceso descarta {config['warmup_frames']} frame(s) y mide {config['measured_frames']}.", "",
             "El tiempo principal procede de marcas de stdout observadas con perf_counter. Incluye física y render intercalados en cada frame; excluye inicialización, carga del checkpoint y exportaciones. El FPS es el inverso de la mediana del tiempo TOTAL por frame.", "",
             "| Variante | Total ms | MAD ms | FPS | Física ms | Render ms |",
             "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for label, metrics in result["summary"]["variants"].items():
        t = metrics["total"]
        lines.append(f"| {label} | {t['median_s']*1000:.3f} | {t['mad_s']*1000:.3f} | {t['fps']:.3f} | {metrics['physics']['median_s']*1000:.3f} | {metrics['render']['median_s']*1000:.3f} |")
    lines += ["", "Las medianas de etapas se calculan por separado; no deben sumarse como si correspondieran a un mismo frame. La tabla agrupa los procesos medidos de cada variante; los cambios siguientes usan solo los procesos del mismo bloque AB/BA para evitar mezclar series.", ""]
    for comparison in result["summary"]["comparisons"]:
        lines.append(f"{comparison['baseline']} → {comparison['candidate']}: reducción de tiempo {comparison['time_reduction_pct']:.2f}% ({comparison['speedup']:.3f}×); mediana de reducciones emparejadas {comparison['paired_time_reduction_pct_median']:.2f}%.")
    equality_checks = result["equality"]
    exact = all(c["csv_exact"] and c["pixels_exact"] and c["png_bytes_exact"] for c in equality_checks)
    lines += ["", "Validación: " + ("CSV, píxeles y PNG idénticos en todas las comparaciones." if exact else "hay diferencias semánticas o de imagen; consulta el JSON antes de interpretar mejoras como equivalentes."),
              "", "Las medidas de ventana y sin ventana pertenecen a series separadas. El coste de las marcas, la planificación del sistema y la lectura de stdout forman parte del margen experimental. Los hashes y los resultados individuales se conservan en el JSON y en cada directorio de ejecución.", ""]
    return "\n".join(lines)


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--variant", action="append", nargs=3, metavar=("LABEL", "EXE", "SOURCE"), default=[])
    p.add_argument("--compare", action="append", nargs=2, metavar=("BASE", "CANDIDATE"))
    p.add_argument("--gtriangle", action="append", default=[], metavar="LABEL", help="Replace only routine 7000 with native GTRIANGLE")
    p.add_argument("--baseline-source", type=Path, default=FIXTURES / "baseline.bas")
    p.add_argument("--checkpoint", type=Path, default=FIXTURES / "state.csv")
    p.add_argument("--expected-checkpoint-sha256", help="Override the frozen baseline manifest checksum, required for a different checkpoint")
    p.add_argument("--expected-source-sha256", help="Override the frozen baseline manifest source checksum")
    p.add_argument("--scene", choices=["drape", "hang"], default="drape", help="HANG requires an explicitly supplied HANG checkpoint")
    p.add_argument("--checkpoint-scene", choices=["drape", "hang"], default="drape")
    p.add_argument("--window", type=int, choices=[0, 1], default=0)
    p.add_argument("--frames", type=int, default=5)
    p.add_argument("--warmup-frames", type=int, default=1)
    p.add_argument("--pairs", type=int, default=4)
    p.add_argument("--warmup-pairs", type=int, default=1)
    p.add_argument("--timeout", type=float, default=120)
    p.add_argument("--output", type=Path)
    p.add_argument("--prepare-only", action="store_true")
    p.add_argument("--report-only", type=Path, metavar="RESULTS_JSON")
    a = p.parse_args(argv)
    if a.report_only:
        path = repo_path(a.report_only)
        path.with_suffix(".md").write_text(spanish_report(json.loads(path.read_text(encoding="utf-8"))), encoding="utf-8")
        return 0
    if len(a.variant) < 2 or a.output is None:
        p.error("Supply at least two --variant entries and a new --output directory")
    if a.frames < 1 or a.warmup_frames < 0 or a.pairs < 1 or a.warmup_pairs < 0 or not math.isfinite(a.timeout) or a.timeout <= 0:
        p.error("Invalid frame, pair, warmup, or timeout configuration")
    if a.scene != a.checkpoint_scene:
        p.error("Checkpoint scene differs; generate and explicitly identify a HANG checkpoint before using --scene hang")
    if a.scene == "hang" and repo_path(a.checkpoint) == (FIXTURES / "state.csv").resolve():
        p.error("HANG requires a separate checkpoint; the default state.csv is DRAPE")
    variants = {}
    baseline_bytes = repo_path(a.baseline_source).read_bytes()
    baseline_code = parse_source(baseline_bytes)
    checkpoint = repo_path(a.checkpoint).read_bytes()
    manifest_path = FIXTURES / "baseline.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else {}
    expected_source = a.expected_source_sha256 or manifest.get("source_sha256")
    expected_checkpoint = a.expected_checkpoint_sha256 or (manifest.get("checkpoint_sha256") if a.scene == "drape" else None)
    if expected_source and sha256(baseline_bytes) != expected_source.lower():
        raise ValueError("Frozen baseline source differs from its declared checksum")
    if expected_checkpoint and sha256(checkpoint) != expected_checkpoint.lower():
        raise ValueError("Checkpoint differs from its declared checksum; identify a new checkpoint explicitly")
    if a.scene == "hang" and not a.expected_checkpoint_sha256:
        p.error("Supply --expected-checkpoint-sha256 to freeze the HANG checkpoint")
    rows = [[float(cell) for cell in row] for row in csv.reader(checkpoint.decode("utf-8-sig").splitlines())]
    if len(rows) != 1435 or any(len(row) != 6 or any(not math.isfinite(value) for value in row) for row in rows):
        raise ValueError("Checkpoint must contain 1435 finite rows of X,Y,Z,VX,VY,VZ")
    output = repo_path(a.output)
    if output.exists():
        p.error("--output must not exist; preserve each frozen study instead of overwriting it")
    for label, executable, source in a.variant:
        if not LABEL.fullmatch(label) or label in variants:
            p.error(f"Invalid or duplicate variant label {label!r}")
        exe, src = repo_path(executable), repo_path(source)
        code = parse_source(src.read_bytes())
        source_contract(code, baseline_code)
        variants[label] = {"executable": str(exe), "source": str(src), "exe_sha256": sha256(exe.read_bytes()),
                           "source_sha256": sha256(src.read_bytes()), "program": make_program(code, a.frames, a.warmup_frames, label in a.gtriangle, a.scene)}
    if set(a.gtriangle) - variants.keys():
        p.error("Unknown --gtriangle label")
    labels = list(variants)
    comparisons = [tuple(pair) for pair in a.compare] if a.compare else list(zip(labels, labels[1:]))
    if not comparisons or any(left == right or left not in variants or right not in variants for left, right in comparisons):
        p.error("Invalid comparison labels")
    schedule = []
    for comparison_index, (left, right) in enumerate(comparisons):
        for phase, count in [("warmup", a.warmup_pairs), ("measured", a.pairs)]:
            for pair in range(count):
                order = [left, right] if pair % 2 == 0 else [right, left]
                for position, label in enumerate(order):
                    schedule.append({"variant": label, "comparison": comparison_index, "phase": phase, "pair": pair + 1, "position": position + 1})
    output.mkdir(parents=True)
    configuration = {"window": a.window, "scene": a.scene, "pixels": [800, 600], "mesh": [40, 34],
                     "steps_per_frame": 6, "H": 1 / 180, "measured_frames": a.frames, "warmup_frames": a.warmup_frames,
                     "pairs": a.pairs, "warmup_pairs": a.warmup_pairs, "timeout": a.timeout,
                     "timer": "external stdout-reader perf_counter timestamps", "baseline_source_sha256": sha256(baseline_bytes),
                     "checkpoint_sha256": sha256(checkpoint), "checkpoint_path": str(repo_path(a.checkpoint)),
                     "comparison_order": comparisons, "gtriangle_transform": a.gtriangle,
                     "runner_sha256": sha256(Path(__file__).read_bytes()), "python": sys.version,
                     "platform": platform.platform(), "processor": platform.processor(), "logical_cpus": os.cpu_count()}
    plan = {"configuration": configuration, "variants": {label: {key: value for key, value in entry.items() if key != "program"} for label, entry in variants.items()}, "schedule": schedule}
    write_json(output / "plan.json", plan)
    for label, entry in variants.items():
        (output / f"{label}.bas").write_bytes(entry["program"])
    (output / "checkpoint.csv").write_bytes(checkpoint)
    if a.prepare_only:
        print(f"Prepared {len(schedule)} serial runs; no interpreter was executed: {output}")
        return 0
    results, equalities, references = [], [], {}
    try:
        for number, metadata in enumerate(schedule, 1):
            label = metadata["variant"]
            entry = variants[label]
            print(f"[{number}/{len(schedule)}] {metadata['phase']} comparison {metadata['comparison']+1}, pair {metadata['pair']}: {label}", flush=True)
            result = run_one(Path(entry["executable"]), output / f"run-{number:03d}-{label}", checkpoint,
                             entry["program"], a.frames, a.warmup_frames, a.window, a.timeout, a.scene, metadata)
            if sha256(Path(entry["executable"]).read_bytes()) != entry["exe_sha256"]:
                raise ValueError(f"Executable changed during the frozen study: {label}")
            reference = references.setdefault(label, result)
            same = equality(reference, result)
            if not same["csv_exact"] or not same["pixels_exact"] or not same["png_bytes_exact"]:
                raise ValueError(f"Nondeterministic state/image within variant {label}: {same}")
            results.append(result)
            write_json(output / "progress.json", {**plan, "runs": results})
        for left, right in comparisons:
            check = equality(references[left], references[right])
            equalities.append({"baseline": left, "candidate": right, **check})
        result = {**plan, "runs": results, "summary": summarize(results, comparisons), "equality": equalities,
                  "semantically_exact": all(e["csv_exact"] and e["pixels_exact"] and e["png_bytes_exact"] for e in equalities)}
        write_json(output / "results.json", result)
        (output / "results.md").write_text(spanish_report(result), encoding="utf-8")
        print(json.dumps(result["summary"], indent=2), flush=True)
        print(f"Semantic/image equality: {result['semantically_exact']}; evidence: {output / 'results.json'}")
        return 0 if result["semantically_exact"] else 2
    except Exception as exc:
        write_json(output / "failure.json", {**plan, "runs": results, "error": str(exc)})
        raise


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, RuntimeError) as error:
        print(f"cloth_viability: {error}", file=sys.stderr)
        raise SystemExit(1)

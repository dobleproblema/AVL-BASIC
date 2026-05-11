"""Compare Rust graphical framebuffer behavior with the Python oracle.

The Python repository remains read-only. This runner executes small,
deterministic BASIC graphics programs in both interpreters, extracts explicit
framebuffer markers printed by the programs, and compares serialized GSCR
strings (`<width>x<height>:rrggbb...`) exactly.

Default paths assume the monorepo layout:
    AVL-BASIC/       Python reference implementation
    AVL-BASIC/rust/  Rust implementation

Examples:
    python tools/run_python_graphics_parity.py --mode summary
    python tools/run_python_graphics_parity.py --mode smoke --rust-bin target/release/avl-basic.exe
    python tools/run_python_graphics_parity.py --mode direct --rust-bin target/release/avl-basic.exe
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from run_python_direct_regression_parity import (
    extract_direct_cases,
    normalize_session_output,
    setup_tmp_path_case,
)


PROJECT_DIR = Path(__file__).resolve().parents[1]


def default_python_repo() -> Path:
    parent = PROJECT_DIR.parent
    if (parent / "basic.py").exists():
        return parent
    return parent / "AVL-BASIC"


DEFAULT_PY_REPO = default_python_repo()
DEFAULT_RUST_BIN = PROJECT_DIR / "target" / "debug" / (
    "avl-basic.exe" if os.name == "nt" else "avl-basic"
)

GRAPHICS_RE = re.compile(
    r"\b("
    r"SCREEN|CLG|PLOT|PLOTR|DRAW|DRAWR|MOVE|MOVER|MODE|INK|PAPER|"
    r"SPRITE|BLOAD|BSAVE|CIRCLE|FCIRCLE|CIRCLER|FCIRCLER|RECTANGLE|"
    r"FRECTANGLE|TRIANGLE|FTRIANGLE|FILL|FRAME|GDISP|LDIR|PENWIDTH|"
    r"MASK|GRAPH|GRAPHRANGE|XAXIS|YAXIS|CROSSAT|SCALE|ORIGIN|"
    r"MOUSE|KEYDOWN|COLMODE|COLCOLOR|COLRESET|HIT|HITCOLOR|"
    r"HITSPRITE|HITID"
    r")\b|SCREEN\$|SPRITE\$|INKEY\$|TEST\s*\(",
    re.IGNORECASE,
)

CAPTURE_MARKER_RE = re.compile(
    r"^__AVL_GRAPHICS_(SCREEN|SPRITE)__=([A-Za-z0-9_.-]+)=(.*)$"
)
VALUE_MARKER_RE = re.compile(r"^__AVL_GRAPHICS_VALUE__=([A-Za-z0-9_.-]+)=(.*)$")


@dataclass(frozen=True)
class GraphicsCase:
    name: str
    program: str
    description: str


@dataclass
class CapturedRun:
    text: str
    captures: dict[tuple[str, str], str]
    values: dict[str, str]


GRAPHICS_SMOKE_CASES = [
    GraphicsCase(
        name="screen_plot_test",
        description="full-screen capture after a single palette plot",
        program=r"""
10 SCREEN
20 MODE 640
30 PAPER 0 : CLG
40 PLOT 1,1,2
50 PRINT "__AVL_GRAPHICS_SCREEN__=screen="+SCREEN$
60 PRINT "__AVL_GRAPHICS_VALUE__=test_pixel=";TEST(1,1)
70 END
""",
    ),
    GraphicsCase(
        name="screen_preserves_framebuffer",
        description="bare SCREEN presents existing graphics without clearing the framebuffer",
        program=r"""
10 MODE 640 : PAPER 0 : CLG
20 PLOT 3,4,2
30 SCREEN
40 PRINT "__AVL_GRAPHICS_SPRITE__=after_screen="+SPRITE$(0,0,8,8)
50 END
""",
    ),
    GraphicsCase(
        name="screen_resets_state_without_clearing",
        description="bare SCREEN resets graphics state but preserves existing pixels",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 INK 2 : PLOT 1,1
30 SCREEN
40 PLOT 2,1
50 PRINT "__AVL_GRAPHICS_SPRITE__=state="+SPRITE$(0,0,4,2)
60 END
""",
    ),
    GraphicsCase(
        name="line_rect_sprite_region",
        description="line, outline rectangle, filled rectangle, and regional SPRITE$",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 INK 2 : DRAW 0,0,10,0
30 RECTANGLE 2,2,6,5,3
40 FRECTANGLE 8,2,10,4,4
50 PRINT "__AVL_GRAPHICS_SPRITE__=region="+SPRITE$(0,0,12,6)
60 END
""",
    ),
    GraphicsCase(
        name="masked_line",
        description="MASK phase and line rasterization on a small region",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 MASK 170
30 MOVE 0,0 : DRAW 15,0,2
40 MASK
50 PRINT "__AVL_GRAPHICS_SPRITE__=masked="+SPRITE$(0,0,15,0)
60 END
""",
    ),
    GraphicsCase(
        name="triangle_color_strings",
        description="named colors plus outline and filled triangle rasterization",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 TRIANGLE 1,1,7,1,4,5,"cyan"
30 FTRIANGLE 10,1,16,1,13,5,"gold"
40 PRINT "__AVL_GRAPHICS_SPRITE__=triangles="+SPRITE$(0,0,17,6)
50 END
""",
    ),
    GraphicsCase(
        name="scale_center_plot",
        description="SCALE coordinate mapping and TEST on the active scale",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE -10,10,-10,10
30 PLOT 0,0,2
40 PRINT "__AVL_GRAPHICS_SCREEN__=screen="+SCREEN$
50 PRINT "__AVL_GRAPHICS_VALUE__=scaled_center=";TEST(0,0)
60 END
""",
    ),
    GraphicsCase(
        name="sprite_collision_id",
        description="explicit sprite ids and HIT/HITSPRITE/HITID state",
        program=r"""
10 SCREEN : CLG
20 COLMODE 2
30 SPRITE "1x1:00ff00",10,10,0,12
40 SPRITE "1x1:ff0000",10,10,0,99
50 PRINT "__AVL_GRAPHICS_VALUE__=hit=";HIT;HITSPRITE;HITID
60 END
""",
    ),
    GraphicsCase(
        name="embedded_small_font",
        description="embedded small bitmap font including extended glyphs",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG : SMALLFONT
20 LOCATE 0,0 : DISP "AÑá€",1,0
30 PRINT "__AVL_GRAPHICS_SPRITE__=font="+SPRITE$(0,HEIGHT-16,31,HEIGHT-1)
40 END
""",
    ),
    GraphicsCase(
        name="embedded_big_font_transparent",
        description="embedded big font and transparent negative paper",
        program=r"""
10 SCREEN : MODE 640 : PAPER 4 : CLG : BIGFONT
20 LOCATE 0,0 : DISP "A",1,-1
30 PRINT "__AVL_GRAPHICS_SPRITE__=font="+SPRITE$(0,HEIGHT-16,15,HEIGHT-1)
40 END
""",
    ),
    GraphicsCase(
        name="mode_preserves_big_font",
        description="MODE preserves BIGFONT selected before the resize",
        program=r"""
10 SCREEN : PAPER 0 : BIGFONT : MODE 640
20 LOCATE 0,0 : DISP "A",1,0
30 PRINT "__AVL_GRAPHICS_SPRITE__=font="+SPRITE$(0,HEIGHT-16,15,HEIGHT-1)
40 END
""",
    ),
    GraphicsCase(
        name="penwidth_plot_sizes",
        description="PENWIDTH 1/2/4 plot footprints",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 PENWIDTH 4 : PLOT 10,10,2
30 PENWIDTH 2 : PLOT 20,10,3
40 PENWIDTH 1 : PLOT 28,10,4
50 PRINT "__AVL_GRAPHICS_SPRITE__=pens="+SPRITE$(6,6,29,13)
60 END
""",
    ),
    GraphicsCase(
        name="origin_viewport_clip",
        description="ORIGIN clipping window limits drawing to its viewport",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 ORIGIN 0,0,100,200,200,100
30 PLOT 50,150,2
40 PLOT 150,150,3
50 ORIGIN 0,0
60 PRINT "__AVL_GRAPHICS_SPRITE__=origin="+SPRITE$(40,140,160,160)
70 END
""",
    ),
    GraphicsCase(
        name="origin_clg_viewport_only",
        description="CLG clears only the active ORIGIN viewport",
        program=r"""
10 SCREEN : MODE 640 : PAPER 1 : CLG
20 ORIGIN 0,0,10,20,20,10
30 PAPER 2 : CLG
40 ORIGIN 0,0
50 PRINT "__AVL_GRAPHICS_SPRITE__=viewport="+SPRITE$(8,8,22,22)
60 END
""",
    ),
    GraphicsCase(
        name="axis_tick_pixel_length",
        description="XAXIS major and subdivision ticks use fixed pixel lengths",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE -1,1,-1,1
30 XAXIS 1,-1,1,1,1
40 SCALE
50 PRINT "__AVL_GRAPHICS_SPRITE__=ticks="+SPRITE$(315,235,325,245)
60 END
""",
    ),
    GraphicsCase(
        name="axis_vertical_labels_and_y_crossing_skip",
        description="vertical XAXIS labels and YAXIS label suppression at the X-axis crossing",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE 1944,1978,48,69,50
30 CROSSAT 1944,48
40 XAXIS 4,,,,1,4
50 YAXIS 1,,,,2
60 SCALE
70 PRINT "__AVL_GRAPHICS_SPRITE__=axislabels="+SPRITE$(20,390,125,459)
80 END
""",
    ),
    GraphicsCase(
        name="xaxis_label_centered_before_crossat",
        description="XAXIS drawn before CROSSAT centers its first label instead of treating it as a Y-axis crossing",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 B=80
30 SCALE 1960,1990,0,200,B
40 XAXIS 10,,,,,10
50 CROSSAT 1960,0
60 YAXIS 30
70 SCALE
80 PRINT "__AVL_GRAPHICS_SPRITE__=firstlabel="+SPRITE$(55,390,125,430)
90 END
""",
    ),
    GraphicsCase(
        name="filled_circle_sprite_shape",
        description="FCIRCLE scanline fill matches sprite-collision silhouettes",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG : SMALLFONT
20 R=9 : INK 2 : FCIRCLE 40,40,R : MOVE 36,46 : GDISP "1",0,-1
30 PRINT "__AVL_GRAPHICS_SPRITE__=ball="+SPRITE$(40-R,40-R,40+R,40+R)
40 END
""",
    ),
    GraphicsCase(
        name="filled_circle_sector",
        description="FCIRCLE supports Python-compatible filled sectors",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG : DEG
20 FCIRCLE 24,24,10,2,0,90
30 PRINT "__AVL_GRAPHICS_SPRITE__=sector="+SPRITE$(12,12,36,36)
40 END
""",
    ),
    GraphicsCase(
        name="filled_circle_aspect_five_args",
        description="the fifth FCIRCLE argument is aspect, not an arc angle",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 FCIRCLE 30,30,10,2,2
30 PRINT "__AVL_GRAPHICS_SPRITE__=ellipse="+SPRITE$(8,18,52,42)
40 END
""",
    ),
    GraphicsCase(
        name="scaled_relative_draw_cursor",
        description="DRAWR updates the user-coordinate cursor while SCALE is active",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE -10,10,-10,10
30 MOVE -5,0
40 DRAWR 1,0,2
50 DRAWR 1,0,3
60 PRINT "__AVL_GRAPHICS_SPRITE__=relative="+SPRITE$(-5,0,-3,0)
70 END
""",
    ),
    GraphicsCase(
        name="graph_range_clips_active_plot",
        description="GRAPHRANGE constrains GRAPH to its active plotting rectangle",
        program=r"""
10 DEF FNX(X)=1/X
20 SCREEN : MODE 640 : PAPER 0 : CLG
30 SCALE -10,2,-10,2,50
40 INK 4 : PENWIDTH 4
50 GRAPHRANGE -2,-0.5,-2,0
60 GRAPH FNX(X)
70 SCALE
80 PRINT "__AVL_GRAPHICS_SPRITE__=range="+SPRITE$(400,105,485,185)
90 END
""",
    ),
    GraphicsCase(
        name="graph_mask_phase_continues_across_segments",
        description="GRAPH carries MASK phase across short line segments instead of restarting each segment",
        program=r"""
10 DEF FNX(X)=1/X
20 SCREEN : MODE 640 : PAPER 0 : CLG
30 SCALE -10,2,-10,2,50
40 XAXIS -1,-8.5,0,1
50 YAXIS -1,-8.5,0,1
60 INK 2 : PENWIDTH 4 : MASK &X10000001
70 GRAPH FNX(X)
80 SCALE
90 PRINT "__AVL_GRAPHICS_SPRITE__=graphmask="+SPRITE$(130,120,420,155)
100 END
""",
    ),
    GraphicsCase(
        name="graph_skips_vertical_asymptote_bridge",
        description="GRAPH avoids drawing a false segment between TAN branches around a vertical asymptote",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE -PI,PI,-10,10,20
30 INK 2 : PENWIDTH 2
40 GRAPH TAN(X),0.01
50 SCALE
60 PRINT "__AVL_GRAPHICS_SPRITE__=tan="+SPRITE$(450,20,490,459)
70 END
""",
    ),
    GraphicsCase(
        name="graph_uses_explicit_axis_ranges",
        description="GRAPH keeps explicit XAXIS/YAXIS ranges after later decorative axes",
        program=r"""
10 DEF FNX(X)=1/X
20 SCREEN : MODE 640 : PAPER 0 : CLG
30 SCALE -10,2,-10,2,50
40 INK 0 : XAXIS 0,-8.5,0,-1
50 YAXIS 0,-8.5,0,-1
60 CROSSAT 0,2 : XAXIS 0 : CROSSAT 2,0 : YAXIS 0
70 INK 2
80 GRAPH FNX(X)
90 SCALE
100 PRINT "__AVL_GRAPHICS_SPRITE__=axisrange="+SPRITE$(105,125,515,430)
110 END
""",
    ),
    GraphicsCase(
        name="hitcolor_resets_after_hittest",
        description="HITCOLOR returns 0 after a later non-colliding HITTEST",
        program=r"""
10 SCREEN : MODE 640 : PAPER 0 : CLG
20 PLOT 10,10,1
30 COLMODE 1 : COLCOLOR 1
40 SPRITE HITTEST "1x1:ff0000",10,10,0,1
50 PRINT "__AVL_GRAPHICS_VALUE__=first=";HITCOLOR
60 SPRITE HITTEST "1x1:ff0000",20,20,0,1
70 PRINT "__AVL_GRAPHICS_VALUE__=second=";HITCOLOR
80 END
""",
    ),
]


def python_repo_from_args(value: str | None) -> Path:
    if value:
        return Path(value).resolve()
    return Path(os.environ.get("AVL_BASIC_PY_REPO", DEFAULT_PY_REPO)).resolve()


def rust_bin_from_args(value: str | None) -> Path:
    if value:
        return Path(value).resolve()
    return Path(os.environ.get("AVL_BASIC_RUST_BIN", DEFAULT_RUST_BIN)).resolve()


def run_process(command: list[str], stdin: str, cwd: Path, timeout: float) -> str:
    proc = subprocess.run(
        command,
        input=stdin,
        cwd=str(cwd),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    return normalize_session_output(proc.stdout, strip_prompt_space=False)


def case_stdin(program: str) -> str:
    lines = [line.rstrip() for line in program.strip().splitlines() if line.strip()]
    return "\n".join(["NEW", *lines, "RUN", "EXIT"]) + "\n"


def parse_marked_output(output: str) -> CapturedRun:
    text_lines: list[str] = []
    captures: dict[tuple[str, str], str] = {}
    values: dict[str, str] = {}

    for line in output.splitlines():
        capture_match = CAPTURE_MARKER_RE.match(line)
        if capture_match:
            kind, name, payload = capture_match.groups()
            captures[(kind, name)] = payload
            continue
        value_match = VALUE_MARKER_RE.match(line)
        if value_match:
            name, payload = value_match.groups()
            values[name] = payload
            continue
        if line:
            text_lines.append(line)

    return CapturedRun("\n".join(text_lines).rstrip("\n"), captures, values)


def parse_gscr(gscr: str) -> tuple[int, int, str]:
    try:
        resolution, hexdata = gscr.split(":", 1)
        width_s, height_s = resolution.split("x", 1)
        width = int(width_s)
        height = int(height_s)
    except Exception as exc:
        raise ValueError(f"invalid GSCR header: {gscr[:80]!r}") from exc
    expected_len = width * height * 6
    if len(hexdata) != expected_len:
        raise ValueError(
            f"invalid GSCR length for {width}x{height}: "
            f"got {len(hexdata)}, expected {expected_len}"
        )
    return width, height, hexdata


def sha256_short(text: str) -> str:
    return hashlib.sha256(text.encode("ascii", errors="strict")).hexdigest()[:16]


def gscr_diff_summary(expected: str, actual: str) -> str:
    try:
        ew, eh, ehex = parse_gscr(expected)
    except ValueError as exc:
        return f"python capture is not valid GSCR: {exc}"
    try:
        aw, ah, ahex = parse_gscr(actual)
    except ValueError as exc:
        return f"rust capture is not valid GSCR: {exc}"

    lines = [
        f"python_sha256={sha256_short(expected)} rust_sha256={sha256_short(actual)}",
        f"python_size={ew}x{eh} rust_size={aw}x{ah}",
    ]
    if (ew, eh) != (aw, ah):
        return "\n".join(lines)

    mismatch_count = 0
    first: list[str] = []
    for pixel in range(ew * eh):
        offset = pixel * 6
        ergb = ehex[offset : offset + 6]
        argb = ahex[offset : offset + 6]
        if ergb == argb:
            continue
        mismatch_count += 1
        if len(first) < 8:
            x = pixel % ew
            y = pixel // ew
            first.append(f"({x},{y}) py=#{ergb} rust=#{argb}")
    lines.append(f"pixel_mismatches={mismatch_count}")
    if first:
        lines.append("first_mismatches=" + "; ".join(first))
    return "\n".join(lines)


def write_ppm(path: Path, gscr: str) -> None:
    width, height, hexdata = parse_gscr(gscr)
    rgb = bytes.fromhex(hexdata)
    path.write_bytes(f"P6\n{width} {height}\n255\n".encode("ascii") + rgb)


def dump_capture_pair(
    dump_dir: Path | None,
    case_name: str,
    kind: str,
    capture_name: str,
    expected: str,
    actual: str,
) -> None:
    if dump_dir is None:
        return
    dump_dir.mkdir(parents=True, exist_ok=True)
    base = f"{case_name}.{kind.lower()}.{capture_name}"
    (dump_dir / f"{base}.python.gscr").write_text(expected, encoding="ascii")
    (dump_dir / f"{base}.rust.gscr").write_text(actual, encoding="ascii")
    try:
        write_ppm(dump_dir / f"{base}.python.ppm", expected)
        write_ppm(dump_dir / f"{base}.rust.ppm", actual)
    except ValueError:
        pass


def compare_captured_runs(
    case: GraphicsCase,
    expected: CapturedRun,
    actual: CapturedRun,
    dump_dir: Path | None,
) -> list[str]:
    failures: list[str] = []
    if actual.text != expected.text:
        failures.append(
            "text output mismatch\n"
            f"--- python ---\n{expected.text}\n"
            f"--- rust ---\n{actual.text}"
        )

    if actual.values != expected.values:
        failures.append(
            "value marker mismatch\n"
            f"python={expected.values!r}\n"
            f"rust={actual.values!r}"
        )

    expected_keys = set(expected.captures)
    actual_keys = set(actual.captures)
    if expected_keys != actual_keys:
        failures.append(
            "capture marker set mismatch\n"
            f"python={sorted(expected_keys)!r}\n"
            f"rust={sorted(actual_keys)!r}"
        )

    for key in sorted(expected_keys & actual_keys):
        expected_capture = expected.captures[key]
        actual_capture = actual.captures[key]
        if expected_capture == actual_capture:
            continue
        kind, capture_name = key
        dump_capture_pair(
            dump_dir, case.name, kind, capture_name, expected_capture, actual_capture
        )
        failures.append(
            f"{kind.lower()} capture {capture_name!r} mismatch\n"
            + gscr_diff_summary(expected_capture, actual_capture)
        )

    return failures


def run_smoke_cases(args: argparse.Namespace, py_repo: Path) -> int:
    rust_bin = rust_bin_from_args(args.rust_bin)
    if not rust_bin.exists():
        print(f"Rust binary not found: {rust_bin}", file=sys.stderr)
        return 2

    dump_dir = Path(args.dump_dir).resolve() if args.dump_dir else None
    failures: list[tuple[str, list[str], str]] = []
    for case in GRAPHICS_SMOKE_CASES:
        stdin = case_stdin(case.program)
        try:
            expected_output = run_process(
                [sys.executable, "-X", "utf8", str(py_repo / "basic.py")],
                stdin,
                PROJECT_DIR,
                args.timeout,
            )
            actual_output = run_process([str(rust_bin)], stdin, PROJECT_DIR, args.timeout)
        except subprocess.TimeoutExpired as exc:
            failures.append((case.name, [f"timeout while running {exc.cmd!r}"], ""))
            continue

        expected = parse_marked_output(expected_output)
        actual = parse_marked_output(actual_output)
        case_failures = compare_captured_runs(case, expected, actual, dump_dir)
        if case_failures:
            failures.append((case.name, case_failures, case.description))
            if len(failures) >= args.max_failures:
                break

    if failures:
        print(
            f"{len(failures)} graphics smoke mismatch(es) before stopping "
            f"(selected={len(GRAPHICS_SMOKE_CASES)})"
        )
        for name, case_failures, description in failures:
            print()
            print(f"case={name}")
            if description:
                print(f"description={description}")
            for failure in case_failures:
                print("--- failure ---")
                print(failure)
        if dump_dir is not None:
            print()
            print(f"wrote mismatch artifacts to {dump_dir}")
        return 1

    print(f"ok mode=smoke selected={len(GRAPHICS_SMOKE_CASES)}")
    return 0


def run_direct_graphics_cases(args: argparse.Namespace, py_repo: Path) -> int:
    rust_bin = rust_bin_from_args(args.rust_bin)
    if not rust_bin.exists():
        print(f"Rust binary not found: {rust_bin}", file=sys.stderr)
        return 2

    cases = [case for case in extract_direct_cases(py_repo) if case.graphics]
    failures: list[tuple[str, str, str, list[str]]] = []
    for case in cases:
        stdin = "\n".join(case.commands) + "\n"
        tmp_context = tempfile.TemporaryDirectory(prefix="avl_basic_graphics_direct_")
        try:
            cwd = PROJECT_DIR
            if case.requires_tmp_path:
                cwd = Path(tmp_context.name)
                setup_tmp_path_case(case, cwd)
            else:
                tmp_context.cleanup()
                tmp_context = None

            expected = run_process(
                [sys.executable, "-X", "utf8", str(py_repo / "basic.py")],
                stdin,
                cwd,
                args.timeout,
            )
            actual = run_process([str(rust_bin)], stdin, cwd, args.timeout)
        except subprocess.TimeoutExpired:
            failures.append((case.label, "<timeout>", "", case.commands))
            continue
        finally:
            if tmp_context is not None:
                tmp_context.cleanup()

        if actual != expected:
            failures.append((case.label, expected, actual, case.commands))
            if len(failures) >= args.max_failures:
                break

    if failures:
        print(
            f"{len(failures)} direct graphics mismatch(es) before stopping "
            f"(selected={len(cases)})"
        )
        for label, expected, actual, commands in failures:
            print()
            print(f"case={label}")
            print("--- commands ---")
            print("\n".join(commands))
            print("--- python oracle ---")
            print(expected)
            print("--- rust ---")
            print(actual)
        return 1

    print(f"ok mode=direct selected={len(cases)}")
    return 0


def graphics_sample_count(py_repo: Path) -> int:
    samples = py_repo / "samples"
    if not samples.exists():
        return 0
    count = 0
    for path in samples.rglob("*.bas"):
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            text = path.read_text(encoding="latin-1")
        if GRAPHICS_RE.search(text):
            count += 1
    return count


def print_summary(py_repo: Path) -> int:
    direct_cases = extract_direct_cases(py_repo)
    direct_graphics = [case for case in direct_cases if case.graphics]
    print(f"graphics_smoke_cases={len(GRAPHICS_SMOKE_CASES)}")
    print(f"direct_session_cases={len(direct_cases)}")
    print(f"direct_graphics_cases={len(direct_graphics)}")
    print(f"graphics_sample_files={graphics_sample_count(py_repo)}")
    for case in GRAPHICS_SMOKE_CASES:
        print(f"smoke_case={case.name}: {case.description}")
    for case in direct_graphics:
        print(f"direct_graphics_case={case.label}")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--mode",
        choices=("summary", "smoke", "direct", "all"),
        default="smoke",
        help="summary only, deterministic framebuffer smoke cases, direct graphics sessions, or both",
    )
    parser.add_argument("--py-repo", help="Path to the Python BASIC repository")
    parser.add_argument("--rust-bin", help="Path to the avl-basic Rust binary")
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--max-failures", type=int, default=20)
    parser.add_argument(
        "--dump-dir",
        help="Optional directory for mismatched .gscr and .ppm artifacts",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    py_repo = python_repo_from_args(args.py_repo)
    if not (py_repo / "basic.py").exists():
        print(f"Python oracle basic.py not found in {py_repo}", file=sys.stderr)
        return 2

    if args.mode == "summary":
        return print_summary(py_repo)
    if args.mode == "smoke":
        return run_smoke_cases(args, py_repo)
    if args.mode == "direct":
        return run_direct_graphics_cases(args, py_repo)

    smoke_status = run_smoke_cases(args, py_repo)
    direct_status = run_direct_graphics_cases(args, py_repo)
    return 1 if smoke_status or direct_status else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

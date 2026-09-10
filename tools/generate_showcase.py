"""Generate the AVL BASIC showcase images from real interpreter runs.

The source samples are never edited.  For graphical captures, this tool writes
a short-lived instrumented copy next to the original so relative assets keep
working, runs it with the graphics window disabled, and removes the copy.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError as exc:
    raise SystemExit(
        "Pillow is required to regenerate the showcase: python -m pip install Pillow"
    ) from exc


ROOT = Path(__file__).resolve().parents[1]
SAMPLES = ROOT / "samples"
SHOWCASE = SAMPLES / "showcase"
ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


@dataclass(frozen=True)
class Capture:
    sample: str
    inject: tuple[str, ...]
    replace: dict[int, str] = field(default_factory=dict)
    timeout: int = 180

    @property
    def slug(self) -> str:
        return Path(self.sample).stem


CAPTURES = (
    Capture(
        "g-old-school.bas",
        (
            "411 SHOWFRAME=SHOWFRAME+1",
            "412 IF SHOWFRAME<80 THEN 420",
            '413 BSAVE "showcase/g-old-school.png"',
            "414 END",
        ),
        {
            230: "RANDOMIZE 20260826 : GOSUB 1000",
            370: (
                'INK 18 : LOCATE 1,1 : GPRINT "AVL-BASIC OLD-SCHOOL DEMO  '
                'STARS:";STARS;"  ESC=EXIT"'
            ),
        },
    ),
    Capture(
        "g-raytracer.bas",
        ('1392 BSAVE "showcase/g-raytracer.png"',),
        timeout=300,
    ),
    Capture(
        "g-bigbang.bas",
        (
            "421 IF F<220 THEN 430",
            '422 BSAVE "showcase/g-bigbang.png"',
            "423 END",
        ),
        {
            410: (
                'IF HUD THEN INK 18:LOCATE 1,1:GPRINT "AVL-BASIC BIG BANG  '
                'PART:";PST;"  PARTICLES:";NP;"  ESC=EXIT"'
            )
        },
    ),
    Capture(
        "g-arkanoid.bas",
        (
            "1561 SHOWFRAME=SHOWFRAME+1",
            "1562 IF SHOWFRAME<80 THEN 1570",
            '1563 BSAVE "showcase/g-arkanoid.png"',
            "1564 END",
        ),
        {
            1010: "RANDOMIZE 20260826 : T1=TIME",
            1195: "REM Showcase capture skips the click gate",
        },
    ),
    Capture(
        "g-jelly.bas",
        (
            "331 SHOWFRAME=SHOWFRAME+1",
            "332 IF SHOWFRAME<24 THEN 340",
            '333 BSAVE "showcase/g-jelly.png"',
            "334 END",
        ),
        {325: 'GPRINT "AVL-BASIC JELLY  ESC=EXIT" : INK &H44ccff'},
    ),
    Capture(
        "g-cube-tquad.bas",
        (
            "751 SHOWFRAME=SHOWFRAME+1",
            "752 IF SHOWFRAME<36 THEN 760",
            '753 BSAVE "showcase/g-cube-tquad.png"',
            "754 END",
        ),
        {
            400: "T=SHOWFRAME/30",
            740: (
                'GPRINT "AVL-BASIC TEXTURED CUBE  SUB=";USING "#";SUBD;'
                '"  ESC=EXIT"'
            ),
        },
    ),
    Capture(
        "g-tunnel-tquad.bas",
        (
            "1511 SHOWFRAME=SHOWFRAME+1",
            "1512 IF SHOWFRAME<30 THEN 1520",
            '1513 BSAVE "showcase/g-tunnel-tquad.png"',
            "1514 END",
        ),
        {
            1360: "T=SHOWFRAME/30",
            1500: 'GPRINT "AVL-BASIC TEXTURED TUNNEL  ESC=EXIT"',
        },
    ),
    Capture(
        "g-gouraud.bas",
        ('427 BSAVE "showcase/g-gouraud.png"',),
        timeout=300,
    ),
    Capture(
        "g-bifurcation.bas",
        (
            '172 BSAVE "showcase/g-bifurcation.png"',
            "173 END",
        ),
        timeout=300,
    ),
    Capture(
        "g-chess960.bas",
        (
            '547 BSAVE "showcase/g-chess960.png"',
            "548 END",
        ),
        {505: "RANDOMIZE 960"},
    ),
    Capture(
        "g-fmandelbrot.bas",
        (
            '305 BSAVE "showcase/g-fmandelbrot.png"',
            "306 END",
        ),
        timeout=300,
    ),
    Capture(
        "g-maze.bas",
        ('695 BSAVE "showcase/g-maze.png"',),
        {110: "RANDOMIZE 20260826"},
    ),
    Capture(
        "g-origin-scale.bas",
        ('1005 BSAVE "showcase/g-origin-scale.png"',),
    ),
    Capture(
        "g-origin.bas",
        (
            "641 SHOWFRAME=SHOWFRAME+1",
            "642 IF SHOWFRAME<36 THEN 650",
            '643 BSAVE "showcase/g-origin.png"',
            "644 END",
        ),
    ),
    Capture(
        "g-zoomer.bas",
        (
            '801 BSAVE "showcase/g-zoomer.png"',
            "802 END",
        ),
        {
            400: "T=.4",
            800: 'GPRINT "AVL-BASIC ZOOMER-ROTATOR  ESC=EXIT"',
        },
        timeout=300,
    ),
    Capture(
        "g-dot-tunnel.bas",
        (
            "175 RANDOMIZE 20260826",
            "255 SHOWFRAME=SHOWFRAME+1",
            "535 IF SHOWFRAME<36 THEN 540",
            '536 BSAVE "showcase/g-dot-tunnel.png"',
            "537 END",
        ),
        {530: 'INK 14 : GPRINT "AVL-BASIC TWISTING DOT TUNNEL  ESC=EXIT"'},
    ),
    Capture(
        "g-balls.bas",
        (
            "395 SHOWFRAME=SHOWFRAME+1",
            "396 IF SHOWFRAME<60 THEN 400",
            '397 BSAVE "showcase/g-balls.png"',
            "398 END",
        ),
    ),
    Capture(
        "g-ftree4.bas",
        (
            "255 SHOWFRAME=SHOWFRAME+1",
            "256 IF SHOWFRAME<18 THEN 260",
            '257 BSAVE "showcase/g-ftree4.png"',
            "258 END",
        ),
        {150: "REM Showcase capture disables the wall-clock FPS timer"},
    ),
    Capture(
        "g-loan.bas",
        ('982 BSAVE "showcase/g-loan.png"',),
        {
            790: 'principal=250000:GPRINT "Loan principal      : 250000"',
            800: 'annualRate=3.5:GPRINT "Annual interest (%) : 3.5"',
            810: 'years=30:GPRINT "Term (years)        : 30"',
            955: 'resp$="Y":GPRINT "Show first-year table (Y/N)? Y"',
        },
    ),
)


def source_line_number(line: str) -> int | None:
    match = re.match(r"\s*(\d+)\b", line)
    return int(match.group(1)) if match else None


def instrumented_source(capture: Capture) -> str:
    source_path = SAMPLES / capture.sample
    lines = source_path.read_text(encoding="utf-8-sig").splitlines()
    seen: set[int] = set()
    rewritten: list[str] = []
    for line in lines:
        number = source_line_number(line)
        if number in capture.replace:
            rewritten.append(f"{number} {capture.replace[number]}")
            seen.add(number)
        else:
            rewritten.append(line)
    missing = set(capture.replace) - seen
    if missing:
        raise ValueError(f"{capture.sample}: missing replacement lines {sorted(missing)}")

    occupied = {number for line in lines if (number := source_line_number(line)) is not None}
    injected = {source_line_number(line) for line in capture.inject}
    collisions = occupied & injected
    if collisions:
        raise ValueError(f"{capture.sample}: capture line collision {sorted(collisions)}")
    return "\n".join((*rewritten, *capture.inject, ""))


def run_process(command: list[str], timeout: int, env: dict[str, str]) -> str:
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )
    if completed.returncode:
        raise RuntimeError(
            f"Command failed with exit code {completed.returncode}:\n{completed.stdout}"
        )
    return completed.stdout


def optimize_png(path: Path) -> None:
    with Image.open(path) as image:
        image.convert("RGB").save(path, format="PNG", optimize=True)


def capture_graphics(executable: Path, capture: Capture, env: dict[str, str]) -> Path:
    output = SHOWCASE / f"{capture.slug}.png"
    staging = SHOWCASE / f"__generated_{capture.slug}.png"
    temporary = SAMPLES / f"__showcase_capture_{capture.slug}.bas"
    if temporary.exists():
        raise RuntimeError(f"Refusing to overwrite temporary source: {temporary}")
    staging.unlink(missing_ok=True)
    source = instrumented_source(capture)
    final_reference = f"showcase/{output.name}"
    staging_reference = f"showcase/{staging.name}"
    if source.count(final_reference) != 1:
        raise RuntimeError(
            f"{capture.sample}: expected one capture target {final_reference}"
        )
    source = source.replace(final_reference, staging_reference)
    try:
        temporary.write_text(source, encoding="utf-8", newline="\n")
        run_process([str(executable), str(temporary)], capture.timeout, env)
        if not staging.is_file():
            raise RuntimeError(f"Capture did not create {staging}")
        optimize_png(staging)
        staging.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
        staging.unlink(missing_ok=True)
    print(f"captured {output.relative_to(ROOT)}")
    return output


def font_candidates(bold: bool) -> Iterable[Path | str]:
    if os.name == "nt":
        windows_fonts = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Fonts"
        yield windows_fonts / ("seguisb.ttf" if bold else "segoeui.ttf")
        yield windows_fonts / ("consolab.ttf" if bold else "consola.ttf")
    yield "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"


def load_font(size: int, bold: bool = False, mono: bool = False) -> ImageFont.FreeTypeFont:
    candidates: list[Path | str]
    if mono and os.name == "nt":
        windows_fonts = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Fonts"
        candidates = [windows_fonts / ("consolab.ttf" if bold else "consola.ttf")]
    elif mono:
        candidates = ["DejaVuSansMono-Bold.ttf" if bold else "DejaVuSansMono.ttf"]
    else:
        candidates = list(font_candidates(bold))
    for candidate in candidates:
        try:
            return ImageFont.truetype(str(candidate), size)
        except OSError:
            continue
    return ImageFont.load_default(size=size)


def capture_console(executable: Path, env: dict[str, str]) -> Path:
    output_path = SHOWCASE / "pimachin-modern.png"
    staging = SHOWCASE / "__generated_pimachin-modern.png"
    temporary = SAMPLES / "__showcase_capture_pimachin-modern.bas"
    capture = Capture(
        "pimachin-modern.bas",
        (),
        {2670: 'PRINT : PRINT "1,000 DIGITS CALCULATED."'},
    )
    if temporary.exists():
        raise RuntimeError(f"Refusing to overwrite temporary source: {temporary}")
    staging.unlink(missing_ok=True)
    try:
        temporary.write_text(
            instrumented_source(capture), encoding="utf-8", newline="\n"
        )
        output = run_process([str(executable), str(temporary)], 300, env)
        lines = ANSI_RE.sub("", output).rstrip().splitlines()
        if len(lines) > 28:
            lines = [*lines[:23], "...", *lines[-4:]]

        canvas = Image.new("RGB", (640, 480), "#090d18")
        draw = ImageDraw.Draw(canvas)
        draw.rectangle((0, 0, 639, 37), fill="#171c2b")
        for x, color in ((18, "#ff5f57"), (38, "#febc2e"), (58, "#28c840")):
            draw.ellipse((x - 5, 14, x + 5, 24), fill=color)
        title_font = load_font(14, bold=True, mono=True)
        text_font = load_font(11, mono=True)
        badge_font = load_font(10, bold=True)
        draw.text((82, 11), "pimachin-modern.bas", font=title_font, fill="#dce5ff")
        draw.rounded_rectangle((448, 8, 624, 29), radius=8, fill="#27314b")
        draw.text(
            (462, 11),
            "ACTUAL OUTPUT · 1,000 DIGITS",
            font=badge_font,
            fill="#8bd5ff",
        )
        draw.text(
            (16, 50),
            "> avl-basic samples/pimachin-modern.bas",
            font=text_font,
            fill="#7ee787",
        )
        y = 72
        for line in lines:
            draw.text((16, y), line, font=text_font, fill="#edf2ff")
            y += 14
        canvas.save(staging, format="PNG", optimize=True)
        staging.replace(output_path)
    finally:
        temporary.unlink(missing_ok=True)
        staging.unlink(missing_ok=True)
    print(f"captured {output_path.relative_to(ROOT)}")
    return output_path


def rounded_panel(
    canvas: Image.Image,
    source: Path,
    box: tuple[int, int, int, int],
    label: str,
) -> None:
    x, y, width, height = box
    with Image.open(source) as image:
        panel = image.convert("RGB").resize((width, height), Image.Resampling.LANCZOS)
    mask = Image.new("L", (width, height), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, width - 1, height - 1), 18, fill=255)
    canvas.paste(panel, (x, y), mask)
    overlay = Image.new("RGBA", (width, 58), (5, 8, 18, 210))
    canvas.paste(overlay, (x, y + height - 58), overlay)
    draw = ImageDraw.Draw(canvas)
    draw.text(
        (x + 20, y + height - 45),
        label,
        font=load_font(24, bold=True),
        fill="#ffffff",
    )


def build_readme_hero() -> Path:
    required = {
        "old-school": SHOWCASE / "g-old-school.png",
        "raytracer": SHOWCASE / "g-raytracer.png",
        "arkanoid": SHOWCASE / "g-arkanoid.png",
    }
    missing = [path for path in required.values() if not path.is_file()]
    if missing:
        raise RuntimeError(f"Cannot build README hero; missing: {missing}")

    canvas = Image.new("RGB", (1600, 980), "#070a12")
    draw = ImageDraw.Draw(canvas)
    draw.rounded_rectangle((50, 36, 276, 77), 18, fill="#522047")
    draw.text((72, 47), "NATIVE RUST RUNTIME", font=load_font(17, bold=True), fill="#ffcf70")
    draw.text((50, 88), "AVL BASIC", font=load_font(66, bold=True), fill="#f8f7ff")
    draw.text(
        (420, 112),
        "Classic immediacy. Serious graphics.",
        font=load_font(30),
        fill="#aeb8d6",
    )
    draw.text(
        (1240, 118),
        f"20 highlights · {sum(1 for _ in SAMPLES.glob('*.bas'))} programs",
        font=load_font(19, bold=True),
        fill="#7ee787",
    )
    rounded_panel(canvas, required["old-school"], (50, 210, 920, 690), "OLD-SCHOOL DEMO")
    rounded_panel(canvas, required["raytracer"], (1040, 170, 480, 360), "RAY TRACER")
    rounded_panel(canvas, required["arkanoid"], (1040, 570, 480, 360), "AVL ARKANOID")
    output = ROOT / "README.png"
    staging = ROOT / "README.__generated.png"
    staging.unlink(missing_ok=True)
    try:
        canvas.save(staging, format="PNG", optimize=True)
        staging.replace(output)
    finally:
        staging.unlink(missing_ok=True)
    print(f"built {output.relative_to(ROOT)}")
    return output


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    default_name = "avl-basic.exe" if os.name == "nt" else "avl-basic"
    parser.add_argument(
        "--executable",
        type=Path,
        default=ROOT / "target" / "release" / default_name,
        help="interpreter binary to run",
    )
    parser.add_argument(
        "--only",
        action="append",
        metavar="SLUG",
        help="capture only one slug (repeatable); skips the README hero",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    executable = args.executable.resolve()
    if not executable.is_file():
        raise SystemExit(f"Missing interpreter executable: {executable}")
    SHOWCASE.mkdir(parents=True, exist_ok=True)
    selected = set(args.only or ())
    known = {capture.slug for capture in CAPTURES} | {"pimachin-modern"}
    unknown = selected - known
    if unknown:
        raise SystemExit(f"Unknown showcase slug(s): {', '.join(sorted(unknown))}")

    env = os.environ.copy()
    env["AVL_BASIC_WINDOW"] = "0"
    for capture in CAPTURES:
        if not selected or capture.slug in selected:
            capture_graphics(executable, capture, env)
    if not selected or "pimachin-modern" in selected:
        capture_console(executable, env)
    if not selected:
        build_readme_hero()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

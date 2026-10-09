"""Build a Windows end-user ZIP for AVL BASIC.

The ZIP is intentionally for people who do not have Rust or Cargo installed. It
contains the native Rust executable plus the manuals and samples.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RELEASE_DIR = ROOT / "release"


def language_version() -> str:
    match = re.search(
        r'^version\s*=\s*"([^"]+)"',
        (ROOT / "Cargo.toml").read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    if not match:
        raise SystemExit("Could not read version from Cargo.toml")
    return match.group(1)


def run(command: list[str], cwd: Path) -> None:
    subprocess.run(command, cwd=str(cwd), check=True)


def copy_tree(src: Path, dst: Path) -> None:
    ignore_patterns = shutil.ignore_patterns(
        "__pycache__", "*.pyc", ".pytest_cache",
        "catalog.tsv",
        "f-scores.csv", "f-records.csv", "f-text.txt",
    )

    shutil.copytree(src, dst, ignore=ignore_patterns)


def documentation_tools() -> Path:
    configured = os.environ.get("AVL_BASIC_TOOLS_DIR")
    if not configured:
        raise SystemExit(
            "Release packaging requires local documentation tools. "
            "Set AVL_BASIC_TOOLS_DIR to the directory containing "
            "render_manuals.py and render_readme.py."
        )
    tools = Path(configured).expanduser().resolve()
    missing = [name for name in ("render_manuals.py", "render_readme.py")
               if not (tools / name).is_file()]
    if missing:
        raise SystemExit(
            f"Missing documentation tools in AVL_BASIC_TOOLS_DIR: {', '.join(missing)}"
        )
    return tools


def render_documentation(script: str, destination: Path) -> None:
    tools = documentation_tools()
    environment = os.environ.copy()
    environment["AVL_BASIC_REPO"] = str(ROOT)
    subprocess.run(
        [sys.executable, str(tools / script),
         "--output", str(destination)],
        cwd=ROOT,
        env=environment,
        check=True,
    )


def build_html_manuals(destination: Path) -> None:
    render_documentation("render_manuals.py", destination)


def build_html_readme(destination: Path) -> None:
    render_documentation("render_readme.py", destination)


def write_first_readme(dst: Path, version: str) -> None:
    dst.write_text(
        f"""AVL BASIC {version} for Windows

Quick start
-----------

Double-click avl-basic.exe, or open a terminal in this folder and run:

    avl-basic.exe

Run a bundled example:

    avl-basic.exe samples\\g-old-school.bas

Inside AVL BASIC:

    HELP RIGHT$

HELP topic gives a compact syntax and parameter reminder. The interpreter itself
is fully self-contained. The samples directory contains optional material
distributed with this package for exploration; its visual gallery and annotated
catalog are in samples\\README.md.

Open README.html in your browser for the project overview and image gallery.
Its images are embedded, so the document works offline.

Open MANUAL.html in your browser for the English manual or MANUAL.es.html
for Spanish. Both manuals are beside the executable and work offline.
To try a complete example, enter NEW and CD "/", paste it, then enter RUN.
Start the interpreter in this package folder so samples/assets paths resolve.

This package uses the native Rust runtime. You do not need to install Rust or
Cargo to use it.

Included files
--------------

- avl-basic.exe: native Windows interpreter
- README.html: offline project overview with embedded images
- samples/: optional collection of BASIC programs, gallery, catalog, and assets
- MANUAL.html: offline English manual with navigation, search and copyable examples
- MANUAL.es.html: offline Spanish manual with the same features
- COPYING: MIT project license
- LICENSES: third-party licenses and copyright notices
""",
        encoding="utf-8",
        newline="\r\n",
    )


def build_package(skip_build: bool) -> Path:
    documentation_tools()
    version = language_version()
    package_name = f"avl-basic-{version}-windows-x64"
    stage = RELEASE_DIR / package_name
    zip_path = RELEASE_DIR / f"{package_name}.zip"
    exe = ROOT / "target" / "release" / "avl-basic.exe"

    if not skip_build:
        run(["cargo", "build", "--release", "--locked"], ROOT)
    if not exe.exists():
        raise SystemExit(f"Missing release executable: {exe}")

    if stage.exists():
        shutil.rmtree(stage)
    if zip_path.exists():
        zip_path.unlink()
    stage.mkdir(parents=True)

    shutil.copy2(exe, stage / "avl-basic.exe")
    write_first_readme(stage / "README-FIRST.txt", version)

    for name in ["COPYING", "LICENSES"]:
        shutil.copy2(ROOT / name, stage / name)

    copy_tree(ROOT / "samples", stage / "samples")
    build_html_manuals(stage)
    build_html_readme(stage)
    shutil.make_archive(str(zip_path.with_suffix("")), "zip", RELEASE_DIR, package_name)
    return zip_path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Reuse target/release/avl-basic.exe instead of running cargo build.",
    )
    args = parser.parse_args()

    zip_path = build_package(skip_build=args.skip_build)
    print(zip_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

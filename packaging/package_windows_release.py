"""Build a Windows end-user ZIP for AVL BASIC.

The ZIP is intentionally for people who do not have Rust or Cargo installed. It
contains the native Rust executable plus the manuals and samples.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
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
    ignore = shutil.ignore_patterns("__pycache__", "*.pyc", ".pytest_cache")
    shutil.copytree(src, dst, ignore=ignore)


def write_launcher(dst: Path) -> None:
    dst.write_text(
        '@echo off\r\n'
        'cd /d "%~dp0"\r\n'
        '"%~dp0avl-basic.exe" %*\r\n',
        encoding="ascii",
    )


def write_first_readme(dst: Path, version: str) -> None:
    dst.write_text(
        f"""AVL BASIC {version} for Windows

Quick start
-----------

Double-click avl-basic.cmd, or open a terminal in this folder and run:

    avl-basic.cmd

Run a bundled example:

    avl-basic.cmd samples\\g-cube2.bas

Inside AVL BASIC:

    CD "samples"
    FILES "*.bas"
    RUN "g-cube2.bas"

This package uses the native Rust runtime. You do not need to install Rust or
Cargo to use it.

Included files
--------------

- avl-basic.exe: native Windows interpreter
- avl-basic.cmd: launcher that starts in this folder
- samples/: bundled BASIC programs and assets
- MANUAL.txt: English manual
- MANUAL.es.txt: Spanish manual
- COPYING: GPLv3-or-later license
""",
        encoding="utf-8",
        newline="\r\n",
    )


def build_package(skip_build: bool) -> Path:
    version = language_version()
    package_name = f"avl-basic-{version}-windows-x64"
    stage = RELEASE_DIR / package_name
    zip_path = RELEASE_DIR / f"{package_name}.zip"
    exe = ROOT / "target" / "release" / "avl-basic.exe"

    if not skip_build:
        run(["cargo", "build", "--release"], ROOT)
    if not exe.exists():
        raise SystemExit(f"Missing release executable: {exe}")

    if stage.exists():
        shutil.rmtree(stage)
    if zip_path.exists():
        zip_path.unlink()
    stage.mkdir(parents=True)

    shutil.copy2(exe, stage / "avl-basic.exe")
    write_launcher(stage / "avl-basic.cmd")
    write_first_readme(stage / "README-FIRST.txt", version)

    for name in [
        "README.md",
        "README.png",
        "MANUAL.txt",
        "MANUAL.es.txt",
        "COPYING",
    ]:
        shutil.copy2(ROOT / name, stage / name)

    copy_tree(ROOT / "samples", stage / "samples")
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

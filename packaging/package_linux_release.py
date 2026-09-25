"""Package a Linux x86-64 release with manuals, samples and desktop integration.

Run on Linux/WSL after copying the release binary to target/release/avl-basic,
or omit --skip-build to build it there. The archive preserves executable modes.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import tarfile
from pathlib import Path

from package_windows_release import (
    ROOT, RELEASE_DIR, build_html_manuals, build_html_readme, copy_tree, language_version,
)


def glibc_requirement(binary: Path) -> str:
    result = subprocess.run(
        ["readelf", "--version-info", str(binary)],
        check=True, capture_output=True, text=True,
    )
    versions = {tuple(map(int, v.split("."))) for v in re.findall(
        r"\bGLIBC_(\d+\.\d+(?:\.\d+)?)\b", result.stdout
    )}
    if not versions:
        raise SystemExit("Could not determine the executable's glibc requirement")
    return ".".join(map(str, max(versions)))


def build_package(skip_build: bool) -> Path:
    version = language_version()
    package_name = f"avl-basic-{version}-linux-x64"
    stage = RELEASE_DIR / package_name
    archive = RELEASE_DIR / f"{package_name}.tar.gz"
    binary = ROOT / "target" / "release" / "avl-basic"
    if not skip_build:
        subprocess.run(
            ["cargo", "build", "--release", "--locked", "--target-dir", str(ROOT / "target")],
            cwd=ROOT, check=True,
        )
    if not binary.is_file():
        raise SystemExit(f"Missing release executable: {binary}")
    header = binary.read_bytes()[:20]
    if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
        raise SystemExit("Expected a little-endian Linux x86-64 ELF executable")
    glibc = glibc_requirement(binary)
    # Only this version's generated staging directory may be replaced.
    if stage.resolve().parent != RELEASE_DIR.resolve() or stage.is_symlink():
        raise SystemExit(f"Unsafe staging directory: {stage}")
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    shutil.copy2(binary, stage / "avl-basic")
    (stage / "avl-basic").chmod(0o755)
    for name in ["COPYING", "LICENSES"]:
        shutil.copy2(ROOT / name, stage / name)
    for name in ["samples", "assets/linux"]:
        copy_tree(ROOT / name, stage / name)
    build_html_manuals(stage)
    build_html_readme(stage)
    (stage / "packaging" / "linux").mkdir(parents=True)
    for name in ["install_linux_desktop.sh", "linux/avl-basic.desktop.in"]:
        # A Windows checkout can have CRLF even when packaging through WSL.
        source = ROOT / "packaging" / name
        destination = stage / "packaging" / name
        destination.write_text(source.read_text(encoding="utf-8"), encoding="utf-8", newline="\n")
    (stage / "README-FIRST.txt").write_text(
        f"""AVL BASIC {version} for Linux x86-64

Quick start
-----------
Extract the archive, open a terminal in this folder, and run:

    ./avl-basic

Inside AVL BASIC:

    RUN "samples/s-cpc-duet.bas"
    RUN "samples/g-arkanoid.bas"

You can also launch an example directly:

    ./avl-basic samples/g-old-school.bas

Open README.html in your browser for the project overview and image gallery.
Its images are embedded, so the document works offline.

Open MANUAL.html in your browser for the English manual or MANUAL.es.html
for Spanish. Both manuals are beside the executable and work offline.
To try a complete example, enter NEW and CD "/", paste it, then enter RUN.
Start the interpreter in this package folder so samples/assets paths resolve.

Requirements
------------
This binary requires glibc {glibc} or later, libasound.so.2, libgcc_s.so.1,
and the standard system C/math libraries. Graphics require X11 or XWayland.
Audio uses the desktop's system audio service; WSLg is supported too.
No Rust toolchain, external player or codec pack is needed.
Older distributions can build from the corresponding source release.

Without an available audio output, BASIC can continue with silent timing.
The runtime libraries listed above must still be installed for the executable
to start. Enter PRINT AUDIOERROR$ to inspect an audio device error.

Optional desktop launcher (current user, no root access)
-----------------------------------------------------
From this folder:

    sh packaging/install_linux_desktop.sh "$PWD/avl-basic"

Included files
--------------
- avl-basic: native Linux interpreter
- README.html: offline project overview with embedded images
- samples/: BASIC programs, gallery, catalog, images and audio assets
- MANUAL.html: offline English manual with navigation, search and copyable examples
- MANUAL.es.html: offline Spanish manual with the same features
- COPYING: MIT project license
- LICENSES: third-party licenses and copyright notices
- packaging/ and assets/linux/: optional desktop launcher and icons
""", encoding="utf-8", newline="\n",
    )

    def archive_permissions(info: tarfile.TarInfo) -> tarfile.TarInfo:
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mode = 0o755 if info.isdir() or info.name == f"{package_name}/avl-basic" else 0o644
        return info

    with tarfile.open(archive, "w:gz") as target:
        target.add(stage, arcname=package_name, filter=archive_permissions)
    return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    print(build_package(args.skip_build))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

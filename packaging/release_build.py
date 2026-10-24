"""Bridge release packaging to the maintainer's external PGO builder."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


BINARY_NAMES = {
    "x86_64-pc-windows-msvc": "avl-basic.exe",
    "x86_64-unknown-linux-gnu": "avl-basic",
}


@dataclass(frozen=True)
class ReleaseBinary:
    path: Path
    sha256: str


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def prepare_release_binary(
    root: Path, target: str, version: str, skip_build: bool,
) -> ReleaseBinary:
    """Build when requested, then always require the external PGO verification."""
    root = root.resolve()
    if target not in BINARY_NAMES:
        raise SystemExit(f"Unsupported release target: {target}")
    configured = os.environ.get("AVL_BASIC_TOOLS_DIR")
    if not configured:
        raise SystemExit(
            "Release packaging requires AVL_BASIC_TOOLS_DIR with build_pgo_release.py."
        )
    builder = Path(configured).expanduser().resolve() / "build_pgo_release.py"
    if not builder.is_file():
        raise SystemExit(f"Missing PGO release builder: {builder}")
    binary = root / "target" / "release" / BINARY_NAMES[target]
    environment = os.environ.copy()
    environment["AVL_BASIC_REPO"] = str(root)
    command = [sys.executable, str(builder), "--repo", str(root), "--target", target]
    try:
        if not skip_build:
            subprocess.run(
                [*command, "--install-bin", str(binary)],
                cwd=root, env=environment, check=True,
            )
        verification = subprocess.run(
            [*command, "--verify-only"],
            cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8", check=False,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"PGO release build or verification could not complete: {error}") from error
    if verification.stderr:
        sys.stderr.write(verification.stderr)
    if verification.returncode:
        raise SystemExit(
            "PGO release verification failed; rebuild with the maintainer PGO builder. "
            "--skip-build requires a valid local PGO receipt."
        )
    try:
        record = json.loads(verification.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit("The PGO verifier did not return its JSON result.") from error
    if not isinstance(record, dict) or record.get("status") != "PASS":
        raise SystemExit("The PGO verifier did not confirm a valid release binary.")
    if record.get("version") != version or record.get("target") != target:
        raise SystemExit("The verified PGO version or target does not match this package.")
    verified_path = record.get("binary_path")
    if not isinstance(verified_path, str) or Path(verified_path).resolve() != binary.resolve():
        raise SystemExit("The PGO verifier identified a different release binary.")
    digest = record.get("binary_sha256")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", digest):
        raise SystemExit("The PGO verifier returned an invalid binary SHA256.")
    artifact = ReleaseBinary(binary, digest.lower())
    if not binary.is_file() or file_sha256(binary) != artifact.sha256:
        raise SystemExit("The release executable changed after PGO verification.")
    return artifact


def copy_release_binary(artifact: ReleaseBinary, destination: Path) -> None:
    """Copy only the verified bytes; refuse a concurrent executable replacement."""
    shutil.copy2(artifact.path, destination)
    if (file_sha256(destination) != artifact.sha256
            or file_sha256(artifact.path) != artifact.sha256):
        raise SystemExit("The release executable changed while copying; package creation stopped.")

# Maintainer tools

This directory contains the tools used to maintain published documentation,
regenerate sample assets, and validate the interpreter. Research reports,
benchmark results, profilers, and experimental scripts are kept outside the
repository.

- **Documentation:** `render_manuals.py`, `render_readme.py`,
  `render_showcase_docs.py`, `generate_showcase.py`, and `sync_language_docs.py`.
  See [manual generation](manuals/README.md) and [the release README](README-html.md).
- **Validation:** `check_manual_examples.py`, `check_python_error_catalog.py`,
  the `run_*_parity.py` scripts, and the console/editor/debugger checks.
  Python parity tests use the external reference checkout specified by
  `AVL_BASIC_PY_REPO`; each script documents its arguments.
- **Sample assets:** `generate_arkanoid_audio.py` and
  `generate_dungeon_locked_audio.py` regenerate the corresponding bundled WAVs.

The release packages contain the generated documentation and assets, rather
than these maintenance tools. Building and running the interpreter requires
neither this directory nor Python. README rendering uses the dependencies in
`requirements-docs.txt`; manual generation uses Cargo, and gallery captures
require Pillow.

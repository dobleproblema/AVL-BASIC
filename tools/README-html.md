# Standalone release README

The repository's `README.md`, `README.png` and `README-console.png` remain the
GitHub sources. Both release packagers generate a single `README.html` instead
of copying those three files into the package root. The sample tree is unchanged.

With Python 3.10 or later, install the pinned maintainer dependency once, then run:

```sh
python -m pip install -r tools/requirements-docs.txt
python tools/render_readme.py
python tools/render_readme.py --check
```

On systems that require a virtual environment, install into a venv and run the
renderer and packagers with that environment's Python. No Markdown library or
Python runtime is included in, or needed to read, the generated document.

The renderer uses Python-Markdown, preserves the raw HTML gallery, embeds every
image as a data URL, and includes its responsive light/dark CSS. It writes only
`dist/README.html`; `--output DIRECTORY` selects another destination. `--check`
checks freshness without writing. The source Markdown and images are never edited.

The document itself can be read offline even when copied to an empty directory.
Links to the manuals, sample programs and licenses refer to companion files in
the release package. Source-only links and Markdown galleries point to GitHub,
preserving their fragments. These links require a connection only when followed.
For a complete local preview, open the generated README in a staged package.

Before packaging, install the same pinned dependency for the Python interpreter
that runs `packaging/package_windows_release.py` or `packaging/package_linux_release.py`.
Both scripts regenerate the HTML from the current sources automatically.

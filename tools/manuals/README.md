# HTML manuals

`MANUAL.txt` and `MANUAL.es.txt` are the maintained sources. From the repository:

```sh
python tools/render_manuals.py
python tools/render_manuals.py --check
python tools/sync_language_docs.py --check
python tools/check_manual_examples.py
```

The renderer writes only `MANUAL.html` (English) and `MANUAL.es.html` (Spanish)
to `dist/` by default. `--output PATH` selects another directory. Each HTML file
embeds its CSS, JavaScript and native syntax styles;
reading it needs neither a server nor an Internet connection. The Windows and
Linux packaging scripts place both files beside the executable in each package.
The TXT sources stay in the repository and are not copied into end-user packages.
Open `MANUAL.html` for English or `MANUAL.es.html` for Spanish. Each manual
provides a language switch that works without JavaScript.

Use `--audit-dir PATH` to write developer JSON audit files to a separate directory
outside the HTML output. These files are not part of end-user packages.

Use explicit fences in the text sources:

- `basic`: a complete, independently runnable numbered program, copied unchanged.
- `console`: commands entered in AVL BASIC immediate mode, without line numbers.
- `text`: plain output, shell commands or a fragment that is not an independent program.
- `output trace`: console output with native TRON styling on each complete `[line]`
  marker; ordinary printed values remain plain.
- `output error=2`: console output with native error styling on the second line.
  Use one-based, comma-separated line indexes for other layouts. Unmarked lines
  remain plain, even if their printed text happens to resemble an error message.

Keep runnable programs identical in both languages; translate their surrounding
explanation. Asset paths start with `samples/assets/`, so readers start the
interpreter in the package root and save these programs there. Include the full
setup and every jump destination. The error-handling example intentionally
demonstrates a nonexistent destination and documents its expected error.

Reference rows start with `+ `. Separate syntax from its description with at least
two spaces, or put the description on an indented continuation line. Wrapped
descriptions stay in the same table cell. Every row in a reference table must have
a description; the renderer rejects incomplete rows. A group containing only
syntax (for example, a list of function names) must use single spaces internally
and renders as syntax blocks instead of an empty table.

The renderer builds `examples/manual_highlight.rs` against the current interpreter
and translates its dark console ANSI styles into HTML, including the actual
`trace_text` and `error_text` renderers for annotated output. It checks text preservation,
source-line coverage and local links. It never executes the examples. `--check`
regenerates in a temporary directory and compares bytes with the selected output.

`check_manual_examples.py` separately runs every complete program in an isolated
directory with the bundled assets. It checks bilingual equivalence, literal jumps,
expected numeric/text/file output and important alternative branches. Graphics
use the interpreter's real pixel buffer with native windows and audio disabled.
Its report explicitly identifies bounded loops and synthetic input used for
interactive programs. Native-window timing, keyboard/mouse interaction and audible
playback still require an interactive check when those examples change.

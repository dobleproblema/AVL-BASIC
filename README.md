# AVL BASIC

<p align="center">
  <img src="README.png" alt="AVL BASIC screenshot" width="900">
</p>

AVL BASIC is a classic-style BASIC interpreter written in pure Python.

It is designed to be easy to copy, easy to run, and easy to learn:

- Single-file interpreter: the core is just `basic.py`.
- No package installation: you only need Python 3.8+ and Tkinter.
- Classic feel: line numbers, immediate mode with syntax highlighting, `ON ERROR`, `MAT`, and more.
- Built-in graphics: plotting, shapes, colors, sprites, screenshots, and mouse support.
- Portable behavior: same interpreter on Windows, macOS, and Linux.
- Plenty of interesting bundled examples in [`samples/`](samples/).

AVL BASIC aims to feel like a powerful but still reasonably standard old-school home-computer BASIC, implemented in a pragmatic modern way.


## Why It Is Interesting

AVL BASIC is not a package-heavy retro project. It is a self-contained interpreter that tries to preserve the immediacy of classic BASIC systems:

- start it and type code straight away,
- save programs as plain `.bas` files,
- use graphics without external libraries beyond Tkinter,
- distribute the interpreter as a single Python file.

That makes it useful both as a nostalgic environment and as a compact educational interpreter.

## Quick Start

Run the interpreter:

```bash
python basic.py
```

Load and run a program directly:

```bash
python basic.py samples/g-cube2.bas
```

Or browse the bundled samples from immediate mode:

```basic
CD "samples"
FILES "*.bas"
RUN "g-cube2.bas"
CD "/"
```

## Documentation

- Full manual in English: [`MANUAL.txt`](MANUAL.txt)
- Manual completo en español: [`MANUAL.es.txt`](MANUAL.es.txt)
- License: [`COPYING`](COPYING)

## Requirements

- Python 3.8 or later
- Tkinter / Tk 8.6 or later

Tkinter is included in the official Python distributions for Windows and macOS. On many Linux systems, you may need to install the `python3-tk` package separately.

## Project Layout

- [`basic.py`](basic.py): interpreter
- [`basicfonts.py`](basicfonts.py): font-editing helper used to regenerate embedded bitmap fonts
- [`samples/`](samples/): example BASIC programs
- [`samples/assets/`](samples/assets/): image assets used by some samples

## License

AVL BASIC is free software released under the GNU GPL, version 3 or later.

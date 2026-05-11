# AVL BASIC

<p align="center">
  <img src="README.png" alt="AVL BASIC screenshot" width="900">
</p>

AVL BASIC is a classic-style BASIC system with line-numbered programs,
immediate mode, structured control flow, matrix operations, sprites, mouse and
keyboard input, and a complete built-in graphics environment.

The project now ships two implementations of the same language:

| Implementation | Location | Role | Quick run |
| --- | --- | --- | --- |
| Rust | [`rust/`](rust/) | Recommended native runtime. It is much faster and suitable for daily use and distribution. | `avl-basic.exe` |
| Python | [`basic.py`](basic.py) | Reference implementation, compact educational interpreter, and compatibility oracle. | `python basic.py` |

Both implementations share the same manual, samples, language version, and
compatibility target. The Rust runtime is tested against the Python interpreter,
including text regressions and framebuffer-level graphics parity cases.

AVL BASIC is a cross-platform project in both forms. The Python implementation
runs on Windows, macOS, and Linux with Tkinter. The Rust implementation builds
and runs natively on Windows and Linux, and is intended to remain portable to
macOS as well. Prebuilt downloads may be added per platform; the first packaged
binary is Windows x64.

## Download

For Windows users, the easiest option is the prebuilt native package:

1. Download the latest `avl-basic-*-windows-x64.zip` from
   [GitHub Releases](https://github.com/dobleproblema/AVL-BASIC/releases/latest).
2. Extract the ZIP.
3. Run `avl-basic.exe`.

The Windows package includes the native interpreter, manuals, examples, assets,
license, and the Python reference implementation. You do not need Rust, Cargo,
Python, or Tkinter to use the native interpreter.

On Linux and macOS, use the Python implementation directly or build the Rust
implementation from source until prebuilt packages are published for those
platforms.

## Quick Start

With the Windows package:

```bat
avl-basic.exe
avl-basic.exe samples\g-cube2.bas
```

From BASIC immediate mode:

```basic
CD "samples"
FILES "*.bas"
RUN "g-cube2.bas"
CD "/"
```

## Build From Source

Rust is only required if you want to build the native interpreter yourself.

Run the Rust interpreter:

```bash
cd rust
cargo run --release
```

Run a bundled sample with Rust:

```bash
cd rust
cargo run --release -- ../samples/g-cube2.bas
```

Run the Python reference implementation:

```bash
python basic.py
python basic.py samples/g-cube2.bas
```

## Why It Is Interesting

AVL BASIC aims to preserve the immediacy of classic home-computer BASIC while
adding a practical modern feature set:

- plain `.bas` files and an interactive immediate mode,
- syntax-preserving program editing and listing,
- `ON ERROR`, `ON TIMER`, `ON MOUSE`, procedures, functions, and matrices,
- graphics commands for plotting, shapes, axes, sprites, screenshots, and input,
- deterministic examples and regressions used to keep both runtimes aligned.

The Python interpreter keeps the implementation easy to study. The Rust
interpreter keeps the same behavior but runs native and is typically much
faster, especially for graphics-heavy programs.

## Documentation

- Full manual in English: [`MANUAL.txt`](MANUAL.txt)
- Manual completo en español: [`MANUAL.es.txt`](MANUAL.es.txt)
- Rust implementation notes: [`rust/README.md`](rust/README.md)
- License: [`COPYING`](COPYING)

## Requirements

Prebuilt Windows package:

- Windows x64

Python implementation:

- Windows, macOS, or Linux
- Python 3.8 or later
- Tkinter / Tk 8.6 or later

Building the Rust implementation:

- Windows, Linux, or macOS
- Rust stable toolchain
- Native desktop environment for the graphics window

Tkinter is included in the official Python distributions for Windows and macOS.
On many Linux systems, install the `python3-tk` package separately.

## Project Layout

- [`basic.py`](basic.py): Python reference interpreter
- [`rust/`](rust/): Rust native interpreter and parity tools
- [`samples/`](samples/): shared BASIC example programs
- [`samples/assets/`](samples/assets/): shared image assets used by examples
- [`tests/`](tests/): Python regression tests
- [`MANUAL.txt`](MANUAL.txt), [`MANUAL.es.txt`](MANUAL.es.txt): shared language manuals

## Release Packaging

Maintainers can build the Windows end-user ZIP with:

```bash
python packaging/package_windows_release.py
```

The generated `release/` directory is local build output. Published binaries
belong in GitHub Releases, not in the Git repository.

## License

AVL BASIC is free software released under the GNU GPL, version 3 or later.

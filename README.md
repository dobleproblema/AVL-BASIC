# AVL BASIC

<p align="center">
  <img src="README.png" alt="AVL BASIC Rust runtime screenshot" width="900">
</p>

AVL BASIC is a native Rust implementation of a classic-style BASIC system with
line-numbered programs, immediate mode, structured control flow, matrix
operations, sprites, mouse and keyboard input, and a complete built-in graphics
environment.

The project is built around the Rust runtime: a fast native executable for
daily use, packaged distribution, and source builds.

## Download

For Windows users, the easiest option is the prebuilt native package:

1. Download the latest `avl-basic-*-windows-x64.zip` from
   [GitHub Releases](https://github.com/dobleproblema/AVL-BASIC/releases/latest).
2. Extract the ZIP.
3. Run `avl-basic.cmd`.

The Windows package includes the native interpreter, manuals, examples, assets,
and license. You do not need Rust or Cargo to use it.

Linux and macOS users can build from source until prebuilt packages are
published for those platforms.

## Quick Start

With the Windows package:

```bat
avl-basic.cmd
avl-basic.cmd samples\g-cube2.bas
```

From BASIC immediate mode:

```basic
CD "samples"
FILES "*.bas"
RUN "g-cube2.bas"
```

## Build From Source

Requirements:

- Rust stable toolchain
- A native desktop environment for the graphics window

Build the release interpreter:

```bash
cargo build --release
```

Run interactive mode:

```bash
cargo run --release
```

Run a bundled sample:

```bash
cargo run --release -- samples/g-cube2.bas
```

The compiled executable is created at:

- Windows: `target/release/avl-basic.exe`
- Linux/macOS: `target/release/avl-basic`

The build also exposes the bundled examples as `target/release/samples`, so
`CD "samples"` works even when you launch the compiled executable directly from
`target/release`.

## Why It Is Interesting

AVL BASIC aims to preserve the immediacy of classic home-computer BASIC while
adding a practical modern feature set:

- plain `.bas` files and an interactive immediate mode,
- syntax-preserving program editing and listing,
- `ON ERROR`, `ON TIMER`, `ON MOUSE`, procedures, functions, and matrices,
- graphics commands for plotting, shapes, axes, sprites, screenshots, and input,
- embedded bitmap fonts for reproducible graphics text,
- deterministic examples and regression tests for the native runtime.

The interpreter is console-first and line-numbered by design. It is meant to
feel direct and teachable rather than like an IDE-centered dialect.

## Embedded Fonts

AVL BASIC embeds its own small and large bitmap fonts in the Rust binary. The
editable source is [`assets/fonts/avl-basic-fonts.txt`](assets/fonts/avl-basic-fonts.txt).
`build.rs` validates that source and generates the Rust glyph tables during the
build.

## Documentation

- Full manual in English: [`MANUAL.txt`](MANUAL.txt)
- Manual completo en español: [`MANUAL.es.txt`](MANUAL.es.txt)
- License: [`COPYING`](COPYING)

## Project Layout

- [`src/`](src/): interpreter, parser helpers, graphics, console, and window backend
- [`tests/`](tests/): Rust unit and integration tests
- [`tools/`](tools/): maintainer validation and benchmark tools
- [`samples/`](samples/): BASIC example programs
- [`samples/assets/`](samples/assets/): image assets used by examples
- [`assets/fonts/`](assets/fonts/): editable embedded bitmap font source
- [`packaging/`](packaging/): release packaging scripts

## Release Packaging

The generated `release/` directory is local build output. Published binaries
belong in GitHub Releases, not in the Git repository.

## License

AVL BASIC is free software released under the GNU GPL, version 3 or later.

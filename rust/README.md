# AVL BASIC Rust Runtime

This directory contains the Rust implementation of AVL BASIC. It is intended to
be functionally compatible with the Python reference interpreter in the parent
directory, while providing much higher execution speed and a native executable.

## Build

```bash
cargo build --release
```

The executable is created at:

- Windows: `target/release/avl-basic.exe`
- Linux/macOS: `target/release/avl-basic`

## Run

Interactive mode:

```bash
cargo run --release
```

When started from this directory, `CD "/"` in BASIC immediate mode moves to the
shared repository root, where `samples/` and the manuals live.

Run a shared sample from the monorepo:

```bash
cargo run --release -- ../samples/g-cube2.bas
```

Run the compiled executable directly:

```bash
target/release/avl-basic ../samples/g-cube2.bas
```

Use `target\release\avl-basic.exe` on Windows.

## Compatibility Checks

Run the Rust test suite:

```bash
cargo test
```

Build the release binary and compare against the Python oracle:

```bash
cargo build --release
python tools/run_python_text_parity.py --mode all-text --rust-bin target/release/avl-basic
python tools/run_python_direct_regression_parity.py --mode all-text --rust-bin target/release/avl-basic
python tools/run_python_graphics_parity.py --mode smoke --rust-bin target/release/avl-basic
```

Use `target\release\avl-basic.exe` for `--rust-bin` on Windows.

The parity tools default to the monorepo layout, where the Python
implementation is the parent directory. Set `AVL_BASIC_PY_REPO` or
`AVL_BASIC_RUST_BIN` to override those paths.

## Useful Environment Variables

- `AVL_BASIC_WINDOW=0`: suppress the native graphics window during automated runs.
- `AVL_BASIC_COLOR=0`: disable console colors.
- `PYTHON=python3`: choose the Python executable used by parity tests.

## Layout

- [`src/`](src/): interpreter, parser helpers, graphics, console, and window backend
- [`tests/`](tests/): Rust unit and parity tests
- [`tools/`](tools/): Python oracle comparison and benchmark scripts
- [`Cargo.toml`](Cargo.toml): Rust package definition

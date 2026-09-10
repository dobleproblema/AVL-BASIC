# Shared CHAIN/MERGE regression corpus

The `.case` files specify the common Python and Rust contract for `CHAIN`,
`CHAIN MERGE` and `MERGE`. Keep the fixtures byte-for-byte identical in both
repositories. Expected output and file contents are written independently of
either implementation; neither runtime acts as an optional oracle for the other.
Both harnesses discover and run every `.case` file in this directory.

## Running the corpus

From the Python repository root:

```sh
python -m pytest tests/test_chain_merge.py -q
```

From the Rust repository root:

```sh
cargo test --test chain_merge_corpus -- --nocapture
```

The Python harness starts `basic.py` with the current Python executable. The
Rust harness starts Cargo's built `avl-basic` executable. Both send commands to
the real interactive CLI through standard input, create a separate temporary
working directory for each case, and enforce a 15-second timeout per session.
They use an eight-column print zone and disable the separate console window.
The Rust corpus does not require Python or an environment variable to enable it.

## Case format

Save cases as UTF-8 with LF line endings. Section headers occupy a complete
line, start in column one, and must be unique within a case. The following
sections are supported:

| Section | Contents |
| --- | --- |
| `[commands]` | Required. The complete BASIC prompt session, including numbered source lines and commands such as `RUN`, `LOAD`, `CHAIN` or `CONT`. |
| `[output]` | Required. Expected console output, including expected error messages, without the startup banner or `Ready` prompts. |
| `[input relative/path]` | Optional. Exact UTF-8 text written to a file before the session starts. Parent directories are created automatically. |
| `[input-hex relative/path]` | Optional. Hexadecimal bytes written before the session; whitespace between bytes is ignored. Use this for invalid UTF-8 or other exact byte sequences. |
| `[expect relative/path]` | Optional. Exact UTF-8 bytes expected in an existing file after the session ends. |

Fixture file paths are relative to the case's temporary directory. Use ordinary
path components separated by `/`, without absolute paths or `..`. A BASIC
program inside a fixture may still exercise virtual absolute paths or parent
directory traversal according to the language's path rules.

Every body line belongs to its section, including blank lines and the newline
before the next header. Do not add blank separator lines unless they are part of
the intended input, output or file contents. In particular, `[expect ...]`
compares file bytes without newline or whitespace normalization. A section
header cannot appear as a literal complete body line with this format.

For example:

```text
[commands]
10 OPEN "result.txt" FOR OUTPUT AS #1
20 PRINT #1,"left-";
30 CHAIN "next"
RUN
[output]
OK
[input next.bas]
10 PRINT #1,"right"
20 PRINT "OK"
30 END
[expect result.txt]
left-right
```

The expected file in this example ends with one LF. The example checks that an
open output channel and a partial line survive `CHAIN`.

The harness adds begin/end markers around the session, then sends `NEW` and
`EXIT` to finish it. Do not put `EXIT`, `QUIT` or `SYSTEM` in a fixture or print
the reserved markers `__CHAIN_TEST_BEGIN__` and `__CHAIN_TEST_END__`. Avoid an
expected output line consisting only of `Ready`, because it is filtered as a
prompt. The harness normalizes actual console output by removing terminal
line-ending differences, trailing whitespace on each line and outer whitespace.
Keep expected lines free of trailing whitespace; internal spacing and blank
lines remain significant. Use `[expect ...]` when exact whitespace is part of the
contract. To check when a channel closes, include an observable operation in
`[commands]`; a file check alone occurs after the harness's final `NEW`.

## Coverage

The named families cover:

- `prompt-`: immediate `CHAIN` and `CHAIN MERGE`, with and without a stopped
  program.
- `flow-`: transfers from nested control flow, inline statement tails, repeated
  chaining, and control state retained by `MERGE`.
- `state-`: variables and arrays retained across program transfers.
- `routines-`: routine replacement, source-side argument expressions, duplicate
  definitions, repairs, deleted routine boundaries and forbidden contexts.
- `errors-`: missing files, invalid UTF-8, invalid physical source lines,
  preserved pending continuations, and missing or protected entry lines.
- `files-`: input positions and EOF, partial OUTPUT/APPEND writes, final file
  bytes, and channel closure at `END` or normal program completion.
- `data-`: replacement and rebuilding of DATA, pointer resets, RESTORE,
  overwritten DATA lines, and DELETE ordering.
- `paths-`: relative and virtual absolute paths, case matching, omitted program
  extensions, current-directory selection and program-directory lifetime.

These are regression examples, not an exhaustive proof of every possible
program. They do not replace the full interpreter suites, GUI/debugger tests,
real timer and mouse-event tests, or host-specific filesystem tests. A failed
case must be assessed against the documented contract; do not regenerate its
expectations from whichever implementation currently passes.

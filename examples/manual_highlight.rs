//! Export the actual console's dark syntax styles for the HTML manual builder.
//! Input and output are one UTF-8 line per record; no BASIC code is executed.
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "--code".into());
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        let styled = match mode.as_str() {
            "--code" => avl_basic::console::syntax_highlight_raw_with_cases(&line, true, None),
            "--error" => avl_basic::console::error_text(true, &line),
            "--trace" => {
                let number = line
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .and_then(|s| s.parse::<i32>().ok())
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "expected [line]")
                    })?;
                avl_basic::console::trace_text(true, number)
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown style mode",
                ))
            }
        };
        writeln!(stdout, "{styled}")?;
    }
    Ok(())
}

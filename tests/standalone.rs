use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn executable_help_is_complete_in_an_empty_directory() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .current_dir(directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"HELP RIGHT$\nPRINT VERSION$\nSYSTEM\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Type HELP <topic> for syntax and parameters."));
    assert!(stdout.contains("RIGHT$(text$, length)"));
    assert!(stdout.contains("Returns the rightmost characters;"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
    assert_eq!(directory.path().read_dir().unwrap().count(), 0);
}

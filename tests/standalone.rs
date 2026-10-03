use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn immediate_exit_commands_ignore_surrounding_whitespace() {
    for command in ["EXIT", "QUIT", "SYSTEM"] {
        for (leading, trailing) in [
            ("", ""),
            ("  ", ""),
            ("", "  "),
            ("  ", "  "),
            ("\t ", " \t"),
        ] {
            for spelling in [command.to_string(), command.to_ascii_lowercase()] {
                let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let source = format!(
                    "PRINT \"BEFORE_EXIT\"\n{leading}{spelling}{trailing}\nPRINT \"AFTER_EXIT\"\nSYSTEM\n"
                );
                child
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write_all(source.as_bytes())
                    .unwrap();
                let output = child.wait_with_output().unwrap();
                let stdout = String::from_utf8(output.stdout).unwrap();
                assert!(output.status.success(), "{source:?}: {:?}", output.stderr);
                assert!(stdout.contains("BEFORE_EXIT"), "{source:?}: {stdout}");
                assert!(!stdout.contains("AFTER_EXIT"), "{source:?}: {stdout}");
                assert_eq!(stdout.matches("Ready").count(), 2, "{source:?}: {stdout}");
            }
        }
    }
}

#[test]
fn immediate_exit_words_inside_other_commands_do_not_exit() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(
        b"  PRINT \" SYSTEM QUIT EXIT \"  \n  REM QUIT  \n  SYSTEMATIC=7  \n  PRINT SYSTEMATIC  \nPRINT \"STILL_RUNNING\"\nSYSTEM\n",
    ).unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(stdout.contains(" SYSTEM QUIT EXIT "), "{stdout}");
    assert!(stdout.contains("STILL_RUNNING"), "{stdout}");
}

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

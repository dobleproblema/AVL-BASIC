use avl_basic::{ErrorCode, Interpreter};
use std::fs;
use tempfile::{tempdir, TempDir};

fn setup() -> (TempDir, Interpreter) {
    let directory = tempdir().unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    (directory, interpreter)
}
fn command(interpreter: &mut Interpreter, source: &str) {
    interpreter
        .process_immediate(source)
        .unwrap_or_else(|error| panic!("{source}: {error}"));
}
fn error(interpreter: &mut Interpreter, source: &str, expected: ErrorCode) {
    assert_eq!(
        interpreter.process_immediate(source).unwrap_err().code,
        expected,
        "{source}"
    );
}

#[test]
fn csv_unicode_quotes_multiline_empty_and_numeric_round_trip() {
    let (directory, mut i) = setup();
    command(&mut i, "OPEN \"data.csv\" FOR OUTPUT AS #1");
    command(
        &mut i,
        "S$=\"niño,東京 \"+CHR$(34)+\"quoted\"+CHR$(34)+CHR$(13)+CHR$(10)+\"second\"",
    );
    command(&mut i, "WRITE #1,S$,0.1,-0,1E20,0.00001");
    command(&mut i, "WRITE #1");
    command(&mut i, "CLOSE #1");
    assert_eq!(fs::read_to_string(directory.path().join("data.csv")).unwrap(), "\"niño,東京 \"\"quoted\"\"\r\nsecond\",0.10000000000000001,0,1e+20,1.0000000000000001e-05\n\n");
    command(&mut i, "OPEN \"data.csv\" FOR INPUT AS #1");
    command(&mut i, "DIM A(5)");
    command(&mut i, "INPUT #1,T$,A(1),A(2),A(3),A(4)");
    command(
        &mut i,
        "PRINT T$=S$;A(1)=0.1;A(2)=0;A(3)=1E20;A(4)=0.00001;EOF(1)",
    );
    assert_eq!(i.take_output(), "-1 -1 -1 -1 -1  0\n");
    command(&mut i, "INPUT #1,E$");
    command(&mut i, "PRINT LEN(E$);EOF(1);EOF(1)");
    assert_eq!(i.take_output(), " 0 -1 -1\n");
    error(&mut i, "INPUT #1,E$", ErrorCode::InputPastEnd);
}

#[test]
fn physical_lines_bom_terminators_final_line_and_ctrl_z() {
    let (directory, mut i) = setup();
    fs::write(
        directory.path().join("lines"),
        "\u{feff}first\r\nsecond\rthird\n\u{1a}last",
    )
    .unwrap();
    command(&mut i, "OPEN \"lines\" FOR INPUT AS #255");
    for expected in ["first", "second", "third", "\u{1a}last"] {
        command(&mut i, "PRINT EOF(255)");
        assert_eq!(i.take_output(), " 0\n");
        command(&mut i, "LINE INPUT #255,S$");
        command(&mut i, "PRINT S$");
        assert_eq!(i.take_output(), format!("{expected}\n"));
    }
    command(&mut i, "PRINT EOF(255)");
    assert_eq!(i.take_output(), "-1\n");
    error(&mut i, "LINE INPUT #255,S$", ErrorCode::InputPastEnd);
    fs::write(directory.path().join("empty"), [0xef, 0xbb, 0xbf]).unwrap();
    command(&mut i, "OPEN \"empty\" FOR INPUT AS #1");
    command(&mut i, "PRINT EOF(1)");
    assert_eq!(i.take_output(), "-1\n");
}

#[test]
fn invalid_csv_and_numeric_data_are_atomic_for_values() {
    let (directory, mut i) = setup();
    for (index, data, expected) in [
        (0, "1,2,3\n", ErrorCode::VarNumberMismatch),
        (1, "1,2+3\n", ErrorCode::InvalidFileData),
        (2, "1,NaN\n", ErrorCode::InvalidFileData),
        (3, "1,\" 2 \"\n", ErrorCode::InvalidFileData),
        (4, "1,\"x\"bad\n", ErrorCode::InvalidFileData),
        (5, "1,\"x\" \"y\"\n", ErrorCode::InvalidFileData),
        (6, "1,\"unterminated", ErrorCode::InvalidFileData),
        (7, "1,2\"3\n", ErrorCode::InvalidFileData),
    ] {
        fs::write(directory.path().join(format!("bad{index}")), data).unwrap();
        command(&mut i, &format!("OPEN \"bad{index}\" FOR INPUT AS #1"));
        command(&mut i, "A=99:B=98");
        error(&mut i, "INPUT #1,A,B", expected);
        command(&mut i, "PRINT A;B");
        assert_eq!(i.take_output(), " 99  98\n");
        command(&mut i, "CLOSE #1");
    }
    fs::write(directory.path().join("utf8"), [255, 10]).unwrap();
    command(&mut i, "OPEN \"utf8\" FOR INPUT AS #1");
    error(&mut i, "LINE INPUT #1,S$", ErrorCode::InvalidFileData);
}

#[test]
fn opening_conflicts_validation_and_close_list_do_not_destroy_files() {
    let (directory, mut i) = setup();
    fs::write(directory.path().join("keep"), "original").unwrap();
    command(&mut i, "OPEN \"keep\" FOR INPUT AS #1");
    command(&mut i, "OPEN \"keep\" FOR INPUT AS #2");
    error(
        &mut i,
        "OPEN \"keep\" FOR OUTPUT AS #3",
        ErrorCode::FileAlreadyOpen,
    );
    error(
        &mut i,
        "OPEN \"other\" FOR OUTPUT AS #1",
        ErrorCode::FileAlreadyOpen,
    );
    error(&mut i, "CLOSE #1,#99", ErrorCode::FileNotOpen);
    command(&mut i, "PRINT EOF(1)");
    assert_eq!(i.take_output(), " 0\n");
    assert_eq!(
        fs::read_to_string(directory.path().join("keep")).unwrap(),
        "original"
    );
    assert!(!directory.path().join("other").exists());
    command(&mut i, "CLOSE #1,#1,#2");
    command(&mut i, "CLOSE:CLOSE");
    for expr in ["0", "256", "1.5", "INF", "-1"] {
        error(
            &mut i,
            &format!("OPEN \"keep\" FOR OUTPUT AS #{expr}"),
            ErrorCode::InvalidArgument,
        );
    }
    error(
        &mut i,
        "OPEN \"missing\" FOR INPUT AS #1",
        ErrorCode::FileNotFound,
    );
    error(
        &mut i,
        "OPEN \"../escape\" FOR OUTPUT AS #1",
        ErrorCode::InvalidArgument,
    );
    error(
        &mut i,
        "OPEN \"\" FOR OUTPUT AS #1",
        ErrorCode::InvalidArgument,
    );
    error(
        &mut i,
        "OPEN CHR$(0) FOR OUTPUT AS #1",
        ErrorCode::InvalidArgument,
    );
    error(
        &mut i,
        "OPEN \"/\" FOR OUTPUT AS #1",
        ErrorCode::InvalidArgument,
    );
    command(&mut i, "OPEN \"keep\" FOR OUTPUT AS #1");
    error(&mut i, "PRINT EOF(1)", ErrorCode::BadFileMode);
    error(&mut i, "INPUT #1,S$", ErrorCode::BadFileMode);
    command(&mut i, "CLOSE");
    command(&mut i, "OPEN \"keep\" FOR INPUT AS #1");
    error(&mut i, "PRINT #1,\"x\"", ErrorCode::BadFileMode);
    error(&mut i, "WRITE #1,1", ErrorCode::BadFileMode);
    error(&mut i, "PRINT #1", ErrorCode::Syntax);
    error(&mut i, "WRITE #1,", ErrorCode::BadFileMode);
}

#[test]
fn channel_types_modes_and_close_errors_follow_shared_validation_order() {
    let (directory, mut i) = setup();
    fs::write(directory.path().join("in"), "record\n").unwrap();
    for source in [
        "OPEN \"in\" FOR INPUT AS #\"1\"",
        "PRINT EOF(\"1\")",
        "PRINT #\"1\",1",
        "INPUT #\"1\",A$",
        "CLOSE #\"1\"",
    ] {
        error(&mut i, source, ErrorCode::InvalidArgument);
    }
    command(&mut i, "OPEN \"in\" FOR INPUT AS #1");
    error(&mut i, "INPUT #1,PRINT", ErrorCode::InvalidArgument);
    error(&mut i, "INPUT #1,EOF", ErrorCode::InvalidArgument);
    error(&mut i, "LINE INPUT #1,A$,B$", ErrorCode::ArgumentMismatch);
    error(&mut i, "INPUT #1,", ErrorCode::Syntax);
    error(&mut i, "WRITE #1,", ErrorCode::BadFileMode);
    error(&mut i, "CLOSE #2,#(1/0)", ErrorCode::FileNotOpen);
    command(&mut i, "INPUT #1,A$:PRINT A$");
    assert_eq!(i.take_output(), "record\n");
    command(&mut i, "OPEN \"out\" FOR OUTPUT AS #2");
    error(&mut i, "WRITE #2,", ErrorCode::Syntax);
}

#[test]
fn new_file_keywords_do_not_change_literal_data_text() {
    let (_, mut i) = setup();
    for source in [
        "10 READ A$,B$,C$,N,D$",
        "20 PRINT A$;\"|\";B$;\"|\";C$;\"|\";D$",
        "30 data los días de la semana son lunes, as, open, 1.00e-3, \"quoted: append\":print N",
    ] {
        command(&mut i, source);
    }
    assert_eq!(
        i.program.get(30).unwrap(),
        " DATA los días de la semana son lunes, as, open, 1.00e-3, \"quoted: append\" : PRINT N"
    );
    command(&mut i, "RUN");
    assert_eq!(
        i.take_output(),
        "los días de la semana son lunes|as|open|quoted: append\n 0.001\n"
    );
}

#[test]
fn append_columns_zones_using_and_no_console_output() {
    let (directory, mut i) = setup();
    fs::write(directory.path().join("a"), "prefix").unwrap();
    command(&mut i, "ZONE 8");
    command(&mut i, "OPEN \"a\" FOR APPEND AS #1");
    command(&mut i, "OPEN \"b\" FOR APPEND AS #2");
    command(&mut i, "PRINT #1,\"A\";");
    command(&mut i, "PRINT #2,\"B\";");
    command(&mut i, "PRINT #1,TAB(4);\"C\",");
    command(&mut i, "PRINT #2,TAB(3);\"D\"");
    command(&mut i, "PRINT #1,USING \"##.##\";1.5");
    command(&mut i, "CLOSE");
    assert_eq!(i.take_output(), "");
    assert_eq!(
        fs::read_to_string(directory.path().join("a")).unwrap(),
        "prefixA  C     1.50\n"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("b")).unwrap(),
        "B D\n"
    );
}

#[test]
fn stop_cont_and_chain_preserve_position_and_program_directory() {
    let (directory, mut i) = setup();
    fs::create_dir(directory.path().join("programs")).unwrap();
    fs::write(directory.path().join("programs/data"), "one\ntwo\n").unwrap();
    fs::write(
        directory.path().join("programs/next.bas"),
        "10 LINE INPUT #1,S$\n20 PRINT S$\n30 END\n",
    )
    .unwrap();
    fs::write(directory.path().join("programs/main.bas"), "10 STOP\n20 OPEN \"data\" FOR INPUT AS #1\n30 LINE INPUT #1,S$\n40 PRINT S$\n50 STOP\n60 CHAIN \"next.bas\"\n").unwrap();
    command(&mut i, "RUN \"programs/main.bas\"");
    i.take_output();
    command(&mut i, "CONT");
    assert!(i.take_output().starts_with("one\n"));
    command(&mut i, "PRINT EOF(1)");
    assert_eq!(i.take_output(), " 0\n");
    command(&mut i, "CONT");
    assert_eq!(i.take_output(), "two\n");
    error(&mut i, "PRINT EOF(1)", ErrorCode::FileNotOpen);
    assert_eq!(i.current_dir, directory.path());
}

#[test]
fn cleanup_boundaries_and_immediate_errors() {
    let (directory, mut i) = setup();
    for boundary in ["END", "CLEAR", "NEW", "RUN", "QUIT", "EXIT", "SYSTEM"] {
        command(&mut i, "OPEN \"out\" FOR APPEND AS #1");
        error(&mut i, "PRINT 1/0", ErrorCode::DivisionByZero);
        command(&mut i, "WRITE #1,1");
        command(&mut i, boundary);
        error(&mut i, "WRITE #1,1", ErrorCode::FileNotOpen);
    }
    fs::write(directory.path().join("load.bas"), "10 END\n").unwrap();
    command(&mut i, "OPEN \"out\" FOR APPEND AS #1");
    command(&mut i, "LOAD \"load.bas\"");
    error(&mut i, "WRITE #1,1", ErrorCode::FileNotOpen);
    command(&mut i, "NEW");
    command(&mut i, "10 OPEN \"out\" FOR OUTPUT AS #1");
    command(&mut i, "20 PRINT 1/0");
    error(&mut i, "RUN", ErrorCode::DivisionByZero);
    error(&mut i, "WRITE #1,1", ErrorCode::FileNotOpen);
    command(&mut i, "20 WRITE #1,42");
    command(&mut i, "RUN");
    error(&mut i, "WRITE #1,1", ErrorCode::FileNotOpen);
    assert_eq!(
        fs::read_to_string(directory.path().join("out")).unwrap(),
        "42\n"
    );
}

#[test]
fn handled_error_resume_preserves_channel() {
    let (directory, mut i) = setup();
    fs::write(directory.path().join("in"), "bad\n42\n").unwrap();
    for line in [
        "10 OPEN \"in\" FOR INPUT AS #1",
        "20 ON ERROR GOTO 60",
        "30 INPUT #1,A",
        "40 PRINT A",
        "50 END",
        "60 PRINT ERR;ERL",
        "70 RESUME 30",
    ] {
        command(&mut i, line);
    }
    command(&mut i, "RUN");
    assert_eq!(i.take_output(), " 62  30\n 42\n");
}

#[test]
fn malformed_record_recovery_consumes_one_physical_line() {
    let (directory, mut i) = setup();
    fs::write(
        directory.path().join("in"),
        b"\"bad\"junk\n\"good\"\n\xff,rest\nlast\n",
    )
    .unwrap();
    command(&mut i, "OPEN \"in\" FOR INPUT AS #1");
    error(&mut i, "INPUT #1,S$", ErrorCode::InvalidFileData);
    command(&mut i, "INPUT #1,S$:PRINT S$");
    assert_eq!(i.take_output(), "good\n");
    error(&mut i, "INPUT #1,S$", ErrorCode::InvalidFileData);
    command(&mut i, "INPUT #1,S$:PRINT S$");
    assert_eq!(i.take_output(), "last\n");
}

#[test]
fn while_eof_is_evaluated_after_every_record() {
    let (directory, mut i) = setup();
    fs::write(directory.path().join("in"), "one\ntwo\nthree").unwrap();
    for line in [
        "10 OPEN \"in\" FOR INPUT AS #1",
        "20 WHILE NOT EOF(1)",
        "30 LINE INPUT #1,S$",
        "40 PRINT S$",
        "50 WEND",
    ] {
        command(&mut i, line);
    }
    command(&mut i, "RUN");
    assert_eq!(i.take_output(), "one\ntwo\nthree\n");
}

#[test]
fn cli_program_in_current_directory_resolves_data_paths() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("main.bas"),
        "10 OPEN \"out\" FOR OUTPUT AS #1\n20 WRITE #1,42\n",
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .current_dir(directory.path())
        .arg("main.bas")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("out")).unwrap(),
        "42\n"
    );
}

#[test]
fn immediate_program_commands_follow_cd_after_run_and_stop() {
    let (directory, mut i) = setup();
    fs::create_dir(directory.path().join("ejemplos")).unwrap();
    fs::write(
        directory.path().join("ejemplos/demo.bas"),
        "10 PRINT \"SUBDIR\"\n20 END\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("raiz.bas"),
        "10 PRINT \"RAIZ\"\n20 END\n",
    )
    .unwrap();
    for source in [
        "RUN \"ejemplos/demo.bas\"",
        "CD \"ejemplos\"",
        "RUN \"demo.bas\"",
        "CD \"/\"",
        "RUN \"raiz.bas\"",
    ] {
        command(&mut i, source);
    }
    assert_eq!(i.take_output(), "SUBDIR\nSUBDIR\nRAIZ\n");
    fs::write(
        directory.path().join("ejemplos/stopped.bas"),
        "10 STOP\n20 END\n",
    )
    .unwrap();
    command(&mut i, "RUN \"ejemplos/stopped.bas\"");
    i.take_output();
    command(&mut i, "LOAD \"raiz.bas\"");
    command(&mut i, "RUN");
    assert_eq!(i.take_output(), "RAIZ\n");
}

fn mount_directory(target: &std::path::Path, link: &std::path::Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn mounted_directories_support_data_and_alias_identity_without_logical_root_escape() {
    let (directory, mut i) = setup();
    let external = tempdir().unwrap();
    mount_directory(external.path(), &directory.path().join("samples"));
    mount_directory(external.path(), &directory.path().join("alias"));
    command(&mut i, "OPEN \"samples/new.csv\" FOR OUTPUT AS #1");
    command(&mut i, "WRITE #1,\"mounted\"");
    error(
        &mut i,
        "OPEN \"alias/new.csv\" FOR INPUT AS #2",
        ErrorCode::FileAlreadyOpen,
    );
    command(&mut i, "CLOSE");
    assert_eq!(
        fs::read_to_string(external.path().join("new.csv")).unwrap(),
        "\"mounted\"\n"
    );
    command(&mut i, "CD \"samples\"");
    command(&mut i, "OPEN \"new.csv\" FOR INPUT AS #1");
    command(&mut i, "INPUT #1,S$:PRINT S$");
    assert_eq!(i.take_output(), "mounted\n");
    command(&mut i, "OPEN \"../root.csv\" FOR OUTPUT AS #2");
    command(&mut i, "WRITE #2,42:CLOSE");
    assert_eq!(
        fs::read_to_string(directory.path().join("root.csv")).unwrap(),
        "42\n"
    );
    error(
        &mut i,
        "OPEN \"../../escape.csv\" FOR OUTPUT AS #1",
        ErrorCode::InvalidArgument,
    );
    command(&mut i, "CD \"/\"");
    fs::write(directory.path().join("Mixed.Csv"), "found").unwrap();
    command(&mut i, "OPEN \"mixed.csv\" FOR INPUT AS #1");
    command(&mut i, "LINE INPUT #1,S$:PRINT S$");
    assert_eq!(i.take_output(), "found\n");
}

#[test]
fn scores_sample_runs_through_runtime_samples_directory_mount() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let runtime = tempdir().unwrap();
    let shared = tempdir().unwrap();
    fs::write(
        shared.path().join("f-scores.bas"),
        include_str!("../samples/f-scores.bas"),
    )
    .unwrap();
    mount_directory(shared.path(), &runtime.path().join("samples"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .current_dir(runtime.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"LOAD \"samples/f-scores\"\nRUN\nEXIT\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(
        result.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        stdout.contains("SAVED SCORES") && stdout.contains("PLAYER THREE"),
        "{stdout}"
    );
    assert!(!stdout.contains("Invalid argument"), "{stdout}");
    assert_eq!(
        fs::read_to_string(shared.path().join("f-scores.csv")).unwrap(),
        "\"PLAYER ONE\",1250\n\"PLAYER TWO\",980\n\"PLAYER THREE\",1420\n"
    );
    assert!(!runtime.path().join("f-scores.csv").exists());
}

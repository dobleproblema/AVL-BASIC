use std::fs;
use std::path::Path;

use avl_basic::Interpreter;

fn interpreter_in(path: &Path) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = path.to_path_buf();
    interpreter.current_dir = path.to_path_buf();
    interpreter
}

#[test]
fn save_rotates_one_backup_only_when_the_program_changes() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("game.bas");
    let backup = directory.path().join("game.bak");
    let mut interpreter = interpreter_in(directory.path());

    interpreter.process_immediate("10 PRINT 1").unwrap();
    interpreter.process_immediate(r#"SAVE "game""#).unwrap();
    assert_eq!(fs::read(&program).unwrap(), b"10 PRINT 1\n");
    assert!(!backup.exists());

    interpreter.process_immediate("10 PRINT 2").unwrap();
    interpreter.process_immediate(r#"SAVE "game.bas""#).unwrap();
    assert_eq!(fs::read(&program).unwrap(), b"10 PRINT 2\n");
    assert_eq!(fs::read(&backup).unwrap(), b"10 PRINT 1\n");

    interpreter.process_immediate(r#"SAVE "game.bas""#).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), b"10 PRINT 1\n");

    interpreter.process_immediate("10 PRINT 3").unwrap();
    interpreter.process_immediate(r#"SAVE "game.bas""#).unwrap();
    assert_eq!(fs::read(&program).unwrap(), b"10 PRINT 3\n");
    assert_eq!(fs::read(&backup).unwrap(), b"10 PRINT 2\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn save_through_a_file_alias_keeps_the_link_and_a_recoverable_backup() {
    let directory = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let target = external.path().join("source.bas");
    let alias = directory.path().join("game.bas");
    fs::write(&target, b"10 PRINT 1\n").unwrap();
    std::os::unix::fs::symlink(&target, &alias).unwrap();

    let mut interpreter = interpreter_in(directory.path());
    interpreter.process_immediate("10 PRINT 2").unwrap();
    interpreter.process_immediate(r#"SAVE "game.bas""#).unwrap();

    assert!(alias.symlink_metadata().unwrap().is_symlink());
    assert_eq!(fs::read(&target).unwrap(), b"10 PRINT 2\n");
    assert_eq!(
        fs::read(directory.path().join("game.bak")).unwrap(),
        b"10 PRINT 1\n"
    );
}

#[test]
fn save_rejects_an_empty_program_without_creating_or_touching_files() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("game.bas");
    let backup = directory.path().join("game.bak");
    fs::write(&program, "10 PRINT 2\n").unwrap();
    fs::write(&backup, "10 PRINT 1\n").unwrap();
    let mut interpreter = interpreter_in(directory.path());

    for command in [r#"SAVE "game.bas""#, r#"SAVE "new.bas""#] {
        let error = interpreter.process_immediate(command).unwrap_err();
        assert_eq!(error.display_for_basic(), "No program in memory.");
    }
    assert_eq!(fs::read(&program).unwrap(), b"10 PRINT 2\n");
    assert_eq!(fs::read(&backup).unwrap(), b"10 PRINT 1\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn save_leaves_the_original_in_place_if_the_backup_cannot_be_written() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("game.bas");
    let backup = directory.path().join("game.bak");
    fs::write(&program, "10 PRINT 1\n").unwrap();
    fs::create_dir(&backup).unwrap();
    let mut interpreter = interpreter_in(directory.path());

    interpreter.process_immediate("10 PRINT 2").unwrap();
    assert!(interpreter.process_immediate(r#"SAVE "game.bas""#).is_err());
    assert_eq!(fs::read(&program).unwrap(), b"10 PRINT 1\n");
    assert!(backup.is_dir());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn load_recovers_an_explicit_backup_with_the_usual_validation() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("game.BAK"), b"10 PRINT 1\r\n").unwrap();
    fs::write(directory.path().join("game.bas"), b"10 PRINT 2\n").unwrap();
    fs::write(directory.path().join("broken.bak"), b"10 END\n20 \xff\n").unwrap();
    let mut interpreter = interpreter_in(directory.path());

    interpreter.process_immediate("FILES").unwrap();
    let listing = interpreter.take_output();
    assert!(listing.contains("game.bas"));
    assert!(!listing.to_ascii_lowercase().contains(".bak"));
    interpreter.process_immediate(r#"CAT "*.bak""#).unwrap();
    let listing = interpreter.take_output();
    assert!(listing.contains("game.BAK"));
    assert!(listing.contains("broken.bak"));

    interpreter.process_immediate(r#"LOAD "game""#).unwrap();
    assert_eq!(interpreter.program.list(), "10 PRINT 2\n");
    interpreter.process_immediate(r#"LOAD "game.bak""#).unwrap();
    assert_eq!(interpreter.program.list(), "10 PRINT 1\n");
    assert!(interpreter
        .process_immediate(r#"LOAD "broken.bak""#)
        .is_err());
    assert_eq!(interpreter.program.list(), "10 PRINT 1\n");
    for command in [
        r#"SAVE "game.bak""#,
        r#"RUN "game.bak""#,
        r#"CHAIN "game.bak""#,
        r#"MERGE "game.bak""#,
    ] {
        assert!(interpreter.process_immediate(command).is_err(), "{command}");
    }
    interpreter
        .process_immediate(r#"SAVE "recovered.bas""#)
        .unwrap();
    assert_eq!(
        fs::read(directory.path().join("recovered.bas")).unwrap(),
        b"10 PRINT 1\n"
    );
    assert_eq!(
        fs::read(directory.path().join("game.BAK")).unwrap(),
        b"10 PRINT 1\r\n"
    );
}

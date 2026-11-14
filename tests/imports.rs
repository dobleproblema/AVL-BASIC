use avl_basic::{DebugAction, Debugger, ErrorCode, Interpreter, RunOutcome};
use std::fs;
use std::path::Path;

fn interpreter_at(root: &Path) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = root.to_path_buf();
    interpreter.current_dir = root.to_path_buf();
    interpreter
}

#[test]
fn imported_sub_can_redim_a_string_array_parameter() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(
        root.join("library.bas"),
        "10 DEF SUB Grow(Items$)\n20 REDIM Items$(2)\n30 Items$(2)=\"new\"\n40 SUBEND\n",
    )
    .unwrap();
    let mut interpreter = interpreter_at(root);
    interpreter
        .program
        .load_text(concat!(
            "10 IMPORT \"library.bas\" AS Lib\n",
            "20 DIM Words$(1)\n",
            "30 Words$(1)=\"kept\"\n",
            "40 CALL Lib.Grow(Words$)\n",
            "50 PRINT UBOUND(Words$,1);Words$(1);Words$(2)\n",
        ))
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 2 keptnew\n");
}

#[test]
fn module_control_errors_preserve_borrowed_arrays_and_caller_flow() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut interpreter = interpreter_at(root);
    interpreter
        .program
        .load_text(concat!(
            "10 IMPORT \"library.bas\" AS Lib\n",
            "20 DIM Items(1):Items(1)=7\n",
            "30 ON ERROR RESUME NEXT\n",
            "40 CALL Lib.Change(Items)\n",
            "50 PRINT ERL;Items(1)\n",
        ))
        .unwrap();

    for command in ["STOP", "STOP 1", "CLEAR", "CLEAR EXTRA"] {
        fs::write(
            root.join("library.bas"),
            format!("10 DEF SUB Change(Values)\n20 {command}\n30 SUBEND\n"),
        )
        .unwrap();
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
        assert_eq!(interpreter.take_output(), " 20  7\n", "{command}");
    }
}

#[test]
fn imported_routines_share_parameters_and_keep_their_own_lines_and_names() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join("game/lib")).unwrap();
    fs::write(
        root.join("game/main.bas"),
        concat!(
            "10 IMPORT \"lib/timing.bas\" AS Timing\n",
            "20 GOTO Ready\n",
            "30 PRINT \"WRONG BRANCH\":END\n",
            "80 DEF FNSpan(N)=N*10\n",
            "120 Ready:\n",
            "130 DIM SplitTimes(1),BestTimes(1)\n",
            "140 SplitTimes(0)=4:SplitTimes(1)=11\n",
            "150 IF 1 THEN CALL tiMing.Record(SplitTimes,BestTimes)\n",
            "160 PRINT Timing.FNClock$(BestTimes(1),\"LAP \")\n",
            "170 PRINT FNSpan(10)\n",
            "180 END\n",
        ),
    )
    .unwrap();
    fs::write(
        root.join("game/lib/timing.bas"),
        concat!(
            "10 ' Timing helpers\n",
            "20 DEF FNSpan(Cuts,Idx)\n",
            "30 FNSpan=Cuts(Idx)-Cuts(Idx-1)\n",
            "40 FNEND\n",
            "50 DEF FNClock$(Seconds,Prefix$)\n",
            "60 FNClock$=Prefix$+DEC$(Seconds,\"00\")\n",
            "70 FNEND\n",
            "80 DEF SUB Record(Times,Bests)\n",
            "90 LOCAL Duration\n",
            "100 GOTO Ready\n",
            "110 PRINT \"WRONG LABEL\"\n",
            "120 Ready:\n",
            "130 Duration=FNSpan(Times,1)\n",
            "140 IF Bests(1)=0 OR Duration<Bests(1) THEN Bests(1)=Duration\n",
            "150 SUBEND\n",
        ),
    )
    .unwrap();

    let mut interpreter = interpreter_at(root);
    interpreter.load_file(&root.join("game/main.bas")).unwrap();
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(interpreter.take_output(), "LAP 07\n 100\n");
    assert!(!interpreter.program.list().contains("DEF SUB Record"));
}

#[test]
fn invalid_imports_are_rejected_before_running_the_main_program() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("good.bas"), "10 DEF FNValue(N)=N+1\n").unwrap();
    fs::write(
        root.join("bad.bas"),
        "10 DEF FNValue(N)=N+1\r\n\r\nmissing line number\r\n",
    )
    .unwrap();
    fs::write(
        root.join("executable.bas"),
        "10 PRINT \"LIBRARY SIDE EFFECT\"\n",
    )
    .unwrap();

    for (declarations, diagnostic) in [
        ("20 IMPORT \"missing.bas\" AS Timing", "missing.bas"),
        ("20 IMPORT \"bad.bas\" AS Timing", "bad.bas"),
        ("20 IMPORT \"executable.bas\" AS Timing", "executable.bas"),
        (
            "20 IMPORT \"good.bas\" AS Timing\n30 IMPORT \"good.bas\" AS tiMing",
            "timing",
        ),
        ("20 IMPORT", "expected import"),
        ("20 IF 0 THEN IMPORT \"good.bas\" AS Timing", "standalone"),
        (
            "20 IMPORT \"good.bas\" AS Timing:PRINT \"EXTRA STATEMENT\"",
            "standalone",
        ),
    ] {
        fs::write(
            root.join("main.bas"),
            format!("10 PRINT \"MAIN SIDE EFFECT\"\n{declarations}\n40 END\n"),
        )
        .unwrap();
        let mut interpreter = interpreter_at(root);
        interpreter.process_immediate("LOAD \"main.bas\"").unwrap();
        let error = interpreter.run_loaded().unwrap_err().display_for_basic();
        assert!(
            error.to_ascii_lowercase().contains(diagnostic),
            "{declarations}: {error}"
        );
        assert_eq!(interpreter.take_output(), "", "{declarations}");
    }
}

#[test]
fn import_is_still_available_as_a_variable_or_array_name() {
    let mut interpreter = Interpreter::new();
    for line in [
        "10 Import=3",
        "20 DIM Import(1)",
        "30 Import (1)=7",
        "40 IF 1 THEN Import = Import+2",
        "50 PRINT Import;Import(1)",
    ] {
        interpreter.process_immediate(line).unwrap();
    }
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(interpreter.take_output(), " 5  7\n");
}

#[test]
fn mutations_through_two_aliases_to_the_same_array_survive_an_imported_error() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(
        root.join("timing.bas"),
        concat!(
            "10 DEF SUB UpdateBoth(A,B)\n",
            "20 A(0)=A(0)+1\n",
            "30 B(1)=B(0)*2\n",
            "40 ERROR 5\n",
            "50 SUBEND\n",
        ),
    )
    .unwrap();

    let mut interpreter = interpreter_at(root);
    for line in [
        "10 IMPORT \"timing.bas\" AS Timing",
        "20 ON ERROR GOTO 100",
        "30 DIM Times(1):Times(0)=7",
        "50 CALL Timing.UpdateBoth(Times,Times):PRINT Times(0);Times(1)",
        "60 END",
        "100 PRINT ERR;ERL",
        "110 RESUME NEXT",
    ] {
        interpreter.process_immediate(line).unwrap();
    }
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(interpreter.take_output(), " 5  40\n 8  16\n");
}

#[test]
fn qualified_functions_can_return_arrays_from_libraries_with_the_same_numbering() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for (file, increment) in [("one.bas", 5), ("two.bas", 8)] {
        fs::write(
            root.join(file),
            format!("10 DEF FNMake(A)\n20 MAT FNMake=A+{increment}\n30 FNEND\n"),
        )
        .unwrap();
    }

    let mut interpreter = interpreter_at(root);
    for line in [
        "10 IMPORT \"one.bas\" AS First",
        "20 IMPORT \"two.bas\" AS Second",
        "30 DIM Source(1):MAT Source=2",
        "40 MAT Result=First.FNMake(Source)+Second.FNMake(Source)",
        "50 PRINT Result(0);Result(1);Source(0)",
    ] {
        interpreter.process_immediate(line).unwrap();
    }
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(interpreter.take_output(), " 17  17  2\n");
}

#[test]
fn cont_keeps_the_imported_code_and_private_globals_until_the_next_run() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let library = root.join("timing.bas");
    fs::write(
        root.join("main.bas"),
        concat!(
            "10 IMPORT \"timing.bas\" AS Timing\n",
            "20 Counter=41\n",
            "30 PRINT Timing.FNValue()\n",
            "40 CALL Timing.Bump()\n",
            "50 STOP\n",
            "60 PRINT Timing.FNValue()\n",
            "70 PRINT Counter\n",
            "80 END\n",
        ),
    )
    .unwrap();
    fs::write(
        &library,
        "10 DEF FNValue=Counter\n20 DEF SUB Bump\n30 Counter=Counter+1\n40 SUBEND\n",
    )
    .unwrap();

    let mut interpreter = interpreter_at(root);
    interpreter.process_immediate("LOAD \"main.bas\"").unwrap();
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
    assert_eq!(interpreter.take_output(), " 0\nLine 50. Program stopped.\n");

    fs::write(
        &library,
        "10 DEF FNValue=Counter+100\n20 DEF SUB Bump\n30 Counter=Counter+10\n40 SUBEND\n",
    )
    .unwrap();
    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), " 1\n 41\n");

    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
    assert_eq!(
        interpreter.take_output(),
        " 100\nLine 50. Program stopped.\n"
    );
    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), " 110\n 41\n");
}

#[test]
fn debugger_and_runtime_errors_identify_the_imported_source() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(
        root.join("main.bas"),
        "10 IMPORT \"timing.bas\" AS Timing\n20 CALL Timing.Fail(0)\n30 END\n",
    )
    .unwrap();
    fs::write(
        root.join("timing.bas"),
        "10 DEF SUB Fail(Divisor)\n20 PRINT 1/Divisor\n30 SUBEND\n",
    )
    .unwrap();
    let mut interpreter = interpreter_at(root);
    interpreter.load_file(&root.join("main.bas")).unwrap();
    let mut debugger = Debugger::scripted([DebugAction::StepInto, DebugAction::Continue]);
    debugger.set_breakpoint(20, true);
    interpreter.set_debugger(debugger);

    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.code, ErrorCode::DivisionByZero);
    assert_eq!(error.line, Some(20));
    let diagnostic = error.display_for_basic();
    assert!(diagnostic.contains("timing.bas"), "{diagnostic}");

    let snapshots = interpreter.debugger().unwrap().snapshots();
    assert_eq!(snapshots[0].location.line, 20);
    assert!(snapshots[0].location.source.contains("CALL Timing.Fail(0)"));
    let library_pause = &snapshots[1];
    assert_eq!(library_pause.location.line, 20);
    assert_eq!(library_pause.location.command, "PRINT 1/Divisor");
    assert!(library_pause
        .location
        .source_path
        .as_deref()
        .is_some_and(|path| path.ends_with("timing.bas")));
}

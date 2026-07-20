use avl_basic::{console, ErrorCode, Interpreter};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn run_rust(program: &str) -> String {
    let mut interp = Interpreter::new();
    interp.process_immediate("ZONE 8").unwrap();
    for line in program.lines() {
        interp.process_immediate(line).unwrap();
    }
    interp.process_immediate("RUN").unwrap();
    interp.take_output()
}

fn run_rust_cli(program: &str, input: &str) -> String {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(program.as_bytes()).unwrap();
    file.flush().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .arg(file.path())
        .env("AVL_BASIC_PRINT_ZONE_DEFAULT", "8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "interpreter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run_rust_cli_error(program: &str, input: &str) -> (String, String) {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(program.as_bytes()).unwrap();
    file.flush().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .arg(file.path())
        .env("AVL_BASIC_PRINT_ZONE_DEFAULT", "8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "interpreter unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn run_rust_cli_with_timeout(program: &str, input: &str, timeout: Duration) -> String {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(program.as_bytes()).unwrap();
    file.flush().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .arg(file.path())
        .env("AVL_BASIC_PRINT_ZONE_DEFAULT", "8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    drop(child.stdin.take());

    let deadline = Instant::now() + timeout;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "interpreter timed out; stdout: {}; stderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "interpreter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run_rust_cli_with_print_zone_default(program: &str, zone_default: Option<&str>) -> String {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(program.as_bytes()).unwrap();
    file.flush().unwrap();

    let mut command = Command::new(env!("CARGO_BIN_EXE_avl-basic"));
    command
        .arg(file.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    if let Some(zone) = zone_default {
        command.env("AVL_BASIC_PRINT_ZONE_DEFAULT", zone);
    } else {
        command.env_remove("AVL_BASIC_PRINT_ZONE_DEFAULT");
    }

    let output = command.spawn().unwrap().wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "interpreter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run_rust_error_code(program: &str) -> ErrorCode {
    let mut interp = Interpreter::new();
    for line in program.lines() {
        interp.process_immediate(line).unwrap();
    }
    interp.process_immediate("RUN").unwrap_err().code
}

#[test]
fn default_print_zone_is_22_with_test_override_available() {
    let program = r#"10 PRINT "a","b"
20 END"#;
    assert_eq!(
        run_rust_cli_with_print_zone_default(program, None),
        format!("a{}b\n", " ".repeat(21))
    );
    assert_eq!(
        run_rust_cli_with_print_zone_default(program, Some("8")),
        "a       b\n"
    );
}

#[test]
fn timers_fire_during_busy_goto_loop_without_pause() {
    let output = run_rust_cli_with_timeout(
        r#"10 EVERY 1 GOSUB 50
20 GOTO 20
50 PRINT "TICK"
60 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "TICK\n");
}

#[test]
fn timer_target_line_must_be_literal() {
    assert_eq!(
        run_rust_error_code("10 AFTER 1 GOSUB 100+10\n20 GOTO 20\n110 END"),
        ErrorCode::InvalidLineNumber
    );
}

#[test]
fn numbered_after_remain_uses_real_time_and_missing_return_ends_run() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 50,1 GOSUB 60
20 AFTER 25,2 GOSUB 70
30 GOTO 30
60 PRINT "T1 ";REMAIN(1) : RETURN
70 PRINT "LEFT ";REMAIN(1)
80 PRINT "DONE""#,
        "",
        Duration::from_secs(2),
    );

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "{output:?}");
    assert!(lines[0].starts_with("LEFT "), "{output:?}");
    let left: i32 = lines[0]["LEFT ".len()..].trim().parse().unwrap();
    assert!(
        (1..50).contains(&left),
        "REMAIN(1) should be partially elapsed, got {left}; output: {output:?}"
    );
    assert_eq!(lines[1], "DONE");
}

#[test]
fn timer_default_number_zero_can_be_cancelled_independently() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 2 GOSUB 100
20 CANCEL 0
30 AFTER 4,1 GOSUB 200
40 GOTO 40
100 PRINT "BAD"
110 END
200 PRINT "OK"
210 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "OK\n");
}

#[test]
fn cancel_timer_disables_only_selected_number() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 2,1 GOSUB 100
20 AFTER 4,2 GOSUB 200
30 CANCEL 1
40 GOTO 40
100 PRINT "BAD"
110 END
200 PRINT "OK"
210 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "OK\n");
}

#[test]
fn rescheduling_same_timer_number_replaces_previous_target() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 2,1 GOSUB 100
20 AFTER 4,1 GOSUB 200
30 GOTO 30
100 PRINT "OLD"
110 END
200 PRINT "NEW"
210 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "NEW\n");
}

#[test]
fn after_is_one_shot_and_every_repeats_until_cancelled() {
    let output = run_rust_cli_with_timeout(
        r#"10 A=0:E=0
20 AFTER 2,1 GOSUB 100
30 EVERY 1,2 GOSUB 200
40 AFTER 8,0 GOSUB 300
50 GOTO 50
100 A=A+1
110 RETURN
200 E=E+1
210 RETURN
300 CANCEL 2
310 PRINT "A";A
320 PRINT "E";E
330 END"#,
        "",
        Duration::from_secs(2),
    );

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "{output:?}");
    assert_eq!(lines[0], "A 1", "{output:?}");
    let every_count: i32 = lines[1]["E ".len()..].trim().parse().unwrap();
    assert!(
        every_count >= 2,
        "EVERY should repeat before the ending AFTER fires; output: {output:?}"
    );
}

#[test]
fn remain_reports_inactive_zero_and_does_not_modify_timer() {
    let output = run_rust_cli_with_timeout(
        r#"10 PRINT REMAIN(3)
20 AFTER 200,3 GOSUB 200
30 A=REMAIN(3)
40 B=REMAIN(3):C=REMAIN(3):D=REMAIN(3):E=REMAIN(3):F=REMAIN(3)
50 IF A-F<3 THEN PRINT "OK" ELSE PRINT "BAD";A;F
60 CANCEL 3
70 PRINT REMAIN(3)
80 END
200 PRINT "BAD FIRED"
210 END"#,
        "",
        Duration::from_secs(2),
    );

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 3, "{output:?}");
    assert_eq!(lines[0].trim(), "0");
    assert_eq!(lines[1], "OK");
    assert_eq!(lines[2].trim(), "0");
}

#[test]
fn di_defers_timer_until_ei_dispatches_it() {
    let output = run_rust_cli_with_timeout(
        r#"10 DI
20 AFTER 1,1 GOSUB 100
30 PAUSE 80
40 PRINT "BEFORE EI"
50 EI
60 PRINT "AFTER EI"
70 END
100 PRINT "TIMER"
110 RETURN"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "BEFORE EI\nTIMER\nAFTER EI\n");
}

#[test]
fn return_from_timer_isr_restores_interrupts_after_di_inside_handler() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 1,1 GOSUB 100
20 AFTER 4,2 GOSUB 200
30 GOTO 30
100 PRINT "ONE"
110 DI
120 RETURN
200 PRINT "TWO"
210 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "ONE\nTWO\n");
}

#[test]
fn higher_priority_timer_interrupts_lower_priority_isr() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 1,1 GOSUB 100
20 AFTER 2,3 GOSUB 200
30 GOTO 30
100 PRINT "LOW START"
110 PAUSE 80
120 PRINT "LOW END"
130 END
200 PRINT "HIGH"
210 RETURN"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "LOW START\nHIGH\nLOW END\n");
}

#[test]
fn lower_priority_timer_waits_until_higher_priority_isr_returns() {
    let output = run_rust_cli_with_timeout(
        r#"10 AFTER 1,3 GOSUB 100
20 AFTER 2,1 GOSUB 200
30 GOTO 30
100 PRINT "HIGH START"
110 PAUSE 80
120 PRINT "HIGH END"
130 RETURN
200 PRINT "LOW"
210 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "HIGH START\nHIGH END\nLOW\n");
}

#[test]
fn interrupts_sample_timer_pattern_runs_to_completion() {
    let output = run_rust_cli_with_timeout(
        r#"10 S$="Z"
20 A$=""
30 EVERY 2,1 GOSUB 100
40 EVERY 1,2 GOSUB 200
50 AFTER 8,0 GOSUB 300
60 PAUSE
70 END
100 A$="Z"
110 PRINT "GUESS"
120 RETURN
200 IF A$=S$ THEN DI:GOTO 400
210 RETURN
300 PRINT "TIME"
310 END
400 PRINT "WIN"
410 END"#,
        "",
        Duration::from_secs(2),
    );

    assert_eq!(output, "GUESS\nWIN\n");
}

#[test]
fn timed_pause_keeps_deadline_across_timer_interrupts() {
    let output = run_rust_cli_with_timeout(
        r#"10 EVERY 5,1 GOSUB 100
20 PAUSE 250
30 CANCEL 1
40 PRINT "DONE";C
50 END
100 C=C+1
110 RETURN"#,
        "",
        Duration::from_secs(2),
    );

    let line = output.trim();
    assert!(line.starts_with("DONE "), "{output:?}");
    let count: i32 = line["DONE ".len()..].trim().parse().unwrap();
    assert!(
        count >= 1,
        "timer should interrupt PAUSE at least once; output: {output:?}"
    );
}

#[test]
fn timer_numbers_outside_zero_to_three_are_rejected() {
    let cases = [
        "10 AFTER 1,4 GOSUB 100\n100 END",
        "10 EVERY 1,-1 GOSUB 100\n100 END",
        "10 CANCEL 4\n20 END",
        "10 PRINT REMAIN(4)\n20 END",
    ];

    for program in cases {
        assert_eq!(run_rust_error_code(program), ErrorCode::InvalidArgument);
    }
}

#[test]
fn ctrl_c_during_run_interrupts_program_without_losing_cont() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 1").unwrap();
    interp.process_immediate("20 PRINT 2").unwrap();

    interp.request_interrupt_for_test();
    let err = interp.process_immediate("RUN").unwrap_err();

    assert_eq!(err.code, ErrorCode::KeyboardInterrupt);
    assert_eq!(
        err.display_for_basic(),
        "Line 10. Execution interrupted by user."
    );
    assert_eq!(interp.take_output(), "");

    interp.process_immediate("CONT").unwrap();
    assert_eq!(interp.take_output(), " 1\n 2\n");
}

#[test]
fn immediate_goto_after_stop_resumes_at_target_preserving_runtime() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 A=1").unwrap();
    interp.process_immediate("20 STOP").unwrap();
    interp.process_immediate("30 A=A+1").unwrap();
    interp.process_immediate("40 PRINT A").unwrap();

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "Line 20. Program stopped.\n");

    interp.process_immediate("GOTO 30").unwrap();
    assert_eq!(interp.take_output(), " 2\n");
}

#[test]
fn immediate_end_cancels_stopped_program_and_does_not_poison_next_immediate_line() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 STOP").unwrap();
    interp.process_immediate("RUN").unwrap();

    interp.process_immediate("END").unwrap();
    let err = interp.process_immediate("CONT").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "There is no stopped program to continue."
    );

    interp.process_immediate("PRINT 1:PRINT 2").unwrap();
    assert_eq!(interp.take_output(), "Line 10. Program stopped.\n 1\n 2\n");
}

#[test]
fn immediate_colon_commands_run_after_stopped_program_without_losing_cont() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 STOP").unwrap();
    interp.process_immediate("20 PRINT \"CONT\"").unwrap();

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "Line 10. Program stopped.\n");

    interp
        .process_immediate("MAT BASE 1:DIM tester(3,3)")
        .unwrap();
    interp
        .process_immediate("PRINT LBOUND(tester):PRINT UBOUND(tester,2)")
        .unwrap();
    assert_eq!(interp.take_output(), " 1\n 3\n");

    interp.process_immediate("CONT").unwrap();
    assert_eq!(interp.take_output(), "CONT\n");
}

#[test]
fn editing_program_after_stop_invalidates_cont_with_standard_error() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT \"A\"").unwrap();
    interp.process_immediate("20 STOP").unwrap();
    interp.process_immediate("30 PRINT \"B\"").unwrap();

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "A\nLine 20. Program stopped.\n");

    interp.process_immediate("30 PRINT \"C\"").unwrap();
    let err = interp.process_immediate("CONT").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "There is no stopped program to continue."
    );
    assert_eq!(interp.take_output(), "");
}

#[test]
fn immediate_assignment_after_stop_keeps_cont_available() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 A=1").unwrap();
    interp.process_immediate("20 STOP").unwrap();
    interp.process_immediate("30 PRINT A").unwrap();

    interp.process_immediate("RUN").unwrap();
    interp.process_immediate("A=7").unwrap();
    interp.process_immediate("CONT").unwrap();

    assert_eq!(interp.take_output(), "Line 20. Program stopped.\n 7\n");
}

#[test]
fn run_file_clears_previous_routine_line_owners() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("first.bas"),
        "100 DEF SUB OLD\n200 PRINT 9\n300 SUBEND\n400 CALL OLD\n500 END\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("second.bas"),
        "100 GOSUB 200\n110 END\n200 PRINT 2\n210 RETURN\n",
    )
    .unwrap();

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    interp.process_immediate("RUN \"first.bas\"").unwrap();
    assert_eq!(interp.take_output(), " 9\n");

    interp.process_immediate("RUN \"second.bas\"").unwrap();
    assert_eq!(interp.take_output(), " 2\n");
}

#[test]
fn print_and_arithmetic_baseline() {
    let output = run_rust(
        r#"10 PRINT 2+3*4
20 PRINT "HOLA"+" "+"BASIC"
30 PRINT 7=7
40 END"#,
    );
    assert_eq!(output, " 14\nHOLA BASIC\n-1\n");
}

#[test]
fn mod_uses_classic_basic_remainder_sign() {
    let output = run_rust(
        r#"10 PRINT -5 MOD 3
20 PRINT 5 MOD -3
30 PRINT -5 MOD -3
40 PRINT 5.5 MOD 2"#,
    );
    assert_eq!(output, "-2\n 2\n-2\n 1.5\n");
}

#[test]
fn hex_and_bin_strings_use_documented_twos_complement_widths() {
    let output = run_rust(
        r#"10 PRINT HEX$(255)
20 PRINT HEX$(255,4)
30 PRINT HEX$(-1)
40 PRINT HEX$(-1,4)
50 PRINT BIN$(10)
60 PRINT BIN$(10,8)
70 PRINT BIN$(-1)
80 PRINT BIN$(-1,4)
90 PRINT BIN$(10,3)"#,
    );

    assert_eq!(
        output,
        "FF\n00FF\nFF\nFFFF\n1010\n00001010\n11111111\n1111\n010\n"
    );
}

#[test]
fn hex_and_bin_reject_negative_widths_and_non_finite_values() {
    assert_eq!(
        run_rust_error_code("10 PRINT HEX$(255,-1)"),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        run_rust_error_code("10 PRINT BIN$(5,-1)"),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        run_rust_error_code("10 PRINT HEX$(1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT BIN$(1E309)"),
        ErrorCode::Overflow
    );
}

#[test]
fn string_repeat_helpers_reject_invalid_counts_and_empty_pattern() {
    let output = run_rust(r#"10 PRINT "[";SPACE$(-2);STRING$(3," -");"]""#);
    assert_eq!(output, "[---]\n");

    assert_eq!(
        run_rust_error_code("10 PRINT SPACE$(1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT STRING$(1E309,\"x\")"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT STRING$(3,1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT STRING$(3,\"\")"),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn string_slice_helpers_reject_non_finite_counts_and_positions() {
    assert_eq!(
        run_rust_error_code("10 PRINT LEFT$(\"abc\",1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT RIGHT$(\"abc\",1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT MID$(\"abc\",1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT MID$(\"abc\",1,1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT INSTR(1E309,\"abc\",\"a\")"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT CHR$(1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT CHR$(-1)"),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn print_spacing_and_pause_reject_non_finite_counts() {
    assert_eq!(
        run_rust_error_code("10 PRINT SPC(1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(
        run_rust_error_code("10 PRINT TAB(1E309)"),
        ErrorCode::Overflow
    );
    assert_eq!(run_rust_error_code("10 PAUSE 1E309"), ErrorCode::Overflow);
}

#[test]
fn rgb_accepts_string_components_and_rejects_invalid_colors() {
    let output = run_rust(
        r#"10 PRINT RGB("1","2","3"),RGB$("1","2","3")
20 PRINT RGB$("4,0,251")"#,
    );
    assert_eq!(output, " 66051  1,2,3\n4,0,251\n");

    assert_eq!(
        run_rust_error_code("10 PRINT RGB(\"256\",\"0\",\"0\")"),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        run_rust_error_code("10 PRINT RGB$(300,0,0)"),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        run_rust_error_code("10 PRINT RGB$(-1)"),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        run_rust_error_code("10 PRINT RGB(1E309)"),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn not_has_logical_precedence_after_relational_operations() {
    let output = run_rust(
        r#"10 IF NOT "juan"<"pepe" THEN PRINT "verdadero" ELSE PRINT "falso"
20 IF NOT 5=2 THEN PRINT "verdadero" ELSE PRINT "falso"
30 PRINT NOT 5=2
40 PRINT NOT (5=2)
50 PRINT NOT 5+2
60 PRINT NOT 5 AND 1
70 END"#,
    );
    assert_eq!(output, "falso\nverdadero\n-1\n-1\n-8\n 0\n");
}

#[test]
fn cached_numeric_conditions_and_array_assignments_preserve_semantics() {
    let output = run_rust(
        r#"10 DIM A(2)
20 I=1:A(I)=SQR(9)+ABS(-2)
30 X=0
40 IF NOT A(I)=4 AND ABS(A(I)-8)=3 THEN X=1
50 WHILE X<4:X=X+1:WEND
60 PRINT A(I),X"#,
    );
    assert_eq!(output, " 5       4\n");

    let output = run_rust(
        r#"10 DIM L$(2)
20 L$(1)="B":L$(2)="A"
30 IF L$(1)<L$(2) THEN PRINT "BAD" ELSE PRINT "OK""#,
    );
    assert_eq!(output, "OK\n");

    assert_eq!(
        run_rust_error_code("10 IF SQR(-1) THEN END"),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        run_rust_error_code("10 IF SQR(INF) THEN END"),
        ErrorCode::Overflow
    );
}

#[test]
fn print_using_detection_does_not_split_utf8_text() {
    let output = run_rust("10 PRINT \"aqu\u{00ed}\"");
    assert_eq!(output, "aqu\u{00ed}\n");
}

#[test]
fn immediate_cat_save_load_delete_and_list_ranges() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("alpha.bas"), "10 PRINT 1\n").unwrap();
    std::fs::write(temp.path().join("notes.txt"), "ignored\n").unwrap();
    std::fs::create_dir(temp.path().join("subdir")).unwrap();

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    interp.process_immediate("CAT").unwrap();
    let cat = interp.take_output();
    assert!(cat.contains("alpha.bas"));
    assert!(cat.contains("subdir/"));
    assert!(!cat.contains("notes.txt"));

    interp.process_immediate("10 print 1").unwrap();
    interp.process_immediate("20 print 2").unwrap();
    interp.process_immediate("30 print 3").unwrap();
    interp.process_immediate("LIST 20-").unwrap();
    assert_eq!(interp.take_output(), "20 PRINT 2\n30 PRINT 3\n");

    interp.process_immediate("DELETE -20").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(interp.take_output(), "30 PRINT 3\n");

    interp.process_immediate("SAVE \"saved").unwrap();
    assert!(temp.path().join("saved.bas").exists());

    interp.process_immediate("NEW").unwrap();
    interp.process_immediate("LOAD \"saved").unwrap();
    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), " 3\n");
}

#[test]
fn immediate_files_separates_entries_longer_than_default_column() {
    let temp = tempfile::tempdir().unwrap();
    for name in [
        "a.bas",
        "pimachin-modern-test.bas",
        "pimachin.bas",
        "zeta.bas",
    ] {
        std::fs::write(temp.path().join(name), "10 PRINT \"OK\"\n").unwrap();
    }

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    interp.process_immediate("FILES").unwrap();
    let output = interp.take_output();
    assert!(!output.contains("pimachin-modern-test.baspimachin.bas"));
    assert_eq!(
        output.split_whitespace().collect::<Vec<_>>(),
        vec![
            "a.bas",
            "pimachin-modern-test.bas",
            "pimachin.bas",
            "zeta.bas",
        ]
    );
}

#[test]
fn renum_fourth_parameter_renumbers_arbitrary_block() {
    let mut interp = Interpreter::new();
    for line in [
        "10 GOTO 30",
        "20 PRINT \"A\"",
        "30 GOSUB 60",
        "40 PRINT \"B\"",
        "50 END",
        "60 RETURN",
    ] {
        interp.process_immediate(line).unwrap();
    }

    interp.process_immediate("RENUM 21,7,20,40").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "10 GOTO 28\n21 PRINT \"A\"\n28 GOSUB 60\n35 PRINT \"B\"\n50 END\n60 RETURN\n"
    );
}

#[test]
fn renum_fourth_parameter_can_move_block_to_later_gap() {
    let mut interp = Interpreter::new();
    for line in [
        "100 PRINT \"START\"",
        "180 PRINT \"BEFORE\"",
        "190 GOTO 215",
        "200 PRINT \"MOVED\"",
        "210 GOSUB 330",
        "215 PRINT \"ENDMOVE\"",
        "220 PRINT \"AFTER OLD\"",
        "290 PRINT \"TARGET BEFORE\"",
        "300 END",
        "330 RETURN",
    ] {
        interp.process_immediate(line).unwrap();
    }

    interp.process_immediate("RENUM 295,1,190,215").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "100 PRINT \"START\"\n180 PRINT \"BEFORE\"\n220 PRINT \"AFTER OLD\"\n290 PRINT \"TARGET BEFORE\"\n295 GOTO 298\n296 PRINT \"MOVED\"\n297 GOSUB 330\n298 PRINT \"ENDMOVE\"\n300 END\n330 RETURN\n"
    );
}

#[test]
fn renum_fourth_parameter_reorders_data_after_move() {
    let mut interp = Interpreter::new();
    for line in [
        "10 DATA \"A\"",
        "20 DATA \"B\"",
        "30 READ A$",
        "40 READ B$",
        "50 PRINT A$;B$",
    ] {
        interp.process_immediate(line).unwrap();
    }

    interp.process_immediate("RENUM 5,1,20,20").unwrap();
    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "BA\n");
}

#[test]
fn renum_fourth_parameter_rejects_incompatible_projection() {
    let mut interp = Interpreter::new();
    for line in [
        "10 PRINT \"A\"",
        "20 PRINT \"B\"",
        "30 PRINT \"C\"",
        "40 PRINT \"D\"",
        "50 PRINT \"E\"",
    ] {
        interp.process_immediate(line).unwrap();
    }

    let err = interp.process_immediate("RENUM 45,10,20,40").unwrap_err();
    assert_eq!(err.display_for_basic(), "Invalid argument.");
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "10 PRINT \"A\"\n20 PRINT \"B\"\n30 PRINT \"C\"\n40 PRINT \"D\"\n50 PRINT \"E\"\n"
    );
}

#[test]
fn renum_accepts_zero_as_explicit_from_line() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 GOTO 20").unwrap();
    interp.process_immediate("20 END").unwrap();

    interp.process_immediate("RENUM 100,10,0").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(interp.take_output(), "100 GOTO 110\n110 END\n");
}

#[test]
fn missing_load_file_reports_basic_file_not_found() {
    let mut interp = Interpreter::new();
    let err = interp
        .process_immediate("LOAD \"este-programa no existel.bas\"")
        .expect_err("missing LOAD target should fail");
    assert_eq!(err.display_for_basic(), "File not found.");
}

#[test]
fn cd_reports_python_path_errors_and_keeps_virtual_root() {
    let temp = tempfile::tempdir().unwrap();
    let examples = temp.path().join("examples");
    std::fs::create_dir(&examples).unwrap();

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    let err = interp.process_immediate("CD").unwrap_err();
    assert_eq!(err.display_for_basic(), "Invalid argument.");

    let err = interp.process_immediate("CD missing").unwrap_err();
    assert_eq!(err.display_for_basic(), "Missing quotes.");

    let err = interp.process_immediate("CD \"fol\"").unwrap_err();
    assert_eq!(err.display_for_basic(), "File not found.");
    assert_eq!(interp.current_dir, temp.path());

    interp.process_immediate("CD \"examples\"").unwrap();
    assert!(interp.current_dir.ends_with("examples"));

    let err = interp.process_immediate("CD \"dgo\"").unwrap_err();
    assert_eq!(err.display_for_basic(), "File not found.");
    assert!(interp.current_dir.ends_with("examples"));

    interp.process_immediate("CD \"..\"").unwrap();
    assert_eq!(
        interp.current_dir.canonicalize().unwrap(),
        temp.path().canonicalize().unwrap()
    );
}

#[test]
#[cfg(windows)]
fn cd_and_files_accept_directory_junctions_inside_virtual_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let shared = temp.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&shared).unwrap();
    std::fs::write(shared.join("demo.bas"), "10 PRINT \"DEMO\"\n").unwrap();

    let junction = root.join("samples");
    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&shared)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mklink /J failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut interp = Interpreter::new();
    interp.root_dir = root.clone();
    interp.current_dir = root.clone();

    interp.process_immediate("FILES").unwrap();
    assert!(interp.take_output().contains("samples/"));

    interp.process_immediate("CD \"samples\"").unwrap();
    assert!(interp.current_dir.ends_with("samples"));

    interp.process_immediate("FILES").unwrap();
    assert!(interp.take_output().contains("demo.bas"));
}

#[test]
fn immediate_file_command_errors_match_python() {
    let mut interp = Interpreter::new();
    assert_eq!(
        interp
            .process_immediate("LOAD")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("LOAD per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("LOAD \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("SAVE")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("SAVE per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("SAVE \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("RUN per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("MERGE \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("CHAIN \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("CHAIN MERGE \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
}

#[test]
fn immediate_files_and_range_errors_match_python() {
    let temp = tempfile::tempdir().unwrap();
    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();
    interp.process_immediate("10 PRINT 1").unwrap();
    interp.process_immediate("20 PRINT 2").unwrap();

    assert_eq!(
        interp
            .process_immediate("FILES per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("FILES \"..\"")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("CAT per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("LIST per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("LIST a-b")
            .unwrap_err()
            .display_for_basic(),
        "Invalid value type."
    );
    assert_eq!(
        interp
            .process_immediate("DELETE per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("RENUM per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("RENUM 10,per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("EDIT per")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("EDIT 999")
            .unwrap_err()
            .display_for_basic(),
        "Target line does not exist."
    );
}

#[test]
fn immediate_run_start_line_matches_python() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 1").unwrap();
    interp.process_immediate("20 PRINT 2").unwrap();
    interp.process_immediate("RUN 20").unwrap();
    assert_eq!(interp.take_output(), " 2\n");

    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 1").unwrap();
    assert_eq!(
        interp
            .process_immediate("RUN 99")
            .unwrap_err()
            .display_for_basic(),
        "Target line does not exist."
    );
}

#[test]
fn error_codes_and_messages_match_python_oracle_table() {
    let expected = [
        (
            1,
            ErrorCode::ImmediateCommand,
            "Instruction not allowed in a program.",
        ),
        (
            2,
            ErrorCode::NonImmediateCommand,
            "Instruction not allowed in immediate mode.",
        ),
        (3, ErrorCode::InvalidLineFormat, "Invalid line format."),
        (4, ErrorCode::InvalidValue, "Invalid value."),
        (5, ErrorCode::TypeMismatch, "Invalid value type."),
        (6, ErrorCode::DivisionByZero, "Division by zero."),
        (7, ErrorCode::Overflow, "Numeric overflow."),
        (8, ErrorCode::Undefined, "Undefined variable or function."),
        (9, ErrorCode::InvalidName, "Invalid name."),
        (10, ErrorCode::InvalidLineNumber, "Invalid line number."),
        (11, ErrorCode::InvalidTargetLine, "Invalid target line."),
        (
            12,
            ErrorCode::TargetLineNotFound,
            "Target line does not exist.",
        ),
        (
            13,
            ErrorCode::ReturnWithoutGosub,
            "RETURN without matching GOSUB.",
        ),
        (14, ErrorCode::ReturnNotFound, "Invalid return line."),
        (15, ErrorCode::Syntax, "Syntax error."),
        (16, ErrorCode::ForWithoutNext, "FOR without matching NEXT."),
        (17, ErrorCode::NextWithoutFor, "NEXT without matching FOR."),
        (
            18,
            ErrorCode::WhileWithoutWend,
            "WHILE without matching WEND.",
        ),
        (
            19,
            ErrorCode::WendWithoutWhile,
            "WEND without matching WHILE.",
        ),
        (20, ErrorCode::ResumeWithoutError, "RESUME without ERROR."),
        (
            21,
            ErrorCode::NumericExpression,
            "Expression must be numeric.",
        ),
        (22, ErrorCode::EvalError, "Error evaluating expression."),
        (23, ErrorCode::UnknownType, "Unknown data type."),
        (24, ErrorCode::DataExhausted, "No more DATA to read."),
        (25, ErrorCode::NoData, "The line has no DATA for RESTORE."),
        (26, ErrorCode::InvalidArgument, "Invalid argument."),
        (27, ErrorCode::NoDimension, "Empty dimension expression."),
        (28, ErrorCode::IndexError, "Error generating array indices."),
        (29, ErrorCode::UsingFormat, "Error in PRINT USING format."),
        (
            30,
            ErrorCode::VarNumberMismatch,
            "Number of inputs does not match number of variables.",
        ),
        (
            31,
            ErrorCode::ArgumentMismatch,
            "Incorrect number of arguments.",
        ),
        (32, ErrorCode::InvalidIndex, "Invalid index."),
        (33, ErrorCode::UndefinedIndex, "Undefined index."),
        (34, ErrorCode::IndexOutOfRange, "Index out of range."),
        (35, ErrorCode::OutOfBounds, "Out of range."),
        (36, ErrorCode::FunctionError, "Error evaluating function."),
        (
            37,
            ErrorCode::ForbiddenExpression,
            "Expression not allowed.",
        ),
        (
            38,
            ErrorCode::Unsupported,
            "Feature not supported in this environment.",
        ),
        (39, ErrorCode::MissingQuotes, "Missing quotes."),
        (40, ErrorCode::FileNotFound, "File not found."),
        (
            41,
            ErrorCode::KeyboardInterrupt,
            "Execution interrupted by user.",
        ),
        (
            42,
            ErrorCode::OnlyBasFiles,
            "Only names with extension '.bas' are allowed.",
        ),
        (
            43,
            ErrorCode::OnlyPngFiles,
            "Only names with extension '.png' are allowed.",
        ),
        (44, ErrorCode::HandlerError, "Error while handling errors."),
        (45, ErrorCode::MergeError, "Error merging files."),
        (
            46,
            ErrorCode::NoStoppedProgram,
            "There is no stopped program to continue.",
        ),
        (
            47,
            ErrorCode::FunctionForbidden,
            "Instruction not allowed inside a function.",
        ),
        (48, ErrorCode::FnEndWithoutDef, "Malformed function."),
        (49, ErrorCode::IfWithoutEndIf, "IF without matching END IF."),
        (50, ErrorCode::ElseWithoutIf, "ELSE without matching IF."),
        (51, ErrorCode::EndIfWithoutIf, "END IF without matching IF."),
        (
            52,
            ErrorCode::InvalidDimensions,
            "Invalid number of dimensions.",
        ),
        (
            53,
            ErrorCode::MatIdnDimension,
            "MAT IDN requires a two-dimensional square matrix.",
        ),
        (
            54,
            ErrorCode::SubroutineForbidden,
            "Instruction not allowed inside a subroutine.",
        ),
        (55, ErrorCode::SubEndWithoutDef, "Malformed subroutine."),
        (
            56,
            ErrorCode::LocalNotAtStart,
            "LOCAL must appear in the initial block of a function or subroutine.",
        ),
    ];
    for (number, code, message) in expected {
        assert_eq!(code.number(), number);
        assert_eq!(ErrorCode::from_number(number), Some(code));
        assert_eq!(code.message(), message);
    }
    assert_eq!(ErrorCode::from_number(0), None);
    assert_eq!(ErrorCode::from_number(57), None);
}

#[test]
fn immediate_and_program_only_command_errors_match_python() {
    let mut interp = Interpreter::new();
    let err = interp.process_immediate("ERROR 3").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "Instruction not allowed in immediate mode."
    );

    let mut interp = Interpreter::new();
    interp.process_immediate("10 LOAD \"missing.bas\"").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "Line 10. Instruction not allowed in a program."
    );
}

#[test]
fn error_statement_reports_python_messages() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 ERROR 40").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 10. File not found.");

    let mut interp = Interpreter::new();
    interp.process_immediate("10 ERROR 999").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 10. Error 999");
}

#[test]
fn error_statement_requires_integer_literal_like_python() {
    for (program, expected) in [
        ("10 ERROR", "Line 10. Syntax error."),
        ("10 ERROR 2.9", "Line 10. Invalid value."),
        ("10 ERROR \"A\"", "Line 10. Invalid value."),
        ("10 ERROR 1E309", "Line 10. Invalid value."),
    ] {
        let mut interp = Interpreter::new();
        interp.process_immediate(program).unwrap();
        let err = interp.process_immediate("RUN").unwrap_err();
        assert_eq!(err.display_for_basic(), expected, "{program}");
    }

    let mut interp = Interpreter::new();
    interp.process_immediate("10 ERROR -1").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 10. Error -1");
}

#[test]
fn on_error_clears_err_and_erl_when_handler_is_disabled() {
    let output = run_rust(
        r#"10 ON ERROR GOTO 100
20 ERROR 200
30 END
100 PRINT ERR;ERL
110 ON ERROR GOTO 0
120 PRINT ERR;ERL"#,
    );

    assert_eq!(output, " 200  20\n 0  0\n");
}

#[test]
fn disabling_error_handler_inside_handler_still_reports_nested_handler_error() {
    let mut interp = Interpreter::new();
    for line in r###"10 A$="##"
20 PRINT USING A$;12.34
30 ON ERROR GOTO 50
40 A==W
50 Z$ = ""
60 ON ERROR GOTO 0
70 PRINT USING W$;12.34
80 END"###
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }

    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(interp.take_output(), "12\n");
    assert_eq!(
        err.display_for_basic(),
        "Error in error handler: Error in PRINT USING format."
    );
}

#[test]
fn on_error_resume_next_clears_err_and_erl_after_skipped_error() {
    let output = run_rust(
        r#"10 ON ERROR RESUME NEXT
20 ERROR 200
30 PRINT ERR;ERL"#,
    );

    assert_eq!(output, " 0  0\n");
}

#[test]
fn error_flow_targets_must_be_literal_line_numbers() {
    assert_eq!(
        run_rust_error_code("10 ON ERROR GOTO 100+10\n20 END\n110 END"),
        ErrorCode::InvalidLineNumber
    );

    let mut interp = Interpreter::new();
    for line in r#"10 ON ERROR GOTO 100
20 ERROR 200
30 END
100 RESUME 200+10
210 END"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "Error in error handler: Invalid line number."
    );
}

#[test]
fn immediate_print_after_error_handler_end_is_not_skipped() {
    let mut interp = Interpreter::new();
    for line in r#"10 ON ERROR GOTO 30
20 GOTO 1000
30 PRINT "Error";ERR;"en la línea";ERL
40 END"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "Error 12 en la línea 20\n");

    interp.process_immediate("PRINT ERR").unwrap();
    assert_eq!(interp.take_output(), " 0\n");
    interp.process_immediate("PRINT ERL").unwrap();
    assert_eq!(interp.take_output(), " 0\n");

    interp.process_immediate(r#"PRINT "hola""#).unwrap();
    assert_eq!(interp.take_output(), "hola\n");

    interp.process_immediate(r#"PRINT "hola""#).unwrap();
    assert_eq!(interp.take_output(), "hola\n");

    let err = interp.process_immediate("PRINT 3/0").unwrap_err();
    assert_eq!(err.display_for_basic(), "Division by zero.");
    interp.process_immediate("PRINT ERR").unwrap();
    assert_eq!(interp.take_output(), " 0\n");
    interp.process_immediate("PRINT ERL").unwrap();
    assert_eq!(interp.take_output(), " 0\n");
}

#[test]
fn program_error_state_is_cleared_only_when_program_ends() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 3/0").unwrap();

    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 10. Division by zero.");
    interp.process_immediate("PRINT ERR").unwrap();
    assert_eq!(interp.take_output(), " 6\n");
    interp.process_immediate("PRINT ERL").unwrap();
    assert_eq!(interp.take_output(), " 10\n");

    let mut interp = Interpreter::new();
    for line in r#"10 ON ERROR GOTO 30
20 GOTO 1000
30 PRINT ERR;ERL
40 STOP
50 END"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), " 12  20\nLine 40. Program stopped.\n");
    interp.process_immediate("PRINT ERR").unwrap();
    assert_eq!(interp.take_output(), " 12\n");
    interp.process_immediate("PRINT ERL").unwrap();
    assert_eq!(interp.take_output(), " 20\n");

    interp.process_immediate("CONT").unwrap();
    interp.process_immediate("PRINT ERR").unwrap();
    assert_eq!(interp.take_output(), " 0\n");
    interp.process_immediate("PRINT ERL").unwrap();
    assert_eq!(interp.take_output(), " 0\n");
}

#[test]
fn console_normalization_completes_file_quotes_and_bas_extension() {
    assert_eq!(console::normalize_code("load\"demo"), "LOAD \"demo.bas\"");
    assert_eq!(
        console::normalize_code("10 print using\"0#\";8"),
        "10 PRINT USING \"0#\";8"
    );
    assert_eq!(
        console::normalize_code("10 print \"A\";using\"0#\";8"),
        "10 PRINT \"A\";USING \"0#\";8"
    );
    assert_eq!(
        console::normalize_code("10 print using(\"0#\");8"),
        "10 PRINT USING(\"0#\");8"
    );
    assert_eq!(
        console::normalize_code("10 print 1e3 'comment"),
        "10 PRINT 1E+3 'comment"
    );
    assert_eq!(
        console::normalize_code("10 print 1'comment"),
        "10 PRINT 1 'comment"
    );
    assert_eq!(
        console::normalize_code("10 LET You=124.4321"),
        "10 LET You=124.4321"
    );
    assert_eq!(
        console::normalize_code("10 PRINT 1.230000000000000"),
        "10 PRINT 1.23"
    );
    assert_eq!(console::normalize_code("'comment"), "'comment");
    assert_eq!(
        console::normalize_code("10 rem print if and mod"),
        "10 REM print if and mod"
    );
    assert_eq!(
        console::normalize_code("10 print 1:print 2"),
        "10 PRINT 1 : PRINT 2"
    );
    assert_eq!(
        console::normalize_code("10 print 0:if a then print 1:print 2 else print 3:print 4"),
        "10 PRINT 0 : IF a THEN PRINT 1:PRINT 2 ELSE PRINT 3:PRINT 4"
    );
    assert_eq!(
        console::normalize_code("10 if a then print 1 : print 2 else print 3 : print 4"),
        "10 IF a THEN PRINT 1:PRINT 2 ELSE PRINT 3:PRINT 4"
    );
    assert_eq!(
        console::normalize_code("10 print \"a:b\":print 2"),
        "10 PRINT \"a:b\" : PRINT 2"
    );
    assert_eq!(
        console::normalize_code("10 clg offscreen"),
        "10 CLG OFFSCREEN"
    );
    assert_eq!(
        console::normalize_code("20 smallfont opaque:bigfont transparent"),
        "20 SMALLFONT OPAQUE : BIGFONT TRANSPARENT"
    );
    assert_eq!(
        console::normalize_code("10 gprint\"hola\""),
        "10 GPRINT \"hola\""
    );
    assert_eq!(
        console::normalize_code("20 label\"rótulo\""),
        "20 LABEL \"rótulo\""
    );
    assert_eq!(console::normalize_code("10 print 1:"), "10 PRINT 1 :");
}

#[test]
fn console_highlight_matches_python_keyword_boundaries() {
    let highlighted = console::syntax_highlight("10 IF A AND B THEN CALL worker", true);
    assert!(highlighted.contains("\x1b[38;5;248mAND\x1b[0m"));
    assert!(!highlighted.contains("\x1b[1m\x1b[3m\x1b[97mAND\x1b[0m"));
    assert!(highlighted.contains("\x1b[1m\x1b[3m\x1b[97mWORKER\x1b[0m"));

    let highlighted = console::syntax_highlight("20 X=ABS(Y)+FNfoo(Y) MOD 2", true);
    assert!(highlighted.contains("\x1b[38;5;248mABS\x1b[0m"));
    assert!(highlighted.contains("\x1b[38;5;248mFNFOO\x1b[0m"));
    assert!(highlighted.contains("\x1b[38;5;248mMOD\x1b[0m"));

    let highlighted = console::syntax_highlight("30 PRINT -5, +.25", true);
    assert!(highlighted.contains("\x1b[38;5;214m-5\x1b[0m"));
    assert!(highlighted.contains("\x1b[38;5;214m+0.25\x1b[0m"));

    let highlighted = console::syntax_highlight("40 CLG OFFSCREEN : PRINT OFFSCREEN", true);
    assert_eq!(
        highlighted
            .matches("\x1b[1m\x1b[3m\x1b[97mOFFSCREEN\x1b[0m")
            .count(),
        2
    );
    assert!(!highlighted.contains("\x1b[1m\x1b[38;5;39mOFFSCREEN\x1b[0m"));

    let mut cases = HashMap::new();
    cases.insert("PERITA".to_string(), "perIta".to_string());
    assert_eq!(
        console::syntax_highlight_with_cases("40 REM PERITA", false, Some(&cases)),
        "40 REM PERITA"
    );
}

#[test]
fn offscreen_is_reserved_identifier_in_rust() {
    let mut interp = Interpreter::new();
    let err = interp.process_immediate("OFFSCREEN=1").unwrap_err();
    assert_eq!(err.code, ErrorCode::Undefined);

    let err = interp.process_immediate("PRINT OFFSCREEN").unwrap_err();
    assert_eq!(err.code, ErrorCode::Undefined);
}

#[test]
fn list_preserves_indentation_and_variable_canonical_case() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10   MiVariable=1").unwrap();
    interp.process_immediate("20     print mivariable").unwrap();
    interp
        .process_immediate("30 REM mivariable print if and mod")
        .unwrap();
    interp
        .process_immediate("40 print mivariable ' mivariable")
        .unwrap();
    interp
        .process_immediate("50 print 1:if mivariable then print 2:print 3")
        .unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "10   MiVariable=1\n20     PRINT MiVariable\n30 REM mivariable print if and mod\n40 PRINT MiVariable ' mivariable\n50 PRINT 1 : IF MiVariable THEN PRINT 2:PRINT 3\n"
    );

    let mut interp = Interpreter::new();
    interp.process_immediate("20 perIta=1").unwrap();
    interp.process_immediate("30 PRINT perIta").unwrap();
    interp.process_immediate("40 REM PERITA").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "20 perIta=1\n30 PRINT perIta\n40 REM PERITA\n"
    );
}

#[test]
fn list_preserves_decimal_literal_without_binary_tail() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 LET You=124.4321").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(interp.take_output(), "10 LET You=124.4321\n");
}

#[test]
fn for_next_and_arrays_baseline() {
    let output = run_rust(
        r#"10 DIM A(3)
20 FOR I=1 TO 3
30 A(I)=I*I
40 NEXT I
50 PRINT A(1);A(2);A(3)
60 END"#,
    );
    assert_eq!(output, " 1  4  9\n");
}

#[test]
fn input_accepts_string_array_elements() {
    let output = run_rust_cli(
        r#"10 DIM amigo$(2),telefono$(2)
20 FOR n=1 TO 2
30 INPUT "Nombre ";amigo$(n)
40 INPUT "Telefono ";telefono$(n)
50 NEXT
60 FOR n=1 TO 2
70 PRINT n;amigo$(n),telefono$(n)
80 NEXT"#,
        "Pedro\n111\nAna\n222\n",
    );
    assert_eq!(
        output,
        "Nombre ? Telefono ? Nombre ? Telefono ?  1 Pedro        111\n 2 Ana  222\n"
    );
}

#[test]
fn input_empty_numeric_value_defaults_to_zero() {
    let output = run_rust_cli(
        r#"10 INPUT A
20 PRINT A"#,
        "\n",
    );
    assert_eq!(output, "?  0\n");
}

#[test]
fn input_reports_mismatch_when_values_are_missing() {
    let (stdout, stderr) = run_rust_cli_error(
        r#"10 INPUT A,B
20 PRINT "BAD""#,
        "1\n",
    );
    assert_eq!(stdout, "? ");
    assert_eq!(
        stderr,
        "Line 10. Number of inputs does not match number of variables.\n"
    );
}

#[test]
fn for_parser_preserves_spaces_after_equals() {
    let output = run_rust(
        r#"10 FOR C= 0 TO 2
20 PRINT C;
30 NEXT
40 PRINT
50 END"#,
    );
    assert_eq!(output, " 0  1  2 \n");
}

#[test]
fn for_rejects_string_loop_variable() {
    assert_eq!(
        run_rust_error_code("10 FOR I$=1 TO 3\n20 PRINT I$\n30 NEXT"),
        ErrorCode::TypeMismatch
    );
}

#[test]
fn nested_exit_for_preserves_last_loop_values() {
    let output = run_rust(
        r#"10 FOR x=1 TO 10
11 IF x=5 THEN EXIT FOR
20 FOR y=1 TO 10
21 IF y=6 THEN EXIT FOR
30 FOR z=1 TO 10
31 IF z=7 THEN EXIT FOR
40 NEXT
50 NEXT
60 NEXT
70 PRINT x;y;z"#,
    );
    assert_eq!(output, " 5  6  7\n");
}

#[test]
fn swap_exchanges_scalars_and_array_elements() {
    let output = run_rust(
        r#"10 A=B=7
20 B=2
30 SWAP A,B
40 PRINT A,B
50 DIM A$(10)
60 A$(7)="adios":A$(8)="hola"
70 SWAP A$(7),A$(8)
80 PRINT A$(7),A$(8)
90 END"#,
    );
    assert_eq!(output, " 2       7\nhola    adios\n");
}

#[test]
fn print_commas_use_logical_zones_and_tab_is_absolute() {
    let output = run_rust(
        r#"10 PRINT "a","b",TAB(20),"c"
20 PRINT "a","b";TAB(20);"c"
30 PRINT (TAB(5));"x""#,
    );
    assert_eq!(
        output,
        "a       b               c\na       b          c\n    x\n"
    );
}

#[test]
fn zone_changes_print_comma_width() {
    let output = run_rust(
        r#"10 ZONE 4
20 PRINT "a","b","c"
30 PRINT "a","b";TAB(10);"c"
40 ZONE 1
50 PRINT "a","b","c""#,
    );
    assert_eq!(output, "a   b   c\na   b    c\na b c\n");
}

#[test]
fn swap_rejects_non_lvalues() {
    let mut interp = Interpreter::new();
    for line in r#"10 X=7
20 Y=2
30 SWAP X,Y+1
40 PRINT X,Y"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("SWAP with an expression target must fail");
    assert_eq!(err.display_for_basic(), "Line 30. Invalid argument.");
}

#[test]
fn swap_array_index_errors_follow_python_bounds() {
    let mut interp = Interpreter::new();
    for line in r#"10 X=7
20 SWAP A(17),A(18)
30 PRINT X"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("implicit arrays should keep default bounds");
    assert_eq!(err.display_for_basic(), "Line 20. Index out of range.");
}

#[test]
fn recursive_single_line_def_fn_reports_basic_error() {
    let mut interp = Interpreter::new();
    for line in r#"10 DEF FNR(X)=FNR(X-1)+1
20 PRINT FNR(3)"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("recursive single-line DEF FN must not overflow the Rust stack");
    assert_eq!(
        err.display_for_basic(),
        "Line 20. Instruction not allowed inside a function."
    );
}

#[test]
fn single_line_def_fn_rejects_array_arguments() {
    let mut interp = Interpreter::new();
    for line in r#"10 DIM A(2),B(2)
20 MAT A=2 : MAT B=3
30 DEF FNSUM(X,Y)=X(0)+Y(1)
40 PRINT FNSUM(A,B)"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("single-line DEF FN should not accept array parameters");
    assert_eq!(err.display_for_basic(), "Line 40. Invalid value type.");
}

#[test]
fn clear_resets_runtime_variables_without_clearing_program() {
    let output = run_rust(
        r#"10 X=5
20 PRINT X
30 CLEAR
40 PRINT X
50 END"#,
    );
    assert_eq!(output, " 5\n 0\n");
}

#[test]
fn clear_input_clears_input_without_clearing_variables() {
    let output = run_rust(
        r#"10 A=123
20 CLEAR INPUT
30 PRINT A
40 END"#,
    );
    assert_eq!(output, " 123\n");
}

#[test]
fn mat_base_controls_lbound_and_ubound_reports_dimensions() {
    let mut interp = Interpreter::new();
    for line in r#"10 DIM A(20)
20 PRINT LBOUND(A)
30 PRINT UBOUND(A)
40 MAT BASE 1
50 PRINT LBOUND(A)
60 PRINT UBOUND(A)
70 PRINT UBOUND(A,2)"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("invalid array dimension should fail");
    assert_eq!(interp.take_output(), " 0\n 20\n 1\n 20\n");
    assert_eq!(err.display_for_basic(), "Line 70. Index out of range.");
}

#[test]
fn row_col_and_base_are_reserved_like_python() {
    for name in ["ROW", "COL", "BASE", "ROW$", "COL$", "BASE$"] {
        let value = if name.ends_with('$') { r#""x""# } else { "1" };
        let mut interp = Interpreter::new();
        interp
            .process_immediate(&format!("10 LET {name}={value}"))
            .unwrap();
        let err = interp.process_immediate("RUN").unwrap_err();
        assert_eq!(
            err.display_for_basic(),
            "Line 10. Undefined variable or function."
        );

        let mut interp = Interpreter::new();
        interp
            .process_immediate(&format!("10 PRINT {name}"))
            .unwrap();
        let err = interp.process_immediate("RUN").unwrap_err();
        assert_eq!(
            err.display_for_basic(),
            "Line 10. Undefined variable or function."
        );
    }
}

#[test]
fn multiline_fn_can_return_matrix_from_mat_assignment() {
    let output = run_rust(
        r#"10 DEF FNM(X,Y)
20 MAT FNM=X+Y
40 FNEND
50 DIM A(1,1),B(1,1),C(1,1)
60 MAT A=CON
70 MAT B=2*A
80 MAT C=FNM(A,B)
90 MAT PRINT C"#,
    );
    assert_eq!(output, " 3   3\n 3   3\n");
}

#[test]
fn mat_print_using_comma_keeps_wide_matrix_columns() {
    let output = run_rust(
        r###"10 MAT BASE 1
20 DIM A(3,3),B(3,3)
30 MAT A=CON
40 MAT B=ZER
50 MAT PRINT USING "#";A;B,"###,
    );
    let wide_zero_row = format!("{:>22}{:>22}{:>22}", "0", "0", "0");
    let expected = format!(
        "1  1  1\n1  1  1\n1  1  1\n\n{0}\n{0}\n{0}\n",
        wide_zero_row
    );
    assert_eq!(output, expected);
}

#[test]
fn mat_multiplication_respects_mat_base_active_block() {
    let output = run_rust(
        r#"10 MAT BASE 1
20 DIM A(2,2),B(2,2),V(2),R(1,2)
30 A(1,1)=1:A(1,2)=0:A(2,1)=0:A(2,2)=1
40 B(1,1)=1:B(1,2)=0:B(2,1)=0:B(2,2)=1
50 A(1,0)=100:B(0,1)=100
60 MAT C=A*B
70 PRINT C(1,1);C(0,1);C(1,0)
80 V(1)=1:V(2)=0:R(1,1)=1:R(1,2)=0
90 V(0)=100:R(0,1)=100
100 MAT X=V*R
110 MAT Y=R*V
120 PRINT X(1,1);X(0,1);X(1,0)
130 PRINT Y(1,1);Y(0,1);Y(1,0)"#,
    );
    assert_eq!(output, " 1  0  0\n 1  0  0\n 1  0  0\n");
}

#[test]
fn multiline_fn_return_name_is_readable_after_assignment() {
    let output = run_rust(
        r#"10 DEF FNT(X)
20 FNT=X
30 FNT=FNT+1
40 FNEND
50 PRINT FNT(4)"#,
    );
    assert_eq!(output, " 5\n");
}

#[test]
fn multiline_subroutine_locals_and_array_references() {
    let output = run_rust(
        r#"10 A=1
20 DIM D(2),Z(2)
30 D(1)=4:Z(1)=1
40 DEF SUB WORK(T)
50 LOCAL A,D(3)
60 A=7
70 D(1)=9
80 T(1)=5
90 PRINT A
100 PRINT D(1)
110 SUBEND
120 CALL WORK(Z)
130 PRINT A
140 PRINT D(1)
150 PRINT Z(1)"#,
    );
    assert_eq!(output, " 7\n 9\n 1\n 4\n 5\n");
}

#[test]
fn multiline_subroutine_array_parameters_are_real_aliases() {
    let output = run_rust(
        r#"10 DEF SUB TOUCH(A,B)
20 A(0)=10
30 B(1)=20
40 SUBEND
50 DIM Z(1)
60 Z(0)=1:Z(1)=2
70 CALL TOUCH(Z,Z)
80 PRINT Z(0);Z(1)"#,
    );
    assert_eq!(output, " 10  20\n");

    let output = run_rust(
        r#"10 DEF SUB TOUCH(A,B)
20 A(0)=10
30 PRINT B(0)
40 SUBEND
50 DIM Z(1)
60 Z(0)=1:Z(1)=2
70 CALL TOUCH(Z,Z)"#,
    );
    assert_eq!(output, " 10\n");
}

#[test]
fn multiline_subroutine_array_aliases_share_global_name_and_nested_calls() {
    let output = run_rust(
        r#"10 DEF SUB TOUCH(A)
20 A(0)=10
30 Z(1)=20
40 SUBEND
50 DIM Z(1)
60 Z(0)=1:Z(1)=2
70 CALL TOUCH(Z)
80 PRINT Z(0);Z(1)"#,
    );
    assert_eq!(output, " 10  20\n");

    let output = run_rust(
        r#"10 DEF SUB INNER(B)
20 B(0)=8
30 SUBEND
40 DEF SUB OUTER(A)
50 CALL INNER(A)
60 SUBEND
70 DIM Z(0)
80 CALL OUTER(Z)
90 PRINT Z(0)"#,
    );
    assert_eq!(output, " 8\n");
}

#[test]
fn multiline_subroutine_redim_and_mat_modify_array_alias() {
    let output = run_rust(
        r#"10 DEF SUB WORK(A)
20 REDIM A(2)
30 A(2)=7
40 SUBEND
50 DIM Z(1)
60 CALL WORK(Z)
70 PRINT Z(2)"#,
    );
    assert_eq!(output, " 7\n");

    let output = run_rust(
        r#"10 DEF SUB WORK(A)
20 MAT A=CON
30 SUBEND
40 DIM Z(1)
50 CALL WORK(Z)
60 MAT PRINT Z"#,
    );
    assert_eq!(output, " 1\n 1\n");
}

#[test]
fn mat_string_arrays_concatenate_scalar_strings() {
    let output = run_rust(
        r#"10 DIM A$(1),B$(1)
20 A$(0)="A":A$(1)="B"
30 MAT B$=A$+"!"
40 MAT A$="<"+B$
50 PRINT A$(0);A$(1)"#,
    );
    assert_eq!(output, "<A!<B!\n");
}

#[test]
fn mat_string_arrays_reject_non_add_scalar_operations() {
    assert_eq!(
        run_rust_error_code("10 DIM A$(1),B$(1)\n20 MAT B$=A$*\"!\""),
        ErrorCode::ForbiddenExpression
    );
    assert_eq!(
        run_rust_error_code("10 DIM A$(1),B$(1)\n20 MAT B$=A$+1"),
        ErrorCode::TypeMismatch
    );
}

#[test]
fn mat_con_zer_idn_require_existing_array() {
    assert_eq!(run_rust_error_code("10 MAT A=CON"), ErrorCode::Undefined);
    assert_eq!(run_rust_error_code("10 MAT A=ZER"), ErrorCode::Undefined);
    assert_eq!(run_rust_error_code("10 MAT A=IDN"), ErrorCode::Undefined);
}

#[test]
fn mat_stat_functions_update_context_values() {
    let output = run_rust(
        r#"10 MAT BASE 1
20 DIM A(3,3),V1(3),V2(3),B(3,5)
30 DATA -3,2,3,5,-3,5,2,5,-1
40 DATA 2,1,3,1,4,2
50 MAT READ A
60 MAT READ V1,V2
70 IF ABSUM(A)<>29 THEN PRINT "E1"
80 IF AMAX(A)<>5 OR AMAXCOL<>1 OR AMAXROW<>2 THEN PRINT "E2"
90 IF AMIN(A)<>-3 OR AMINCOL<>1 OR AMINROW<>1 THEN PRINT "E3"
100 IF CNORM(A)<>10 OR CNORMCOL<>1 THEN PRINT "E4"
110 IF DOT(V1,V2)<>12 THEN PRINT "E5"
120 IF ROUND(FNORM(A),2)<>10.54 THEN PRINT "E6"
130 IF LBND(A)<>1 THEN PRINT "E7"
140 IF MAXAB(A)<>5 OR MAXABCOL<>1 OR MAXABROW<>2 THEN PRINT "E8"
150 IF RNORM(A)<>13 OR RNORMROW<>2 THEN PRINT "E9"
160 IF SUM(A)<>15 THEN PRINT "E10"
170 IF UBND(B,1.4)<>3 OR UBND(B,1.6)<>5 THEN PRINT "E11"
180 PRINT "OK""#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn mat_input_fills_matrix_from_multiple_tokens_per_line() {
    let output = run_rust_cli(
        r#"10 MAT BASE 1 : DIM aa(2,2)
20 MAT INPUT aa
30 MAT PRINT aa"#,
        "1,2\n3\n4,5,6\n",
    );
    assert_eq!(output, "aa(1,1)? aa(2,1)? aa(2,2)?  1   2\n 3   4\n");
}

#[test]
fn print_using_suppresses_leading_thousands_separators() {
    let output = run_rust(
        r#"10 PRINT USING "Monthly payment:    ###,###,###.##"; 536.82
20 PRINT USING "Total paid:         ###,###,###.##"; 193255.78
30 PRINT USING "Total interest:     ###,###,###.##"; 93255.78"#,
    );
    assert_eq!(
        output,
        "Monthly payment:            536.82\nTotal paid:             193,255.78\nTotal interest:          93,255.78\n"
    );
}

#[test]
fn print_using_leading_comma_uses_european_separators() {
    let output = run_rust(
        r#"10 X=123456.7892
20 PRINT USING ",#,###,###,###.##"; X
30 PRINT USING ",#,###,###.##"; 1234567.23
40 PRINT USING ",0.00^^^^"; 12345"#,
    );
    assert_eq!(output, "      123.456,79\n1.234.567,23\n1,23E+04\n");
}

#[test]
fn print_using_clause_can_follow_regular_print_items() {
    let output = run_rust(
        r#"10 PRINT "Precio";USING "0#";8;" EUR"
20 PRINT "A",USING "0#";9
30 PRINT USING"0#";8
40 PRINT "B";USING"0#";7
50 PRINT USING "0#";8,9"#,
    );
    assert_eq!(output, "Precio08 EUR\nA       09\n08\nB07\n08      09\n");
}

#[test]
fn print_using_template_still_requires_semicolon() {
    assert_eq!(
        run_rust_error_code(r#"10 PRINT USING "0#",8"#),
        ErrorCode::Syntax
    );
    assert_eq!(
        run_rust_error_code(r#"10 PRINT USING("0#");8"#),
        ErrorCode::Syntax
    );
}

#[test]
fn print_using_scientific_formats_general_masks() {
    let output = run_rust(
        r###"10 PRINT USING "#.##^^^^"; 123245435234
20 PRINT USING "##.##^^^^"; 12345
30 PRINT USING "#.##^^^^"; 999.9
40 PRINT USING "0.00^^^^^"; 1E308"###,
    );
    assert_eq!(output, "1.23E+11\n12.35E+03\n1.00E+03\n1.00E+308\n");
}

#[test]
fn dec_uses_the_same_formatter_as_print_using() {
    let output = run_rust(
        r####"10 PRINT USING "###.#";68.123
20 PRINT USING "0##.#";68.123
30 PRINT USING "#0#.#";68.123
40 PRINT USING "##0.#";68.123
50 PRINT DEC$(68.123,"###.0")
60 PRINT DEC$(68.123,"0##.0")
70 PRINT DEC$(68.123,"#0#.0")
80 PRINT DEC$(68.123,"##0.0")
90 PRINT DEC$(-1.23,"+#.##")
100 PRINT DEC$(1234567.23,",#,###,###.##")
110 PRINT DEC$(12345,"0.00^^^^")"####,
    );
    assert_eq!(
        output,
        " 68.1\n068.1\n068.1\n068.1\n 68.1\n068.1\n068.1\n068.1\n-1.23\n1.234.567,23\n1.23E+04\n"
    );
}

#[test]
fn tab_positions_to_columns_across_print_statements() {
    let output = run_rust(
        r###"10 PRINT "Month"; TAB(17); "Payment"; TAB(33); "Interest"; TAB(49); "Principal"; TAB(68); "Balance"
20 PRINT USING "##"; 1;
30 PRINT TAB(8);
40 PRINT USING "#,###,###,###.##"; 536.82;
50 PRINT TAB(25);
60 PRINT USING "#,###,###,###.##"; 416.67;
70 PRINT TAB(42);
80 PRINT USING "#,###,###,###.##"; 120.15;
90 PRINT TAB(59);
100 PRINT USING "#,###,###,###.##"; 99879.85"###,
    );
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(
        lines[0],
        "Month           Payment         Interest        Principal          Balance"
    );
    assert_eq!(
        lines[1],
        " 1               536.82           416.67           120.15        99,879.85"
    );
    assert!(lines.iter().all(|line| line.len() <= 80));
}

#[test]
fn degree_mode_affects_trig_functions() {
    let output = run_rust(
        r#"10 DEG
20 A=SIN(30):B=COS(60):PRINT A;B
30 PRINT SIN(30);COS(60)
40 RAD
50 PRINT ROUND(SIN(PI/6),6)
60 END"#,
    );
    assert_eq!(output, " 0.5  0.5\n 0.5  0.5\n 0.5\n");
}

#[test]
fn graphics_screen_string_baseline() {
    let output = run_rust(
        r##"10 SCREEN
20 MODE 640
30 PAPER 0 : CLG
40 PLOT 1,1,2
50 A$=SCREEN$
60 PRINT LEFT$(A$,6)
70 PRINT TEST(1,1)
80 END"##,
    );
    assert_eq!(output, "640x48\n 16711680\n");
}

#[test]
fn testchr_reads_recognizable_text_cells_without_moving_cursor() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 LOCATE 5,7 : GPRINT "Z";
30 PRINT TESTCHR$(5,7)
40 PRINT HPOS;VPOS
50 LOCATE 5,7
60 PRINT TESTCHR$
70 PRINT HPOS;VPOS
80 END"#,
    );
    assert_eq!(output, "Z\n 6  7\nZ\n 5  7\n");
}

#[test]
fn gprint_uses_print_lists_using_zones_and_graphics_newlines() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG : ZONE 22
20 LOCATE 3,4 : GPRINT USING "0#";7;
30 PRINT HPOS;VPOS
40 LOCATE 0,5 : GPRINT "A","B";
50 PRINT HPOS;VPOS
60 GPRINT
70 PRINT HPOS;VPOS
80 LOCATE -2,7 : GPRINT TAB(4);"X";
90 PRINT HPOS;VPOS
100 END"#,
    );
    assert_eq!(output, " 5  4\n 23  5\n 0  6\n 4  7\n");
}

#[test]
fn removed_disp_commands_report_syntax_error() {
    let mut interp = Interpreter::new();
    for statement in [r#"DISP "old""#, r#"GDISP "old""#] {
        let err = interp.process_immediate(statement).unwrap_err();
        assert_eq!(err.code, ErrorCode::Syntax, "{statement}");
    }
}

#[test]
fn text_background_defaults_to_opaque_and_font_modes_persist() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 PLOT 0,479,2 : LOCATE 0,0 : GPRINT " ";
30 PRINT TEST(0,479)
40 BIGFONT OPAQUE : SMALLFONT : PLOT 0,479,2 : LOCATE 0,0 : GPRINT " ";
50 PRINT TEST(0,479)
60 BIGFONT TRANSPARENT : SMALLFONT : PLOT 0,479,2 : LOCATE 0,0 : GPRINT " ";
70 PRINT TEST(0,479)
80 BIGFONT TRANSPARENT : SCREEN : PLOT 0,479,2 : LOCATE 0,0 : GPRINT " ";
90 PRINT TEST(0,479)
100 END"#,
    );
    assert_eq!(output, " 0\n 0\n 16711680\n 0\n");
}

#[test]
fn transparent_gprint_composes_glyphs_in_the_same_text_cell() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG : SMALLFONT TRANSPARENT
20 LOCATE 0,0 : GPRINT "A";
30 LOCATE 0,0 : GPRINT "_";
40 PRINT "["+TESTCHR$(0,0)+"]"
50 END"#,
    );
    assert_eq!(output, "[]\n");
}

#[test]
fn new_restores_smallfont_opaque() {
    let mut interp = Interpreter::new();
    interp.process_immediate("BIGFONT TRANSPARENT").unwrap();
    interp.process_immediate("NEW").unwrap();
    interp.process_immediate("PLOT 0,479,2").unwrap();
    interp.process_immediate("LOCATE 0,0").unwrap();
    interp.process_immediate("GPRINT \" \";").unwrap();
    interp.process_immediate("PRINT TEST(0,479)").unwrap();
    assert_eq!(interp.take_output(), " 0\n");
}

#[test]
fn testchr_reads_pixels_after_screen_restore() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 GPRINT "Extremo superior izquierdo"
30 A$=SCREEN$
40 SCREEN
50 SCREEN A$
60 PRINT TESTCHR$(0,0)
70 END"#,
    );
    assert_eq!(output, "E\n");
}

#[test]
fn testchr_returns_empty_for_modified_cell() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 GPRINT "E"
30 PLOT 0,479,2
40 PRINT "["+TESTCHR$(0,0)+"]"
50 END"#,
    );
    assert_eq!(output, "[]\n");
}

#[test]
fn clg_offscreen_clears_graphics_buffer_without_explicit_frame() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 7 : CLG
20 PAPER 0 : CLG OFFSCREEN
30 PRINT TEST(0,0)
40 END"#,
    );
    assert_eq!(output, " 0\n");
}

#[test]
fn string_fast_paths_preserve_basic_results() {
    let output = run_rust(
        r#"10 A$="HOLA,MUNDO"
20 PRINT LEN(A$)
30 PRINT LEFT$(A$,4)
40 PRINT RIGHT$(A$,5)
50 PRINT MID$(A$,6,5)
60 PRINT INSTR(A$,",")
70 PRINT A$[2]
80 MID$(A$,6,5)="BASIC"
90 PRINT A$
100 END"#,
    );
    assert_eq!(output, " 10\nHOLA\nMUNDO\nMUNDO\n 5\nO\nHOLA,BASIC\n");
}

#[test]
fn version_string_matches_python_language_version() {
    let output = run_rust(
        r#"10 PRINT VERSION$
20 END"#,
    );
    assert_eq!(output, format!("{}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn graphics_mask_consumes_low_bit_first() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 MASK 170
30 MOVE 0,0 : DRAW 15,0,2
40 MASK
50 PRINT SPRITE$(0,0,15,0)
60 END"#,
    );
    assert_eq!(output, format!("16x1:{}\n", "000000ff0000".repeat(8)));
}

#[test]
fn scale_parameter_functions_report_physical_scale_by_default() {
    let output = run_rust(
        r#"10 IF XMIN<>0 THEN PRINT "BAD XMIN":END
20 IF XMAX<>WIDTH-1 THEN PRINT "BAD XMAX":END
30 IF YMIN<>0 THEN PRINT "BAD YMIN":END
40 IF YMAX<>HEIGHT-1 THEN PRINT "BAD YMAX":END
50 IF BORDER<>0 THEN PRINT "BAD BORDER":END
60 PRINT "OK"
70 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn scale_parameter_functions_report_active_scale() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640
20 SCALE -PI,PI,-1,1,20
30 IF ROUND(XMIN,6)<>-3.141593 THEN PRINT "BAD XMIN":END
40 IF ROUND(XMAX,6)<>3.141593 THEN PRINT "BAD XMAX":END
50 IF YMIN<>-1 THEN PRINT "BAD YMIN":END
60 IF YMAX<>1 THEN PRINT "BAD YMAX":END
70 IF BORDER<>20 THEN PRINT "BAD BORDER":END
80 PRINT "OK"
90 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn degree_radian_conversion_functions_are_angle_mode_independent() {
    let output = run_rust(
        r#"10 IF ROUND(RTD(PI),6)<>180 THEN PRINT "BAD RTD":END
20 IF ROUND(DTR(180),6)<>ROUND(PI,6) THEN PRINT "BAD DTR":END
30 DEG
40 IF ROUND(RTD(PI),6)<>180 THEN PRINT "BAD DEG RTD":END
50 IF ROUND(DTR(180),6)<>ROUND(PI,6) THEN PRINT "BAD DEG DTR":END
60 PRINT "OK"
70 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn graphics_triangle_axis_and_graph_commands_run() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE -1,1,-1,1,10
30 CROSSAT 0,0
40 XAXIS 0.5
50 YAXIS 0.5
60 GRAPH X*X,0.1
70 TRIANGLE -.5,-.5,.5,-.5,0,.5,2
80 FTRIANGLE -.25,-.25,.25,-.25,0,.25,3
90 PRINT "OK"
100 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn cached_filled_graphics_commands_preserve_pixels() {
    let output = run_rust(
        r#"10 CLG
20 FRECTANGLE 10,10,30,30,66051
30 IF TEST(20,20)<>66051 THEN PRINT "BAD RECT":END
40 FTRIANGLE 40,10,80,10,60,40,263430
50 IF TEST(60,20)<>263430 THEN PRINT "BAD TRIANGLE":END
60 FCIRCLE 110,20,12,460809
70 IF TEST(110,20)<>460809 THEN PRINT "BAD CIRCLE":END
80 RECTANGLE 140,5,180,35,1:MOVE 160,20:INK 658188:FILL
90 IF TEST(160,20)<>658188 THEN PRINT "BAD FILL":END
100 PRINT "OK""#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn mouse_commands_and_functions_are_deterministic_headless() {
    let output = run_rust(
        r#"10 SCREEN
20 ON MOUSE LEFTDOWN GOSUB 100
30 MOUSE 0
40 PRINT MOUSEX;MOUSEY;MOUSELEFT;MOUSERIGHT;MOUSEEVENT$
50 END
100 RETURN"#,
    );
    assert_eq!(output, " 0  0  0  0 \n");
}

#[test]
fn graphics_runtime_errors_include_program_line() {
    let mut interp = Interpreter::new();
    for line in r#"10 SCREEN : CLG
20 COLMODE 2
30 SPRITE "1x1:00ff00",10,10,0,0"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("non-positive explicit sprite id must fail");
    assert_eq!(err.display_for_basic(), "Line 30. Invalid argument.");
}

#[test]
fn frame_fps_extension_accepts_one_positive_argument() {
    let output = run_rust(
        r#"10 SCREEN
20 FRAME
30 FRAME 60
40 PRINT "OK"
50 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn frame_fps_extension_rejects_non_positive_rate() {
    let mut interp = Interpreter::new();
    for line in r#"10 SCREEN
20 FRAME 0"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 20. Invalid argument.");
}

#[test]
fn frame_fps_extension_rejects_extra_arguments() {
    let mut interp = Interpreter::new();
    let err = interp.process_immediate("FRAME 30,60").unwrap_err();
    assert_eq!(err.display_for_basic(), "Incorrect number of arguments.");
}

#[test]
fn on_gosub_selects_one_based_target_and_returns() {
    let output = run_rust(
        r#"10 S=1 : GOSUB 100
20 S=2 : GOSUB 100
30 PRINT A
40 END
100 ON S GOSUB 200,300
110 RETURN
200 A=A+10
210 RETURN
300 A=A+100
310 RETURN"#,
    );
    assert_eq!(output, " 110\n");
}

#[test]
fn gosub_and_on_targets_must_be_literal_line_numbers() {
    assert_eq!(
        run_rust_error_code("10 ON 1 GOTO 100+10\n110 END"),
        ErrorCode::InvalidLineNumber
    );
    assert_eq!(
        run_rust_error_code("10 ON 1 GOSUB 100+10\n110 RETURN"),
        ErrorCode::InvalidLineNumber
    );
    assert_eq!(
        run_rust_error_code("10 ON MOUSE LEFTDOWN GOSUB 100+10\n110 RETURN"),
        ErrorCode::InvalidLineNumber
    );
    assert_eq!(
        run_rust_error_code("10 IF -1 THEN GOSUB 100+10: PRINT \"BAD\"\n110 RETURN"),
        ErrorCode::InvalidLineNumber
    );
}

#[test]
fn if_then_colon_else_executes_only_selected_branch() {
    let output = run_rust(
        r#"10 D=12 : F=10
20 IF D>F THEN D=D-F:K=1 ELSE K=0
30 PRINT D;K
40 D=3
50 IF D>F THEN D=D-F:K=1 ELSE K=0
60 PRINT D;K
70 END"#,
    );
    assert_eq!(output, " 2  1\n 3  0\n");
}

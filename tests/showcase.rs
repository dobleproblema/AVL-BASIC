use avl_basic::{ErrorCode, Interpreter};
use std::path::PathBuf;

fn repo_interpreter() -> Interpreter {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = root.clone();
    interpreter.current_dir = root;
    interpreter
}

#[test]
fn tour_and_samples_render_the_embedded_catalog() {
    let mut interpreter = repo_interpreter();

    interpreter.process_immediate("tour").unwrap();
    let tour = interpreter.take_output();
    assert!(tour.starts_with("AVL BASIC TOUR - 20 highlights\n"));
    assert_eq!(
        tour.lines()
            .filter(|line| line.trim_start().starts_with("RUN \"/samples/"))
            .count(),
        20
    );
    assert!(!tour.contains("The RUN commands use /samples"));
    assert!(tour.contains("Visual gallery and annotated catalog: samples/README.md\n"));
    assert!(!tour.contains("index.html"));

    interpreter.process_immediate("samples").unwrap();
    let samples = interpreter.take_output();
    assert!(samples.starts_with("AVL BASIC SAMPLES - 115 programs\n"));
    assert_eq!(
        samples
            .lines()
            .filter(|line| line.starts_with("  ") && line.contains(".bas - "))
            .count(),
        115
    );

    for line in tour.lines().chain(samples.lines()) {
        assert!(line.chars().count() <= 80, "line is too wide: {line}");
    }
}

#[test]
fn showcase_commands_reject_arguments() {
    for command in ["TOUR 1", "SAMPLES games"] {
        let mut interpreter = repo_interpreter();
        let error = interpreter.process_immediate(command).unwrap_err();
        assert_eq!(error.code, ErrorCode::Syntax, "{command}");
    }
}

#[test]
fn showcase_commands_are_forbidden_inside_programs() {
    for command in ["TOUR", "SAMPLES"] {
        let mut interpreter = repo_interpreter();
        interpreter
            .process_immediate(&format!("10 {command}"))
            .unwrap();
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::ImmediateCommand, "{command}");
    }
}

#[test]
fn showcase_command_names_remain_valid_identifiers() {
    let mut interpreter = repo_interpreter();

    interpreter.process_immediate("SAMPLES=7").unwrap();
    interpreter.process_immediate("TOUR=9").unwrap();
    interpreter.process_immediate("PRINT SAMPLES;TOUR").unwrap();

    assert_eq!(interpreter.take_output(), " 7  9\n");

    interpreter.process_immediate("SAMPLES = 8").unwrap();
    interpreter.process_immediate("TOUR = 10").unwrap();
    interpreter.process_immediate("PRINT SAMPLES;TOUR").unwrap();
    assert_eq!(interpreter.take_output(), " 8  10\n");

    interpreter.process_immediate("10 SAMPLES = 11").unwrap();
    interpreter.process_immediate("20 TOUR = 13").unwrap();
    interpreter
        .process_immediate("30 PRINT SAMPLES;TOUR")
        .unwrap();
    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(interpreter.take_output(), " 11  13\n");
}

#[test]
fn showcase_commands_preserve_program_directory_and_continuation() {
    for command in ["TOUR", "SAMPLES"] {
        let mut interpreter = repo_interpreter();
        let original_dir = interpreter.current_dir.clone();
        interpreter
            .process_immediate("10 PRINT \"BEFORE\"")
            .unwrap();
        interpreter.process_immediate("20 STOP").unwrap();
        interpreter
            .process_immediate("30 PRINT \"RESUMED\"")
            .unwrap();

        interpreter.process_immediate("RUN").unwrap();
        assert_eq!(
            interpreter.take_output(),
            "BEFORE\nLine 20. Program stopped.\n",
            "{command}"
        );

        interpreter.process_immediate(command).unwrap();
        let showcase = interpreter.take_output();
        assert!(showcase.starts_with("AVL BASIC "), "{command}");
        assert_eq!(interpreter.current_dir, original_dir, "{command}");

        interpreter.process_immediate("LIST").unwrap();
        assert_eq!(
            interpreter.take_output(),
            "10 PRINT \"BEFORE\"\n20 STOP\n30 PRINT \"RESUMED\"\n",
            "{command}"
        );

        interpreter.process_immediate("CONT").unwrap();
        assert_eq!(interpreter.take_output(), "RESUMED\n", "{command}");
    }
}

#[test]
fn banner_points_to_the_showcase_commands() {
    let mut interpreter = repo_interpreter();

    interpreter.print_banner();

    assert!(interpreter.take_output().contains(
        "Type HELP <topic> for syntax, TOUR for highlights, or SAMPLES for the catalog.\n"
    ));
}

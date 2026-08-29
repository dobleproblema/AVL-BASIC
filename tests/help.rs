use avl_basic::{ErrorCode, Interpreter};

#[test]
fn help_right_dollar_is_brief_and_complete() {
    let mut interpreter = Interpreter::new();

    interpreter.process_immediate("help right$").unwrap();

    assert_eq!(
        interpreter.take_output(),
        concat!(
            "RIGHT$(text$, length)\n",
            "  text$: string expression\n",
            "  length: nonnegative character count\n",
            "Returns the rightmost characters; zero returns an empty string.\n",
            "Related: LEFT$, MID$\n",
        )
    );
}

#[test]
fn help_fill_lists_the_cursor_color_form() {
    let mut interpreter = Interpreter::new();

    interpreter.process_immediate("HELP FILL").unwrap();

    assert!(interpreter.take_output().contains("FILL color\n"));
}

#[test]
fn help_covers_optional_routine_parameters_local_arrays_and_exact_radix_widths() {
    let mut interpreter = Interpreter::new();

    interpreter.process_immediate("HELP DEF FN").unwrap();
    assert!(interpreter
        .take_output()
        .contains("DEF FNname[(parameter,...)]"));

    interpreter.process_immediate("HELP LOCAL").unwrap();
    assert!(interpreter
        .take_output()
        .contains("LOCAL name[(bound1[,bound2...])]"));

    interpreter.process_immediate("HELP BIN$").unwrap();
    assert!(interpreter
        .take_output()
        .contains("width: optional exact bit width, truncating higher bits"));
}

#[test]
fn help_lookup_ignores_case_and_normalizes_spaces_and_aliases() {
    let mut canonical = Interpreter::new();
    canonical.process_immediate("HELP ELSEIF").unwrap();

    let mut variant = Interpreter::new();
    variant.process_immediate("hElP   else    if").unwrap();
    assert_eq!(variant.take_output(), canonical.take_output());

    let mut files = Interpreter::new();
    files.process_immediate("HELP FILES").unwrap();
    let mut alias = Interpreter::new();
    alias.process_immediate("HELP cat").unwrap();
    assert_eq!(alias.take_output(), files.take_output());

    let mut rem = Interpreter::new();
    rem.process_immediate("HELP REM").unwrap();
    let mut quote_alias = Interpreter::new();
    quote_alias.process_immediate("HELP '").unwrap();
    assert_eq!(quote_alias.take_output(), rem.take_output());
}

#[test]
fn bare_and_unknown_help_topics_have_short_guidance() {
    let mut interpreter = Interpreter::new();
    interpreter.process_immediate("HELP").unwrap();
    assert_eq!(
        interpreter.take_output(),
        concat!(
            "HELP [topic]\n",
            "Shows syntax, parameters and a short description.\n",
        )
    );

    interpreter
        .process_immediate("HELP DOES-NOT-EXIST")
        .unwrap();
    let output = interpreter.take_output();
    assert!(output.starts_with("No HELP entry for DOES-NOT-EXIST\n"));
    assert!(output.contains("Use HELP with an instruction or function name.\n"));
}

#[test]
fn help_name_remains_a_valid_variable() {
    let mut interpreter = Interpreter::new();

    interpreter.process_immediate("HELP=1").unwrap();
    interpreter.process_immediate("HELP = HELP + 1").unwrap();
    interpreter.process_immediate("PRINT HELP").unwrap();
    assert_eq!(interpreter.take_output(), " 2\n");

    interpreter.process_immediate("10 HELP = 7").unwrap();
    interpreter.process_immediate("20 PRINT HELP").unwrap();
    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
}

#[test]
fn help_is_forbidden_inside_a_program() {
    for source in ["10 HELP", "10 HELP RIGHT$"] {
        let mut interpreter = Interpreter::new();
        interpreter.process_immediate(source).unwrap();
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::ImmediateCommand, "{source}");
    }
}

#[test]
fn help_preserves_a_stopped_program_continuation() {
    let mut interpreter = Interpreter::new();
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
        "BEFORE\nLine 20. Program stopped.\n"
    );

    interpreter.process_immediate("HELP RIGHT$").unwrap();
    assert!(interpreter
        .take_output()
        .starts_with("RIGHT$(text$, length)\n"));

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "RESUMED\n");
}

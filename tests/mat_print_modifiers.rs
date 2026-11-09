use avl_basic::expr::eval_expression;
use avl_basic::{ErrorCode, Interpreter, Value};

fn command(interpreter: &mut Interpreter, source: &str) {
    interpreter.process_immediate(source).unwrap();
}

fn number(interpreter: &mut Interpreter, source: &str) -> f64 {
    eval_expression(interpreter, source)
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn using_can_change_between_arrays_without_extra_blank_lines() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(0,1),B(0,1),C(0,1):A(0,0)=1.5:A(0,1)=2.5:MAT B=A:MAT C=A",
    );
    command(
        &mut interpreter,
        r#"MAT PRINT A;USING "0.0";B;USING "0.00";C"#,
    );
    assert_eq!(
        interpreter.take_output(),
        " 1.5   2.5\n\n1.5  2.5\n\n1.50  2.50\n"
    );
}

#[test]
fn mat_using_keeps_its_historical_expression_grammar_and_masks() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, r#"DIM A(0):MAT A=8:F$="0#""#);
    for source in [r#"MAT PRINT USING"0#";A"#, "MAT PRINT USING(F$);A"] {
        command(&mut interpreter, source);
        assert_eq!(interpreter.take_output(), "08\n", "{source}");
    }
    for source in [r#"MAT PRINT A;USING"0#";A"#, "MAT PRINT A;USING(F$);A"] {
        command(&mut interpreter, source);
        assert_eq!(interpreter.take_output(), " 8\n\n08\n", "{source}");
    }
    for (format, expected) in [("", "8\n"), ("label", "label8\n")] {
        command(&mut interpreter, &format!("MAT PRINT USING \"{format}\";A"));
        assert_eq!(interpreter.take_output(), expected);
        command(
            &mut interpreter,
            &format!("MAT PRINT A;USING \"{format}\";A"),
        );
        assert_eq!(interpreter.take_output(), format!(" 8\n\n{expected}"));
    }
    command(
        &mut interpreter,
        "DIM USINGvalues(0):MAT USINGvalues=9:MAT PRINT USINGvalues",
    );
    assert_eq!(interpreter.take_output(), " 9\n");
}

#[test]
fn color_items_preserve_orientation_base_text_and_wide_fields_in_plain_capture() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        let bound = base + 1;
        command(
            &mut interpreter,
            &format!(
                "MAT BASE {base}:DIM A({bound},{bound}),V({bound}),T$({bound}):MAT A=999:MAT V=999:MAT T$=\"skip\""
            ),
        );
        command(
            &mut interpreter,
            &format!(
                "A({base},{base})=1:A({base},{bound})=2:A({bound},{base})=3:A({bound},{bound})=4:V({base})=5:V({bound})=6:T$({base})=\"a\":T$({bound})=\"b\""
            ),
        );
        command(
            &mut interpreter,
            r#"MAT PRINT COLOR(2,4);USING "0#";ROW A;COLOR(1),COL A;COLOR();COL V;COLOR("white");T$,"#,
        );
        assert_eq!(
            interpreter.take_output(),
            format!(
                "01  02\n03  04\n\n01  03\n02  04\n\n05  06\n\n{:>22}\n{:>22}\n",
                "a", "b"
            ),
            "MAT BASE {base}"
        );
    }
}

#[test]
fn modifier_commas_do_not_advance_zones_or_select_array_width() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(0,1):A(0,0)=1:A(0,1)=2");
    command(
        &mut interpreter,
        r#"MAT PRINT USING "0#";COLOR(2),A;COLOR(),A,"#,
    );
    assert_eq!(
        interpreter.take_output(),
        format!("01  02\n\n{:>22}{:>22}\n", "01", "02")
    );
    for source in [
        "MAT PRINT",
        "MAT PRINT COLOR(2),",
        r#"MAT PRINT COLOR(2,4);USING "0#";COLOR()"#,
    ] {
        command(&mut interpreter, source);
        assert!(interpreter.take_output().is_empty(), "{source}");
    }
    command(&mut interpreter, "MAT PRINT A;COLOR(2),");
    assert_eq!(interpreter.take_output(), " 1   2\n");
}

#[test]
fn empty_arrays_keep_the_existing_separation_between_array_items() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 1:DIM E(0),A(1):MAT A=8");
    command(
        &mut interpreter,
        r#"MAT PRINT COLOR(2);E;USING "0#";COLOR(),A"#,
    );
    assert_eq!(interpreter.take_output(), "\n08\n");
    command(&mut interpreter, r#"MAT PRINT USING "0#";COLOR();E"#);
    assert!(interpreter.take_output().is_empty());
}

#[test]
fn modifiers_evaluate_once_in_list_order_and_foreground_before_background() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 DIM A(2):MAT A=8:ORDER$=\"\":CALLS=0:END\n\
         100 DEF FNARG(N,V)\n110 ORDER$=ORDER$+CHR$(64+N):CALLS=CALLS+1\n120 FNARG=V\n130 FNEND\n\
         200 DEF FNFMT$()\n210 ORDER$=ORDER$+\"F\":CALLS=CALLS+1\n220 FNFMT$=\"0#\"\n230 FNEND",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    command(
        &mut interpreter,
        "MAT PRINT COLOR(FNARG(1,2),FNARG(2,4));USING FNFMT$();COL A;COLOR(FNARG(3,1));COL A;COLOR()",
    );
    assert_eq!(interpreter.take_output(), "08  08  08\n\n08  08  08\n");
    assert_eq!(number(&mut interpreter, "CALLS"), 4.0);
    assert_eq!(
        eval_expression(&mut interpreter, "ORDER$").unwrap(),
        Value::string("ABFC")
    );

    for (arguments, order, calls) in [
        ("FNARG(1,-1),FNARG(2,4)", "A", 1.0),
        ("FNARG(1,2),FNARG(2,-1)", "AB", 2.0),
    ] {
        command(&mut interpreter, "ORDER$=\"\":CALLS=0");
        assert_eq!(
            interpreter
                .process_immediate(&format!("MAT PRINT COLOR({arguments});A"))
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
        assert!(interpreter.take_output().is_empty());
        assert_eq!(number(&mut interpreter, "CALLS"), calls);
        assert_eq!(
            eval_expression(&mut interpreter, "ORDER$").unwrap(),
            Value::string(order)
        );
    }
}

#[test]
fn a_late_format_function_changes_only_subsequent_array_output() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 DIM A(1):MAT A=3:END\n\
         100 DEF FNFMT$()\n110 MAT BASE 1:A(1)=8\n120 FNFMT$=\"0#\"\n130 FNEND",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    command(&mut interpreter, "MAT PRINT A;USING FNFMT$();COLOR(2);A");
    assert_eq!(interpreter.take_output(), " 3\n 3\n\n08\n");
}

#[test]
fn invalid_modifiers_report_errors_after_preserving_prior_array_output() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(0):MAT A=8");
    for (tail, code, expected) in [
        (r#"USING "0#",A"#, ErrorCode::Syntax, " 8\n"),
        (r#"USING "0#""#, ErrorCode::Syntax, " 8\n"),
        ("USING 2;A", ErrorCode::TypeMismatch, " 8\n"),
        ("COLOR(1,2,3);A", ErrorCode::ArgumentMismatch, " 8\n"),
        ("COLOR(2,-1);A", ErrorCode::InvalidArgument, " 8\n"),
        ("COLOR(2);A+1", ErrorCode::ForbiddenExpression, " 8\n\n"),
    ] {
        assert_eq!(
            interpreter
                .process_immediate(&format!("MAT PRINT A;{tail}"))
                .unwrap_err()
                .code,
            code,
            "{tail}"
        );
        assert_eq!(interpreter.take_output(), expected, "{tail}");
    }
}

#[test]
fn horizontal_print_colors_column_slices_without_transposing_them() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            r####"10 MAT BASE 1
20 DIM A(3,3)
30 MAT READ A
40 MAT PRINT USING "###";COLOR(3);A(,1) | COLOR(2);A(,2:UBOUND(A,2))
50 END
60 DATA 1,2,3,4,5,6,7,8,9"####,
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(
        interpreter.take_output(),
        "  1    2    3\n  4    5    6\n  7    8    9\n"
    );
}

#[test]
fn mat_print_selects_explicit_cells_and_orients_after_slicing() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 1:DIM A(2,3)");
    for row in 0..=2 {
        for col in 0..=3 {
            command(
                &mut interpreter,
                &format!("A({row},{col})={}", row * 10 + col),
            );
        }
    }
    command(
        &mut interpreter,
        r#"MAT PRINT USING "0#";A(0,1:2);A(2,3);A(,0);COL A(1:2,2:3)"#,
    );
    assert_eq!(
        interpreter.take_output(),
        "01  02\n\n23\n\n10\n20\n\n12  22\n13  23\n"
    );
}

#[test]
fn mat_print_supports_reverse_and_empty_vector_slices() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        r#"MAT BASE 1:DIM V$(3):V$(0)="zero":V$(1)="one":V$(2)="two":V$(3)="three"#,
    );
    command(&mut interpreter, "MAT PRINT COL V$(4:-1);V$(2)");
    assert_eq!(interpreter.take_output(), "three  two  one  zero\n\ntwo\n");
    command(&mut interpreter, "MAT PRINT V$(1:0)");
    assert!(interpreter.take_output().is_empty());
}

#[test]
fn horizontal_blocks_mix_with_stacked_arrays() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        r#"DIM A$(1),B$(1),C$(1),D$(0),E$(0):A$(0)="x":A$(1)="xxx":B$(0)="yy":B$(1)="y":C$(0)="z":C$(1)="zz":D$(0)="before":E$(0)="after"#,
    );
    command(&mut interpreter, "MAT PRINT D$;A$ | B$ | C$;E$");
    assert_eq!(
        interpreter.take_output(),
        "before\n\nx  yy  z\nxxx  y  zz\n\nafter\n"
    );
}

#[test]
fn horizontal_print_respects_wide_fields_and_quoted_pipes() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(0),B(0),C(0):A(0)=1:B(0)=2:C(0)=3");
    command(
        &mut interpreter,
        r#"MAT PRINT USING "0#|";COLOR(2),A, | COLOR(3),B;C,"#,
    );
    assert_eq!(
        interpreter.take_output(),
        format!("{:>22}  02|\n\n{:>22}\n", "01|", "03|")
    );
    command(&mut interpreter, r#"MAT PRINT USING "0#|";A | "/" | B,"#);
    assert_eq!(interpreter.take_output(), format!("01|/{:>22}\n", "02|"));
    command(&mut interpreter, r#"MAT PRINT USING "0#|";A;B"#);
    assert_eq!(interpreter.take_output(), "01|\n\n02|\n");
    command(&mut interpreter, r#"PRINT "a|b""#);
    assert_eq!(interpreter.take_output(), "a|b\n");
}

#[test]
fn horizontal_print_reuses_custom_separators_across_rows() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            r#"10 DIM A$(1),B$(1),C$(1)
20 A$(0)="x":A$(1)="xx":MAT B$="y":MAT C$="z":CALLS=0:END
100 DEF FNSEP$()
110 CALLS=CALLS+1:FNSEP$=SPACE$(2)
120 FNEND"#,
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    command(
        &mut interpreter,
        r#"MAT PRINT A$ | " | " | B$ | (FNSEP$()) | C$"#,
    );
    assert_eq!(interpreter.take_output(), "x | y  z\nxx | y  z\n");
    assert_eq!(number(&mut interpreter, "CALLS"), 1.0);

    command(&mut interpreter, r#"MAT PRINT A$ | "" | C$"#);
    assert_eq!(interpreter.take_output(), "xz\nxxz\n");

    command(
        &mut interpreter,
        r#"SEP$=" :: ":MAT PRINT B$ | (SEP$) | C$"#,
    );
    assert_eq!(interpreter.take_output(), "y :: z\ny :: z\n");
}

#[test]
fn horizontal_print_preserves_spacing_and_open_line_when_joining_slices() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "MAT BASE 1:DIM A(2,3):A(1,1)=1:A(1,2)=52:A(1,3)=3:\
         A(2,1)=-400:A(2,2)=5:A(2,3)=601:MAT X=A(,1):MAT Y=A(,2:3)",
    );
    for (using, sep) in [
        ("", ""),
        ("", ","),
        (r#"USING "0#";"#, ";"),
        (r#"USING "0#";"#, ","),
    ] {
        command(
            &mut interpreter,
            &format!(r#"PRINT "prefix ";:MAT PRINT {using}A{sep}"#),
        );
        let whole = interpreter.take_output();
        command(
            &mut interpreter,
            &format!(r#"PRINT "prefix ";:MAT PRINT {using}X{sep} | Y{sep}"#),
        );
        assert_eq!(interpreter.take_output(), whole, "{using}{sep}");
    }
}

#[test]
fn horizontal_print_skips_zero_column_blocks_but_keeps_empty_string_cells() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        r#"MAT BASE 1:DIM A$(2,2):A$(1,1)="a":A$(1,2)="b":A$(2,2)="c":MAT X$=A$(,1):MAT Y$=A$(,2):MAT E$=A$(,1:0)"#,
    );
    command(&mut interpreter, "MAT PRINT E$ | X$ | E$ | Y$ | E$");
    assert_eq!(interpreter.take_output(), "a  b\n  c\n");
    command(
        &mut interpreter,
        "MAT PRINT A$(,1:0) | A$(,1) | A$(,1:0) | A$(,2) | A$(,1:0)",
    );
    assert_eq!(interpreter.take_output(), "a  b\n  c\n");
}

#[test]
fn horizontal_print_rejects_control_characters_before_outputting_the_group() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        r#"DIM A$(1),B$(1):MAT A$="a":MAT B$="b":A$(1)="a"+CHR$(10)"#,
    );
    for source in ["MAT PRINT A$ | B$", "MAT PRINT B$ | (CHR$(9)) | B$"] {
        assert_eq!(
            interpreter.process_immediate(source).unwrap_err().code,
            ErrorCode::InvalidValue,
            "{source}"
        );
        assert!(interpreter.take_output().is_empty(), "{source}");
    }
}

#[test]
fn horizontal_print_matches_displayed_heights_after_base_and_orientation() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "MAT BASE 1:DIM A(1,2),B(2,1):A(1,1)=1:A(1,2)=2:B(1,1)=3:B(2,1)=4",
    );
    for source in ["MAT PRINT ROW A | B", "MAT PRINT ROW A(1,) | B(,1:1)"] {
        assert_eq!(
            interpreter.process_immediate(source).unwrap_err().code,
            ErrorCode::InvalidDimensions,
            "{source}"
        );
        assert!(interpreter.take_output().is_empty(), "{source}");
    }
    command(&mut interpreter, r#"MAT PRINT USING "0#";ROW A | COL B"#);
    assert_eq!(interpreter.take_output(), "01  02  03  04\n");
    command(
        &mut interpreter,
        r#"MAT PRINT USING "0#";ROW A(1,) | COL B(1:2,1)"#,
    );
    assert_eq!(interpreter.take_output(), "01  02  03  04\n");
}

#[test]
fn horizontal_print_requires_an_array_on_each_side_of_a_pipe() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(0):A(0)=1");
    for source in [
        "MAT PRINT | A",
        "MAT PRINT A |",
        "MAT PRINT A || A",
        "MAT PRINT A | COLOR(2)",
    ] {
        assert_eq!(
            interpreter.process_immediate(source).unwrap_err().code,
            ErrorCode::Syntax,
            "{source}"
        );
        assert!(interpreter.take_output().is_empty(), "{source}");
    }
}

#[test]
fn horizontal_print_evaluates_modifiers_once_in_order_and_keeps_earlier_rows() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            r#"10 DIM A(1):A(0)=3:A(1)=7:ORDER$="":END
100 DEF FNCOLOR(N,V)
110 ORDER$=ORDER$+CHR$(64+N)
120 FNCOLOR=V
130 FNEND
200 DEF FNFMT$()
210 ORDER$=ORDER$+"F"
220 FNFMT$="0#"
230 FNEND
300 DEF FNLATE$()
310 ORDER$=ORDER$+"L":A(0)=8
320 FNLATE$="00#"
330 FNEND"#,
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    command(
        &mut interpreter,
        "MAT PRINT COLOR(FNCOLOR(1,2),FNCOLOR(2,4));USING FNFMT$();A | USING FNLATE$();COLOR(FNCOLOR(3,1));A | COLOR();A",
    );
    assert_eq!(interpreter.take_output(), "03  008  008\n07  007  007\n");
    assert_eq!(
        eval_expression(&mut interpreter, "ORDER$").unwrap(),
        Value::string("ABFLC")
    );
}

#[test]
fn horizontal_print_evaluates_slice_bounds_once_in_order_and_keeps_earlier_rows() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 DIM A(1):A(0)=3:A(1)=7:ORDER=0:END\n\
             100 DEF FNIDX(N,I)\n110 ORDER=ORDER*10+N\n120 IF N=3 THEN A(0)=8\n130 FNIDX=I\n140 FNEND",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    command(
        &mut interpreter,
        r#"MAT PRINT USING "0#";A(FNIDX(1,0):FNIDX(2,1)) | A(FNIDX(3,0):FNIDX(4,1))"#,
    );
    assert_eq!(interpreter.take_output(), "03  08\n07  07\n");
    assert_eq!(number(&mut interpreter, "ORDER"), 1234.0);
}

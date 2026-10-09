use avl_basic::{ErrorCode, Interpreter};

fn load_program(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.process_immediate("ZONE 8").unwrap();
    for line in source.lines() {
        interpreter.process_immediate(line).unwrap();
    }
    interpreter
}

fn run_program(source: &str) -> String {
    let mut interpreter = load_program(source);
    interpreter.process_immediate("RUN").unwrap();
    interpreter.take_output()
}

#[test]
fn read_only_fn_observes_complete_array_without_changing_caller_or_formal_global() {
    assert_eq!(
        run_program(
            r#"10 DIM source(1,1),values(0):MAT source=3:values(0)=300
20 PRINT FNRead(source)
30 PRINT SUM(source);values(0);UBOUND(values)
40 END
100 DEF FNRead(values)
110 FNRead=SUM(values)+10*UBOUND(values,1)+100*UBOUND(values,2)
120 FNEND"#,
        ),
        " 122\n 12  300  0\n"
    );
}

#[test]
fn parameter_writes_mat_and_redim_update_the_callers_array() {
    for (mutation, expected, caller) in [
        ("values(0)=99", " 205\n", " 99  6  1  300  0\n"),
        ("MAT values=99", " 298\n", " 99  99  1  300  0\n"),
        ("MAT values=values+10", " 131\n", " 15  16  1  300  0\n"),
        (
            "REDIM values(2):values(0)=99:values(2)=101",
            " 406\n",
            " 99  6  2  300  0\n",
        ),
    ] {
        let source = format!(
            "10 DIM source(1),values(0)\n\
             20 source(0)=5:source(1)=6:values(0)=300\n\
             30 PRINT FNChange(source)\n\
             40 PRINT source(0);source(1);UBOUND(source);values(0);UBOUND(values)\n\
             50 END\n100 DEF FNChange(values)\n110 {mutation}\n\
             120 FNChange=SUM(values)+100*UBOUND(values)\n130 FNEND"
        );
        assert_eq!(
            run_program(&source),
            format!("{expected}{caller}"),
            "{mutation}"
        );
    }
}

#[test]
fn fn_array_reference_observes_global_writes_and_dimension_changes() {
    for (mutation, returned, caller) in [
        ("source(0)=70", " 176\n", " 70  6  76  1\n"),
        ("MAT source=70", " 240\n", " 70  70  140  1\n"),
        ("MAT source=source+10", " 131\n", " 15  16  31  1\n"),
        (
            "REDIM source(2):source(0)=70:source(2)=90",
            " 276\n",
            " 70  6  166  2\n",
        ),
    ] {
        let program = format!(
            "10 DIM source(1):source(0)=5:source(1)=6\n\
             20 PRINT FNReadReference(source)\n\
             30 PRINT source(0);source(1);SUM(source);UBOUND(source)\n\
             40 END\n100 DEF FNReadReference(values)\n110 {mutation}\n\
             120 FNReadReference=values(0)+values(1)+100*UBOUND(values)\n130 FNEND"
        );
        assert_eq!(
            run_program(&program),
            format!("{returned}{caller}"),
            "{mutation}"
        );
    }
}

#[test]
fn repeated_actual_array_arguments_alias_the_same_callers_array() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),first(0),second(0)
20 source(0)=5:first(0)=300:second(0)=400
30 PRINT FNTouch(source,source)
40 PRINT source(0);first(0);second(0)
50 END
100 DEF FNTouch(first,second)
110 first(0)=99
120 PRINT first(0);second(0);source(0)
130 second(0)=88
140 FNTouch=first(0)*100+second(0)
150 FNEND"#,
        ),
        " 99  99  99\n 8888\n 88  300  400\n"
    );
}

#[test]
fn string_array_references_share_parameter_and_global_mutations() {
    assert_eq!(
        run_program(
            r#"10 DIM source$(1),firstText$(0),secondText$(0)
20 source$(0)="original":source$(1)="stable"
30 firstText$(0)="outside first":secondText$(0)="outside second"
40 PRINT FNTouch$(source$,source$)
50 PRINT source$(0);"|";source$(1);"|";firstText$(0);"|";secondText$(0)
60 END
100 DEF FNTouch$(firstText$,secondText$)
110 MAT firstText$="filled"
120 source$(0)="caller"
130 PRINT firstText$(0);"|";secondText$(0);"|";source$(0)
140 FNTouch$=firstText$(1)+"/"+secondText$(1)
150 FNEND"#,
        ),
        "caller|caller|caller\nfilled/filled\ncaller|filled|outside first|outside second\n"
    );
}

#[test]
fn mutations_through_nested_sub_aliases_are_visible_through_fn_reference() {
    assert_eq!(
        run_program(
            r#"10 DIM source(1),first(0),second(0)
20 source(0)=5:source(1)=6:first(0)=300:second(0)=400
30 CALL Outer(source)
40 PRINT source(0);source(1);first(0);second(0)
50 END
100 DEF SUB Outer(first)
110 CALL Inner((first))
120 PRINT first(0);first(1)
130 SUBEND
200 DEF SUB Inner(second)
210 PRINT FNTouch(second)
220 PRINT second(0);second(1)
230 SUBEND
300 DEF FNTouch(values)
310 second(0)=70:first(1)=80
320 FNTouch=values(0)+values(1)
330 FNEND"#,
        ),
        " 150\n 70  80\n 70  80\n 70  80  300  400\n"
    );
}

#[test]
fn nested_fn_references_share_parent_parameter_and_original_global() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),items(0):source(0)=5:items(0)=300
20 PRINT FNOuter(source)
30 PRINT source(0);items(0)
40 END
100 DEF FNOuter(items)
110 LOCAL innerResult
120 items(0)=items(0)+10
130 innerResult=FNInner(items)
140 FNOuter=innerResult+items(0)
150 FNEND
200 DEF FNInner(items)
210 items(0)=items(0)+100
220 source(0)=20
230 FNInner=items(0)
240 FNEND"#,
        ),
        " 40\n 20  300\n"
    );
}

#[test]
fn array_returns_survive_scope_restoration_and_remain_independent_values() {
    for ending in ["130 FNEND", "130 EXIT FN\n140 values(0)=999\n150 FNEND"] {
        let source = format!(
            "10 DIM source(1),first(1),second(1),values(0)\n\
             20 source(0)=5:source(1)=6:values(0)=300\n\
             30 MAT first=FNMirror(source)\n40 source(0)=20\n\
             50 MAT second=FNMirror(source)\n60 first(0)=30:second(1)=40\n\
             70 PRINT first(0);first(1);second(0);second(1);source(0);source(1);values(0)\n\
             80 END\n100 DEF FNMirror(values)\n120 MAT FNMirror=values\n{ending}"
        );
        assert_eq!(run_program(&source), " 30  6  20  40  20  6  300\n");
    }
}

#[test]
fn redim_updates_outer_sub_reference_while_mat_return_remains_a_value() {
    assert_eq!(
        run_program(
            r#"10 DIM source(1),items(0),result(0)
20 source(0)=5:source(1)=6:items(0)=300
30 CALL Outer(source)
40 PRINT source(0);source(1);UBOUND(source);items(0);UBOUND(items)
50 END
100 DEF SUB Outer(items)
110 MAT result=FNMake(items)
120 PRINT result(0);result(1);result(2);UBOUND(result)
125 result(0)=77
130 items(1)=8
140 SUBEND
200 DEF FNMake(items)
210 REDIM items(2)
220 MAT items=items+10
230 MAT FNMake=items
240 EXIT FN
250 items(0)=999
260 FNEND"#,
        ),
        " 15  16  10  2\n 15  8  2  300  0\n"
    );
}

#[test]
fn body_error_keeps_array_writes_and_restores_parameter_bindings() {
    let mut interpreter = load_program(
        r#"10 DIM source(1),items(0)
20 source(0)=5:source(1)=6:items(0)=300:shouldFail=42
30 CALL Outer(source)
40 END
100 DEF SUB Outer(items)
110 CALL Inner(items)
120 SUBEND
150 DEF SUB Inner(items)
160 PRINT FNFail(items,1)
170 SUBEND
200 DEF FNFail(items,shouldFail)
210 MAT items=99
220 REDIM items(2)
230 source(0)=70
240 IF shouldFail THEN FNFail=1/0
250 FNFail=items(0)
260 FNEND"#,
    );
    let error = interpreter.process_immediate("RUN").unwrap_err();
    assert_eq!(error.code, ErrorCode::DivisionByZero);
    assert_eq!(error.line, Some(240));
    assert_eq!(interpreter.take_output(), "");
    interpreter
        .process_immediate(
            "PRINT source(0);source(1);source(2);UBOUND(source);items(0);UBOUND(items);shouldFail",
        )
        .unwrap();
    assert_eq!(interpreter.take_output(), " 70  99  0  2  300  0  42\n");
    interpreter
        .process_immediate("PRINT FNFail(source,0)")
        .unwrap();
    assert_eq!(interpreter.take_output(), " 70\n");
    interpreter
        .process_immediate("PRINT source(0);source(1);source(2);items(0);shouldFail")
        .unwrap();
    assert_eq!(interpreter.take_output(), " 70  99  99  300  42\n");
}

#[test]
fn binding_and_local_errors_restore_array_bindings_and_sub_aliases() {
    for (locals, argument, code) in [
        ("", "7", ErrorCode::TypeMismatch),
        (
            "205 LOCAL scratch(-1)\n",
            "\"valid\"",
            ErrorCode::InvalidValue,
        ),
    ] {
        let mut interpreter = load_program(&format!(
            "10 DIM source(0),items(0):source(0)=5:items(0)=300:savedText$=\"outside\"\n\
             20 CALL Outer(source)\n30 END\n\
             100 DEF SUB Outer(items)\n110 PRINT FNFail(items,{argument})\n120 SUBEND\n\
             200 DEF FNFail(items,savedText$)\n{locals}210 MAT items=99\n\
             220 FNFail=items(0)\n230 FNEND"
        ));
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(interpreter.take_output(), "");
        interpreter
            .process_immediate("PRINT source(0);items(0):PRINT savedText$")
            .unwrap();
        assert_eq!(interpreter.take_output(), " 5  300\noutside\n");
    }
}

#[test]
fn array_return_captures_value_before_later_writes_to_reference_parameter() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),result(0):source(0)=5
20 MAT result=FNCapture(source)
30 PRINT result(0);source(0)
40 result(0)=77
50 PRINT result(0);source(0)
60 END
100 DEF FNCapture(values)
110 MAT FNCapture=values
120 values(0)=99
130 EXIT FN
140 values(0)=999
150 FNEND"#,
        ),
        " 5  99\n 77  99\n"
    );
    assert_eq!(
        run_program(
            r#"10 DIM source$(0),result$(0):source$(0)="original"
20 MAT result$=FNCapture$(source$)
30 PRINT result$(0);"|";source$(0)
40 result$(0)="from caller"
50 PRINT result$(0);"|";source$(0)
60 END
100 DEF FNCapture$(values$)
110 MAT FNCapture$=values$
120 values$(0)="from FN"
130 EXIT FN
140 FNEND"#,
        ),
        "original|from FN\nfrom caller|from FN\n"
    );
}

#[test]
fn swapped_string_formal_names_keep_each_original_actual_array() {
    assert_eq!(
        run_program(
            r#"10 DIM leftText$(0),rightText$(0)
20 leftText$(0)="left":rightText$(0)="right"
30 PRINT FNExchange$(rightText$,leftText$)
40 PRINT leftText$(0);"|";rightText$(0)
50 END
100 DEF FNExchange$(leftText$,rightText$)
110 leftText$(0)=leftText$(0)+"!"
120 rightText$(0)=rightText$(0)+"?"
130 FNExchange$=leftText$(0)+"/"+rightText$(0)
140 FNEND"#,
        ),
        "right!/left?\nleft?|right!\n"
    );
}

#[test]
fn mat_operand_snapshot_respects_function_side_effects_and_left_to_right_order() {
    for (expression, expected) in [
        ("source+FNRewrite(0)", " 15  16  70  6\n"),
        ("FNRewrite(0)+source", " 80  16  70  6\n"),
    ] {
        let program = format!(
            "10 DIM source(1),result(1):source(0)=5:source(1)=6\n\
             20 MAT result={expression}\n\
             30 PRINT result(0);result(1);source(0);source(1)\n40 END\n\
             100 DEF FNRewrite(unusedValue)\n110 source(0)=70\n\
             120 FNRewrite=10\n130 FNEND"
        );
        assert_eq!(run_program(&program), expected, "{expression}");
    }
}

#[test]
fn fn_and_sub_crossed_vector_matrix_arguments_use_actual_shapes_for_indexes() {
    for string_arrays in [false, true] {
        let (
            left,
            right,
            total,
            initial_left,
            initial_right,
            sum,
            left_write,
            right_write,
            separator,
            expected,
        ) = if string_arrays {
            (
                "leftValues$",
                "rightValues$",
                "totalValue$",
                "\"left\"",
                "\"right\"",
                "leftValues$(0,0)+\"/\"+rightValues$(0,0)",
                "leftValues$(0,0)=leftValues$(0,0)+\"!\"",
                "rightValues$(0,0)=rightValues$(0,0)+\"?\"",
                ";\"|\";",
                "right/left\nleft?|right!\n 0  0  0\n",
            )
        } else {
            (
                "leftValues",
                "rightValues",
                "totalValue",
                "5",
                "20",
                "leftValues(0,0)+rightValues(0,0)",
                "leftValues(0,0)=leftValues(0,0)+10",
                "rightValues(0,0)=rightValues(0,0)+1",
                ";",
                " 25\n 6  30\n 0  0  0\n",
            )
        };
        for function in [false, true] {
            let (call, definition, returned, ending) = if function {
                let name = if string_arrays {
                    "FNExchange$"
                } else {
                    "FNExchange"
                };
                (
                    format!("PRINT {name}({right},{left})"),
                    format!("DEF {name}({left},{right})"),
                    format!("150 {name}={total}\n"),
                    "FNEND",
                )
            } else {
                (
                    format!("CALL Exchange({right},{left})"),
                    format!("DEF SUB Exchange({left},{right})"),
                    format!("150 PRINT {total}\n"),
                    "SUBEND",
                )
            };
            let program = format!(
                "10 DIM {left}(0),{right}(0,0):{left}(0)={initial_left}:{right}(0,0)={initial_right}\n\
                 20 {call}\n30 PRINT {left}(0){separator}{right}(0,0)\n\
                 40 PRINT UBOUND({left});UBOUND({right},1);UBOUND({right},2)\n50 END\n\
                 100 {definition}\n110 LOCAL {total}\n120 {total}={sum}\n\
                 130 {left_write}\n140 {right_write}\n{returned}160 {ending}"
            );
            assert_eq!(run_program(&program), expected, "{definition}");
        }
    }
}

#[test]
fn nested_same_function_array_return_is_independent_of_its_reference_input() {
    for string_arrays in [false, true] {
        let (source, parameter, result, function, initial, sentinel, change, separator, expected) =
            if string_arrays {
                (
                    "source$",
                    "items$",
                    "result$",
                    "FNCapture$",
                    "\"original\"",
                    "\"outside\"",
                    "items$(0)=items$(0)+\"!\"",
                    ";\"|\";",
                    "original|original!|outside\n",
                )
            } else {
                (
                    "source",
                    "items",
                    "result",
                    "FNCapture",
                    "5",
                    "300",
                    "items(0)=items(0)+1",
                    ";",
                    " 5  6  300\n",
                )
            };
        let program = format!(
            "10 DIM {source}(0),{parameter}(0),{result}(0):{source}(0)={initial}:{parameter}(0)={sentinel}\n\
             20 MAT {result}={function}({function}({source}))\n\
             30 PRINT {result}(0){separator}{source}(0){separator}{parameter}(0)\n40 END\n\
             100 DEF {function}({parameter})\n110 MAT {function}={parameter}\n\
             120 {change}\n130 EXIT FN\n140 FNEND"
        );
        assert_eq!(run_program(&program), expected);
    }
}

#[test]
fn outer_error_with_same_function_array_result_input_restores_scopes_and_allows_reentry() {
    for string_arrays in [false, true] {
        let (
            source,
            parameter,
            result,
            function,
            initial,
            sentinel,
            saved_result,
            change,
            check,
            restored,
            after,
        ) = if string_arrays {
            (
                "source$",
                "items$",
                "result$",
                "FNCapture$",
                "\"original\"",
                "\"outside\"",
                "\"unchanged\"",
                "items$(0)=items$(0)+\"!\"",
                "PRINT source$(0);\"|\";items$(0);\"|\";result$(0):PRINT invocations",
                "original!|outside|unchanged\n 2\n",
                "original!!|outside|original!\n 3\n",
            )
        } else {
            (
                "source",
                "items",
                "result",
                "FNCapture",
                "5",
                "300",
                "700",
                "items(0)=items(0)+1",
                "PRINT source(0);items(0);result(0);invocations",
                " 6  300  700  2\n",
                " 7  300  6  3\n",
            )
        };
        let program = format!(
            "10 DIM {source}(0),{parameter}(0),{result}(0)\n\
             20 {source}(0)={initial}:{parameter}(0)={sentinel}:{result}(0)={saved_result}:invocations=0\n\
             30 MAT {result}={function}({function}({source}))\n40 END\n\
             100 DEF {function}({parameter})\n110 MAT {function}={parameter}\n\
             120 {change}\n130 invocations=invocations+1\n\
             140 IF invocations=2 THEN {function}=1/0\n150 FNEND"
        );
        let mut interpreter = load_program(&program);
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::DivisionByZero);
        assert_eq!(error.line, Some(140));
        assert_eq!(interpreter.take_output(), "");
        interpreter.process_immediate(check).unwrap();
        assert_eq!(interpreter.take_output(), restored);
        interpreter
            .process_immediate(&format!("MAT {result}={function}({source})"))
            .unwrap();
        interpreter.process_immediate(check).unwrap();
        assert_eq!(interpreter.take_output(), after);
    }
}

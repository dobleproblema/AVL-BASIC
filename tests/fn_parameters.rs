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
fn bare_fn_arguments_prefer_visible_numeric_and_string_scalars() {
    for multiline in [false, true] {
        for string_value in [false, true] {
            let (setup, name, parameter, argument, expression, expected, restored) = if string_value
            {
                (
                    "DIM source$(0):source$(0)=\"array\":source$=\"scalar\"",
                    "FNEcho$",
                    "value$",
                    "source$",
                    "value$+\"!\"",
                    "scalar!\n",
                    "scalar|array\n",
                )
            } else {
                (
                    "DIM source(0):source(0)=100:source=7",
                    "FNIncrement",
                    "value",
                    "source",
                    "value+1",
                    " 8\n",
                    " 7  100\n",
                )
            };
            let definition = if multiline {
                format!("100 DEF {name}({parameter})\n110 {name}={expression}\n120 FNEND")
            } else {
                format!("100 DEF {name}({parameter})={expression}")
            };
            let mut interpreter = load_program(&format!(
                "10 {setup}\n20 PRINT {name}({argument})\n30 END\n{definition}"
            ));
            interpreter.process_immediate("RUN").unwrap();
            assert_eq!(interpreter.take_output(), expected);
            let check = if string_value {
                "PRINT source$;\"|\";source$(0)"
            } else {
                "PRINT source;source(0)"
            };
            interpreter.process_immediate(check).unwrap();
            assert_eq!(interpreter.take_output(), restored);
        }
    }
}

#[test]
fn fn_arguments_preserve_sub_local_scalars_with_global_array_homonyms() {
    for string_value in [false, true] {
        let (setup, local, value, name, parameter, expression, expected) = if string_value {
            (
                "DIM normalText$(0):normalText$(0)=\"array\"",
                "normalText$",
                "\"scalar\"",
                "FNEcho$",
                "value$",
                "value$+\"!\"",
                "scalar!\n",
            )
        } else {
            (
                "DIM normalX(0):normalX(0)=100",
                "normalX",
                "7",
                "FNIncrement",
                "value",
                "value+1",
                " 8\n",
            )
        };
        assert_eq!(
            run_program(&format!(
                "10 {setup}\n20 CALL Outer\n30 END\n\
                 100 DEF SUB Outer\n110 LOCAL {local}\n120 {local}={value}\n\
                 130 PRINT {name}({local})\n140 SUBEND\n\
                 200 DEF {name}({parameter})\n210 {name}={expression}\n220 FNEND"
            )),
            expected
        );
    }
}

#[test]
fn fn_scalar_priority_leaves_mat_operands_and_array_name_intrinsics_unchanged() {
    assert_eq!(
        run_program(
            r#"10 DIM A(1),B(1):MAT A=3:A=-7
20 DEF FNABS(value)=ABS(value)
30 MAT B=A+FNABS(A)
40 IF UBOUND(A)<>1 OR SUM(A)<>6 OR B(0)<>10 OR B(1)<>10 THEN PRINT "FAIL"
50 PRINT FNABS(A)
60 PRINT A;A(0)"#,
        ),
        " 7\n-7  3\n"
    );
}

#[test]
fn fn_array_references_preserve_sub_alias_on_fnend_and_exit_fn() {
    for parameter in ["items", "renamedItems"] {
        for early_exit in [false, true] {
            let ending = if early_exit {
                format!("230 EXIT FN\n240 {parameter}(0)=101\n250 FNEND")
            } else {
                "230 FNEND".to_string()
            };
            let program = format!(
                "10 DIM source(1),items(0)\n\
                 20 source(0)=5:source(1)=6:items(0)=300\n\
                 30 CALL Outer(source)\n40 PRINT source(0);source(1);items(0)\n50 END\n\
                 100 DEF SUB Outer(items)\n110 PRINT FNChange(items)\n\
                 120 PRINT items(0);items(1)\n130 items(1)=8\n140 SUBEND\n\
                 200 DEF FNChange({parameter})\n210 {parameter}(0)=99\n\
                 220 FNChange={parameter}(0)\n{ending}"
            );
            assert_eq!(run_program(&program), " 99\n 99  6\n 99  8  300\n");
        }
    }
}

#[test]
fn fn_string_array_reference_restores_outer_sub_alias() {
    assert_eq!(
        run_program(
            r#"10 DIM source$(0),items$(0):source$(0)="source":items$(0)="outer"
20 CALL Outer(source$)
30 PRINT source$(0);"|";items$(0)
40 END
100 DEF SUB Outer(items$)
110 PRINT FNChange$(items$)
120 PRINT items$(0)
130 items$(0)="changed by SUB"
140 SUBEND
200 DEF FNChange$(items$)
210 items$(0)="changed by FN"
220 FNChange$=items$(0)
230 EXIT FN
240 FNEND"#,
        ),
        "changed by FN\nchanged by FN\nchanged by SUB|outer\n"
    );
}

#[test]
fn nested_fn_array_references_restore_each_scope_and_its_sub_alias() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),items(0):source(0)=5:items(0)=300
20 CALL Outer(source)
30 PRINT source(0);items(0)
40 END
100 DEF SUB Outer(items)
110 PRINT FNMiddle(items)
120 PRINT items(0)
130 items(0)=8
140 SUBEND
200 DEF FNMiddle(items)
210 LOCAL nested
220 items(0)=items(0)+10
230 nested=FNInner(items)
240 FNMiddle=nested+items(0)
250 FNEND
300 DEF FNInner(items)
310 items(0)=items(0)+100
320 FNInner=items(0)
330 FNEND"#,
        ),
        " 230\n 115\n 8  300\n"
    );
}

#[test]
fn fn_array_returns_and_redim_preserve_the_callers_original_reference() {
    for parameter in ["items", "renamedItems"] {
        let program = format!(
            "10 DIM source(1),items(0),result(0)\n\
             20 source(0)=5:source(1)=6:items(0)=300\n\
             30 CALL Outer(source)\n40 PRINT source(0);source(1);items(0)\n50 END\n\
             100 DEF SUB Outer(items)\n110 MAT result=FNMake(items)\n\
             120 PRINT result(0);result(2);UBOUND(result)\n125 result(0)=77\n\
             130 PRINT items(0);items(1);UBOUND(items)\n140 items(1)=8\n150 SUBEND\n\
             200 DEF FNMake({parameter})\n210 REDIM {parameter}(2)\n\
             220 {parameter}(0)=99:{parameter}(2)=101\n\
             230 MAT FNMake={parameter}\n240 EXIT FN\n250 FNEND"
        );
        assert_eq!(
            run_program(&program),
            " 99  101  2\n 99  6  2\n 99  8  300\n"
        );
    }
}

#[test]
fn fn_body_errors_preserve_array_writes_and_restore_outer_bindings() {
    for parameter in ["items", "renamedItems"] {
        let mut interpreter = load_program(&format!(
            "10 DIM source(1),items(0)\n\
             20 source(0)=5:source(1)=6:items(0)=300\n30 CALL Outer(source)\n40 END\n\
             100 DEF SUB Outer(items)\n110 PRINT FNFail(items)\n120 SUBEND\n\
             200 DEF FNFail({parameter})\n210 {parameter}(0)=99\n\
             220 FNFail=1/0\n230 FNEND"
        ));
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::DivisionByZero);
        assert_eq!(error.line, Some(220));
        assert_eq!(interpreter.take_output(), "");
        interpreter
            .process_immediate("PRINT source(0);source(1);items(0)")
            .unwrap();
        assert_eq!(interpreter.take_output(), " 99  6  300\n");
    }
}

#[test]
fn fn_binding_errors_restore_partially_bound_sub_aliases() {
    for (locals, call, code) in [
        ("", "FNFail(items,7)", ErrorCode::TypeMismatch),
        (
            "205 LOCAL scratch(-1)\n",
            "FNFail(items,\"valid\")",
            ErrorCode::InvalidValue,
        ),
    ] {
        let mut interpreter = load_program(&format!(
            "10 DIM source(0),items(0):source(0)=5:items(0)=300:savedText$=\"outer\"\n\
             20 CALL Outer(source)\n30 END\n\
             100 DEF SUB Outer(items)\n110 PRINT {call}\n120 SUBEND\n\
             200 DEF FNFail(items,savedText$)\n{locals}210 FNFail=1\n220 FNEND"
        ));
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, code);
        interpreter
            .process_immediate("PRINT source(0);items(0):PRINT savedText$")
            .unwrap();
        assert_eq!(interpreter.take_output(), " 5  300\nouter\n");
    }
}

#[test]
fn fn_array_formals_keep_the_separate_homonymous_scalar_namespace() {
    let mut interpreter = load_program(
        r#"10 DIM source(0):source(0)=5:value=7
20 PRINT FNRead(source)
30 END
100 DEF FNRead(value)
110 value(0)=99
120 FNRead=value
130 FNEND"#,
    );
    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
    interpreter
        .process_immediate("PRINT source(0);value")
        .unwrap();
    assert_eq!(interpreter.take_output(), " 99  7\n");
}

#[test]
fn fn_same_name_and_grouped_array_arguments_keep_the_callers_reference() {
    for parameter in ["source", "items"] {
        for argument in ["source", "(source)", "((source))", "(((source)))"] {
            let program = format!(
                "10 DIM source(0),items(0):source(0)=5:items(0)=300\n\
                 20 PRINT FNChange({argument})\n30 PRINT source(0);items(0)\n40 END\n\
                 100 DEF FNChange({parameter})\n110 {parameter}(0)={parameter}(0)+1\n\
                 120 FNChange={parameter}(0)\n130 FNEND"
            );
            assert_eq!(
                run_program(&program),
                " 6\n 6  300\n",
                "{parameter}: {argument}"
            );
        }
    }
}

#[test]
fn fn_elements_and_computed_arguments_remain_scalar_values() {
    for (argument, expected) in [
        ("source(0)", " 8\n"),
        ("(source(0))", " 8\n"),
        ("((source(0)))", " 8\n"),
        ("source(0)+1", " 9\n"),
        ("(source(0)+1)", " 9\n"),
        ("source+0", " 1\n"),
    ] {
        let program = format!(
            "10 DIM source(0):source(0)=7:value=42\n\
             20 PRINT FNChange({argument})\n30 PRINT source(0);value\n40 END\n\
             100 DEF FNChange(value)\n110 value=value+1\n\
             120 FNChange=value\n130 FNEND"
        );
        assert_eq!(
            run_program(&program),
            format!("{expected} 7  42\n"),
            "{argument}"
        );
    }
    for (argument, expected) in [
        ("source$(0)", "array!\n"),
        ("(source$(0))", "array!\n"),
        ("((source$(0)))", "array!\n"),
        ("source$(0)+\"?\"", "array?!\n"),
        ("source$+\"\"", "!\n"),
    ] {
        let program = format!(
            "10 DIM source$(0):source$(0)=\"array\":value$=\"saved\"\n\
             20 PRINT FNChange$({argument})\n30 PRINT source$(0);\"|\";value$\n40 END\n\
             100 DEF FNChange$(value$)\n110 value$=value$+\"!\"\n\
             120 FNChange$=value$\n130 FNEND"
        );
        assert_eq!(
            run_program(&program),
            format!("{expected}array|saved\n"),
            "{argument}"
        );
    }
}

#[test]
fn fn_swapped_actual_and_formal_array_names_resolve_before_binding() {
    for ending in ["140 FNEND", "140 EXIT FN\n150 leftValues(0)=999\n160 FNEND"] {
        let program = format!(
            "10 DIM leftValues(0),rightValues(0):leftValues(0)=5:rightValues(0)=20\n\
             20 PRINT FNExchange(rightValues,leftValues)\n\
             30 PRINT leftValues(0);rightValues(0)\n40 END\n\
             100 DEF FNExchange(leftValues,rightValues)\n110 leftValues(0)=leftValues(0)+1\n\
             120 rightValues(0)=rightValues(0)+2\n\
             130 FNExchange=leftValues(0)*100+rightValues(0)\n{ending}"
        );
        assert_eq!(run_program(&program), " 2107\n 7  21\n");
    }
}

#[test]
fn fn_rebound_alias_points_to_original_actual_not_later_formal_binding() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),items(0):source(0)=5:items(0)=300
20 CALL Outer(source)
30 PRINT source(0);items(0)
40 END
100 DEF SUB Outer(items)
110 PRINT FNExchange(items,source)
120 PRINT items(0);source(0)
130 SUBEND
200 DEF FNExchange(source,items)
210 source(0)=source(0)+1
220 items(0)=items(0)+2
230 FNExchange=source(0)*100+items(0)
240 FNEND"#,
        ),
        " 808\n 8  8\n 8  300\n"
    );
}

#[test]
fn scalar_fn_parameter_preserves_separate_inherited_array_alias() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),items(0):source(0)=5:items(0)=300:items=42
20 CALL Outer(source)
30 PRINT source(0);items;items(0)
40 END
100 DEF SUB Outer(items)
110 PRINT FNIncrement(7)
120 PRINT items;items(0)
130 SUBEND
200 DEF FNIncrement(items)
210 items(0)=items(0)+10
220 items=items+1
230 FNIncrement=items
240 FNEND"#,
        ),
        " 8\n 0  15\n 15  42  300\n"
    );
}

#[test]
fn local_array_can_shadow_actual_name_without_hiding_the_reference_parameter() {
    for string_array in [false, true] {
        let (source, parameter, function, initial, changed, local, sentinel, expected) =
            if string_array {
                (
                    "source$",
                    "items$",
                    "FNTouch$",
                    "\"original\"",
                    "\"from FN\"",
                    "\"local\"",
                    "\"outside\"",
                    "from FN\nfrom FN|outside\n",
                )
            } else {
                (
                    "source",
                    "items",
                    "FNTouch",
                    "5",
                    "9",
                    "100",
                    "300",
                    " 9\n 9  300\n",
                )
            };
        let separator = if string_array { ";\"|\";" } else { ";" };
        let program = format!(
            "10 DIM {source}(0),{parameter}(0):{source}(0)={initial}:{parameter}(0)={sentinel}\n\
             20 PRINT {function}({source})\n30 PRINT {source}(0){separator}{parameter}(0)\n40 END\n\
             100 DEF {function}({parameter})\n110 LOCAL {source}(0)\n\
             120 {parameter}(0)={changed}:{source}(0)={local}\n\
             130 {function}={parameter}(0)\n140 FNEND"
        );
        assert_eq!(run_program(&program), expected);
    }
}

#[test]
fn nested_local_binding_error_restores_hidden_actual_arrays_and_alias_scopes() {
    for string_array in [false, true] {
        let (
            source,
            parameter,
            local_array,
            middle_fn,
            failing_fn,
            initial,
            sentinel,
            saved_local,
            changed,
            local_value,
            restored,
            result,
            after,
        ) = if string_array {
            (
                "source$",
                "items$",
                "middleArray$",
                "FNMiddle$",
                "FNFail$",
                "\"original\"",
                "\"outside\"",
                "\"middle outside\"",
                "\"from FN\"",
                "\"local\"",
                "original|outside|middle outside\n",
                "from FN\n",
                "from FN|outside|middle outside\n",
            )
        } else {
            (
                "source",
                "items",
                "middleArray",
                "FNMiddle",
                "FNFail",
                "5",
                "300",
                "77",
                "9",
                "100",
                " 5  300  77\n",
                " 9\n",
                " 9  300  77\n",
            )
        };
        let program = format!(
            "10 DIM {source}(0),{parameter}(0),{local_array}(0)\n\
             20 {source}(0)={initial}:{parameter}(0)={sentinel}:{local_array}(0)={saved_local}:badBound=-1\n\
             30 CALL Outer({source})\n40 END\n\
             100 DEF SUB Outer({parameter})\n110 PRINT {middle_fn}({parameter})\n120 SUBEND\n\
             200 DEF {middle_fn}({parameter})\n210 LOCAL {local_array}(0)\n\
             220 {middle_fn}={failing_fn}({parameter})\n230 FNEND\n\
             300 DEF {failing_fn}({parameter})\n310 LOCAL {source}(0),scratch(badBound)\n\
             320 {parameter}(0)={changed}:{source}(0)={local_value}\n\
             330 {failing_fn}={parameter}(0)\n340 FNEND"
        );
        let mut interpreter = load_program(&program);
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidValue);
        assert_eq!(interpreter.take_output(), "");
        let separator = if string_array { ";\"|\";" } else { ";" };
        let check =
            format!("PRINT {source}(0){separator}{parameter}(0){separator}{local_array}(0)");
        interpreter.process_immediate(&check).unwrap();
        assert_eq!(interpreter.take_output(), restored);
        interpreter
            .process_immediate(&format!("badBound=0:PRINT {middle_fn}({source})"))
            .unwrap();
        assert_eq!(interpreter.take_output(), result);
        interpreter.process_immediate(&check).unwrap();
        assert_eq!(interpreter.take_output(), after);
    }
}

#[test]
fn local_declaration_order_restores_inherited_alias_and_hidden_actual_array() {
    for string_array in [false, true] {
        let (
            source,
            inherited,
            parameter,
            function,
            initial,
            sentinel,
            changed,
            local_source,
            local_alias,
            separator,
            expected,
        ) = if string_array {
            (
                "source$",
                "items$",
                "values$",
                "FNWork$",
                "\"original\"",
                "\"outside\"",
                "\"from FN\"",
                "\"local source\"",
                "\"local alias\"",
                ";\"|\";",
                "from FN\nfrom FN|from FN\nfrom FN|outside\n",
            )
        } else {
            (
                "source",
                "items",
                "values",
                "FNWork",
                "5",
                "300",
                "9",
                "100",
                "200",
                ";",
                " 9\n 9  9\n 9  300\n",
            )
        };
        for source_first in [false, true] {
            let locals = if source_first {
                format!("{source}(0),{inherited}(0)")
            } else {
                format!("{inherited}(0),{source}(0)")
            };
            let program = format!(
                "10 DIM {source}(0),{inherited}(0):{source}(0)={initial}:{inherited}(0)={sentinel}\n\
                 20 CALL Outer({source})\n\
                 30 PRINT {source}(0){separator}{inherited}(0)\n40 END\n\
                 100 DEF SUB Outer({inherited})\n110 PRINT {function}({inherited})\n\
                 120 PRINT {inherited}(0){separator}{source}(0)\n130 SUBEND\n\
                 200 DEF {function}({parameter})\n210 LOCAL {locals}\n\
                 220 {parameter}(0)={changed}:{source}(0)={local_source}:{inherited}(0)={local_alias}\n\
                 230 {function}={parameter}(0)\n240 FNEND"
            );
            assert_eq!(run_program(&program), expected, "{locals}");
        }
    }
}

#[test]
fn nested_same_name_local_actuals_keep_original_and_private_references_separate() {
    assert_eq!(
        run_program(
            r#"10 DIM source(0),items(0):source(0)=5:items(0)=300
20 PRINT FNOuter(source)
30 PRINT source(0);items(0)
40 END
100 DEF FNOuter(items)
110 LOCAL source(0),innerResult
120 items(0)=9:source(0)=100
130 innerResult=FNInner(source)
140 PRINT items(0);source(0)
150 FNOuter=innerResult+items(0)+source(0)
160 FNEND
200 DEF FNInner(items)
210 LOCAL source(0)
220 items(0)=items(0)+1:source(0)=200
230 FNInner=items(0)
240 FNEND"#,
        ),
        " 9  101\n 211\n 9  300\n"
    );
}

#[test]
fn redim_and_mat_update_reference_hidden_by_local_actual_name() {
    for string_array in [false, true] {
        let (source, parameter, function, initial, changed, local, sentinel, expected) =
            if string_array {
                (
                    "source$",
                    "items$",
                    "FNGrow$",
                    "\"original\"",
                    "\"from FN\"",
                    "\"local\"",
                    "\"outside\"",
                    "from FN\nfrom FN|from FN|outside\n 1  0\n",
                )
            } else {
                (
                    "source",
                    "items",
                    "FNGrow",
                    "5",
                    "9",
                    "100",
                    "300",
                    " 9\n 9  9  300\n 1  0\n",
                )
            };
        let separator = if string_array { ";\"|\";" } else { ";" };
        let program = format!(
            "10 DIM {source}(0),{parameter}(0):{source}(0)={initial}:{parameter}(0)={sentinel}\n\
             20 PRINT {function}({source})\n\
             30 PRINT {source}(0){separator}{source}(1){separator}{parameter}(0)\n\
             40 PRINT UBOUND({source});UBOUND({parameter})\n50 END\n\
             100 DEF {function}({parameter})\n110 LOCAL {source}(0)\n\
             120 REDIM {parameter}(1)\n130 MAT {parameter}={changed}\n\
             140 {source}(0)={local}\n150 {function}={parameter}(1)\n160 FNEND"
        );
        assert_eq!(run_program(&program), expected);
    }
}

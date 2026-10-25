use avl_basic::expr::eval_expression;
use avl_basic::{ErrorCode, Interpreter};

fn loaded(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter
}

fn run(source: &str) -> Interpreter {
    let mut interpreter = loaded(source);
    interpreter.run_loaded().unwrap();
    interpreter
}

fn number(interpreter: &mut Interpreter, expression: &str) -> f64 {
    eval_expression(interpreter, expression)
        .unwrap()
        .as_number()
        .unwrap()
}

fn text(interpreter: &mut Interpreter, expression: &str) -> String {
    eval_expression(interpreter, expression)
        .unwrap()
        .into_string()
        .unwrap()
}

#[test]
fn repeated_calls_reset_all_local_cells_and_restore_globals_on_both_exits() {
    let mut interpreter = run(r#"10 DIM A(1),A$(1):A(0)=71:A$(0)="global":X=91:S$="outer"
20 FOR Pass=1 TO 5:CALL Work:NEXT Pass
30 END
100 DEF SUB Work
110 LOCAL X,S$,A(2,3),A$(2),I,J
120 Initial=Initial+X+LEN(S$)
130 FOR I=0 TO 2:FOR J=0 TO 3:Initial=Initial+A(I,J):A(I,J)=100+I+J:NEXT J:NEXT I
140 FOR I=0 TO 2:Initial=Initial+LEN(A$(I)):A$(I)="dirty "+STR$(Pass):NEXT I
150 X=9:S$="dirty"
160 IF Pass MOD 2=0 THEN EXIT SUB
170 Tail=Tail+1
180 SUBEND"#);
    assert_eq!(number(&mut interpreter, "Initial"), 0.0);
    assert_eq!(number(&mut interpreter, "Tail"), 3.0);
    assert_eq!(number(&mut interpreter, "X"), 91.0);
    assert_eq!(text(&mut interpreter, "S$"), "outer");
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 1.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 71.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A$)"), 1.0);
    assert_eq!(text(&mut interpreter, "A$(0)"), "global");
}

#[test]
fn changing_local_shapes_recompute_bounds_in_order_after_earlier_bindings() {
    let mut interpreter = run(r#"10 DIM A(7):A(0)=99
20 CALL Work(1):CALL Work(3):CALL Work(0):CALL Work(2):CALL Work(1)
30 END
100 DEF SUB Work(N)
110 LOCAL A(N),B(UBOUND(A)+1),D(FNTick(),FNTick()),I
120 Bounds=Bounds+UBOUND(A):Dependent=Dependent+UBOUND(B)
130 FirstBounds=FirstBounds+UBOUND(D,1):SecondBounds=SecondBounds+UBOUND(D,2)
140 FOR I=0 TO N:Initial=Initial+A(I):A(I)=100+I:NEXT I
150 SUBEND
200 DEF FNTick()
210 Clock=Clock+1:FNTick=Clock
220 FNEND"#);
    assert_eq!(number(&mut interpreter, "Bounds"), 7.0);
    assert_eq!(number(&mut interpreter, "Dependent"), 12.0);
    assert_eq!(number(&mut interpreter, "Clock"), 10.0);
    assert_eq!(number(&mut interpreter, "FirstBounds"), 25.0);
    assert_eq!(number(&mut interpreter, "SecondBounds"), 30.0);
    assert_eq!(number(&mut interpreter, "Initial"), 0.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 7.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 99.0);
}

#[test]
fn nested_calls_preserve_dynamic_visibility_and_duplicate_aliases_to_local_arrays() {
    let mut interpreter = run(r#"10 X=90:S$="global":DIM Scratch(0):Scratch(0)=70
20 CALL Outer:CALL Outer
30 END
100 DEF SUB Outer
110 LOCAL X,S$,Scratch(1)
120 Fresh=Fresh+Scratch(0)+Scratch(1):FreshBounds=FreshBounds+UBOUND(Scratch)
130 X=7:S$="outer":Scratch(0)=11:Scratch(1)=22
140 CALL Observe
150 CALL Inner(Scratch,Scratch)
160 OuterSeen=OuterSeen+X+Scratch(2):OuterText$=OuterText$+S$
170 SUBEND
200 DEF SUB Observe
210 Observed=Observed+X+Scratch(1):ObservedText$=ObservedText$+S$
220 X=X+1:Scratch(1)=Scratch(1)+1
230 SUBEND
300 DEF SUB Inner(Scratch,Other)
310 LOCAL X,S$
320 Fresh=Fresh+X+LEN(S$):REDIM Other(2)
330 Scratch(2)=33:AliasBounds=AliasBounds+UBOUND(Scratch)
340 X=123:S$="inner":EXIT SUB
350 SUBEND"#);
    assert_eq!(number(&mut interpreter, "Fresh"), 0.0);
    assert_eq!(number(&mut interpreter, "FreshBounds"), 2.0);
    assert_eq!(number(&mut interpreter, "Observed"), 58.0);
    assert_eq!(text(&mut interpreter, "ObservedText$"), "outerouter");
    assert_eq!(number(&mut interpreter, "OuterSeen"), 82.0);
    assert_eq!(text(&mut interpreter, "OuterText$"), "outerouter");
    assert_eq!(number(&mut interpreter, "AliasBounds"), 4.0);
    assert_eq!(number(&mut interpreter, "X"), 90.0);
    assert_eq!(text(&mut interpreter, "S$"), "global");
    assert_eq!(number(&mut interpreter, "UBOUND(Scratch)"), 0.0);
    assert_eq!(number(&mut interpreter, "Scratch(0)"), 70.0);
}

#[test]
fn repeated_string_array_alias_redim_does_not_leak_into_the_next_local_invocation() {
    let mut interpreter = run(r#"10 DIM Work$(0):Work$(0)="global"
20 CALL Outer:CALL Outer:CALL Outer
30 END
100 DEF SUB Outer
110 LOCAL Work$(2),I
120 FreshBounds=FreshBounds+UBOUND(Work$)
130 FOR I=0 TO 2:Fresh=Fresh+LEN(Work$(I)):NEXT I
140 Work$(0)="caller":CALL Inner(Work$,Work$)
150 Text$=Text$+Work$(3):ChangedBounds=ChangedBounds+UBOUND(Work$)
160 SUBEND
200 DEF SUB Inner(Work$,Other$)
210 REDIM Other$(3):Work$(3)="nested"
220 SUBEND"#);
    assert_eq!(number(&mut interpreter, "Fresh"), 0.0);
    assert_eq!(number(&mut interpreter, "FreshBounds"), 6.0);
    assert_eq!(number(&mut interpreter, "ChangedBounds"), 9.0);
    assert_eq!(text(&mut interpreter, "Text$"), "nestednestednested");
    assert_eq!(number(&mut interpreter, "UBOUND(Work$)"), 0.0);
    assert_eq!(text(&mut interpreter, "Work$(0)"), "global");
}

#[test]
fn warm_call_bindings_restore_after_partial_local_and_parameter_failures() {
    let mut interpreter = run(
        r#"10 DIM Source(0),P(0),A(0),Stage$(0):P(0)=77:A(0)=88:Stage$(0)="global"
20 X=99:Tag$="outer":Bound=1
30 END
100 DEF SUB Work(P,Tag$)
110 LOCAL X,A(2),Stage$(2),B(Bound)
120 Initial=Initial+X+A(0)+A(1)+A(2)+LEN(Stage$(0))+LEN(Stage$(1))+LEN(Stage$(2))
130 P(0)=P(0)+1:X=5:A(1)=6:Stage$(1)=Tag$
140 SUBEND"#,
    );
    interpreter
        .process_immediate("CALL Work(Source,\"first\")")
        .unwrap();
    interpreter.process_immediate("Bound=-1").unwrap();
    assert_eq!(
        interpreter
            .process_immediate("CALL Work(Source,\"unused\")")
            .unwrap_err()
            .code,
        ErrorCode::InvalidValue
    );
    interpreter.process_immediate("Bound=0").unwrap();
    assert_eq!(
        interpreter
            .process_immediate("CALL Work(Source,42)")
            .unwrap_err()
            .code,
        ErrorCode::TypeMismatch
    );
    assert_eq!(number(&mut interpreter, "P(0)"), 77.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 88.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 0.0);
    assert_eq!(text(&mut interpreter, "Stage$(0)"), "global");
    assert_eq!(number(&mut interpreter, "UBOUND(Stage$)"), 0.0);
    assert_eq!(number(&mut interpreter, "X"), 99.0);
    assert_eq!(text(&mut interpreter, "Tag$"), "outer");
    assert_eq!(number(&mut interpreter, "Source(0)"), 1.0);
    interpreter
        .process_immediate("CALL Work(Source,\"last\")")
        .unwrap();
    assert_eq!(number(&mut interpreter, "Initial"), 0.0);
    assert_eq!(number(&mut interpreter, "Source(0)"), 2.0);
    assert_eq!(number(&mut interpreter, "P(0)"), 77.0);
}

#[test]
fn root_recovery_reuses_fresh_locals_after_a_warm_call_and_partial_preparation_failure() {
    for (resume, expected_calls) in [("RESUME", 3.0), ("RESUME NEXT", 2.0)] {
        let mut interpreter = run(&format!(
            r#"10 X=99:DIM A(0):A(0)=88:Bound=1:ON ERROR GOTO 80
20 CALL Work
30 Bound=-1
40 CALL Work:Tail=1
50 CALL Work
60 END
80 Hits=Hits+1:IF Hits>2 THEN END
90 Observed=X+A(0):Bound=1:{resume}
100 DEF SUB Work
110 LOCAL X,A(2),Stage$(2),B(Bound)
120 Calls=Calls+1:Initial=Initial+X+A(0)+A(1)+A(2)+LEN(Stage$(1))
130 X=5:A(1)=6:Stage$(1)="dirty"
140 SUBEND"#
        ));
        assert_eq!(
            number(&mut interpreter, "Calls"),
            expected_calls,
            "{resume}"
        );
        assert_eq!(number(&mut interpreter, "Hits"), 1.0, "{resume}");
        assert_eq!(number(&mut interpreter, "Observed"), 187.0, "{resume}");
        assert_eq!(number(&mut interpreter, "Initial"), 0.0, "{resume}");
        assert_eq!(number(&mut interpreter, "Tail"), 1.0, "{resume}");
        assert_eq!(number(&mut interpreter, "X"), 99.0, "{resume}");
        assert_eq!(number(&mut interpreter, "A(0)"), 88.0, "{resume}");
    }
}

#[test]
fn repeated_local_array_returns_remain_independent_of_reused_function_storage() {
    let mut interpreter = run(r#"10 DIM A(0):A(0)=99
20 MAT First=FNMake(2):MAT Second=FNMake(2):MAT Third=FNMake(1)
30 END
100 DEF FNMake(N)
110 LOCAL A(N),I
120 Calls=Calls+1
130 FOR I=0 TO N:Initial=Initial+A(I):A(I)=Calls*10+I:NEXT I
140 MAT FNMake=A
150 FNEND"#);
    assert_eq!(number(&mut interpreter, "Initial"), 0.0);
    assert_eq!(number(&mut interpreter, "First(0)"), 10.0);
    assert_eq!(number(&mut interpreter, "First(2)"), 12.0);
    assert_eq!(number(&mut interpreter, "Second(0)"), 20.0);
    assert_eq!(number(&mut interpreter, "Second(2)"), 22.0);
    assert_eq!(number(&mut interpreter, "Third(0)"), 30.0);
    assert_eq!(number(&mut interpreter, "Third(1)"), 31.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 0.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 99.0);
}

#[test]
fn clear_inside_repeated_sub_calls_invalidates_cached_slots_before_restoration() {
    let mut interpreter = run(r#"10 X=99:P=77:S$="outer":DIM A(0):A(0)=88:Erased=55
20 CALL Work(5):CALL Work(6)
30 END
100 DEF SUB Work(P)
110 LOCAL X,S$,A(2)
120 CLEAR
130 Created=17:X=123:S$="inside":DIM A(1):A(0)=44
140 Captured=X+P:CapturedText$=S$
150 EXIT SUB
160 SUBEND"#);
    assert_eq!(number(&mut interpreter, "X"), 99.0);
    assert_eq!(number(&mut interpreter, "P"), 77.0);
    assert_eq!(text(&mut interpreter, "S$"), "outer");
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 0.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 88.0);
    assert_eq!(number(&mut interpreter, "Created"), 17.0);
    assert_eq!(number(&mut interpreter, "Captured"), 123.0);
    assert_eq!(text(&mut interpreter, "CapturedText$"), "inside");
    assert_eq!(number(&mut interpreter, "Erased"), 0.0);
}

#[test]
fn invalid_local_dimensions_remain_lazy_and_restore_partially_bound_locals_when_called() {
    for (bound, expected) in [
        ("1+", ErrorCode::Syntax),
        ("\"wrong\"", ErrorCode::TypeMismatch),
        ("-1", ErrorCode::InvalidValue),
    ] {
        let mut interpreter = run(&format!(
            r#"10 X=99:S$="outer":DIM A(0):A(0)=88:Marker=1
20 END
100 DEF SUB Bad(P)
110 LOCAL X,S$,A(2),Failure({bound})
120 Marker=999
130 SUBEND
200 DEF SUB Good
210 Marker=Marker+1
220 SUBEND"#
        ));
        assert_eq!(number(&mut interpreter, "Marker"), 1.0, "{bound}");
        for _ in 0..2 {
            let error = interpreter.process_immediate("CALL Bad(3)").unwrap_err();
            assert_eq!(error.code, expected, "{bound}");
            assert_eq!(number(&mut interpreter, "X"), 99.0, "{bound}");
            assert_eq!(text(&mut interpreter, "S$"), "outer", "{bound}");
            assert_eq!(number(&mut interpreter, "UBOUND(A)"), 0.0, "{bound}");
            assert_eq!(number(&mut interpreter, "A(0)"), 88.0, "{bound}");
            assert_eq!(number(&mut interpreter, "Marker"), 1.0, "{bound}");
        }
        interpreter.process_immediate("CALL Good").unwrap();
        assert_eq!(number(&mut interpreter, "Marker"), 2.0, "{bound}");
    }
}

use avl_basic::expr::{compile_expression, eval_compiled, eval_compiled_number};
use avl_basic::{ErrorCode, Interpreter};

#[test]
fn scalar_mod_preserves_binary64_remainders_and_basic_errors_in_each_evaluator() {
    let expression = compile_expression("A MOD B").unwrap();
    let result_variable = compile_expression("R").unwrap();
    for (left, right, expected) in [
        ("-0", "3", Ok(-0.0_f64)),
        ("-6", "-1.5", Ok(-0.0)),
        ("6", "-1.5", Ok(0.0)),
        // The nearest binary64 representation of 0.2 is slightly greater than 1/5.
        ("5.5", "-0.2", Ok(0.0999999999999997)),
        // Construct both neighbors of the exact multiple 4 * 0.375. Immediate
        // mode rounds long decimal literals before the expression reaches MOD.
        ("1.5-2^(-52)", "0.375", Ok(0.375 - f64::EPSILON)),
        ("1.5+2^(-52)", "0.375", Ok(f64::EPSILON)),
        // Exponent differences of 63, 64, 127 and 128, on either side of each reduction.
        ("2^62", "0.75", Ok(0.25)),
        ("2^63", "0.75", Ok(0.5)),
        ("2^126", "0.75", Ok(0.25)),
        ("2^127", "0.75", Ok(0.5)),
        ("(2-2^(-52))*2^1023", "0.75", Ok(0.5)),
        ("-3*2^(-1074)", "2*2^(-1074)", Ok(-f64::from_bits(1))),
        ("-1.25", "INF", Ok(-1.25)),
        ("INF", "1", Err(ErrorCode::InvalidValue)),
        ("1", "-0", Err(ErrorCode::DivisionByZero)),
        ("INF", "0", Err(ErrorCode::DivisionByZero)),
    ] {
        let mut interpreter = Interpreter::new();
        let inputs = format!("A={left} : B={right}");
        interpreter.process_immediate(&inputs).unwrap();
        let value =
            eval_compiled(&mut interpreter, &expression).and_then(|value| value.as_number());
        let number = eval_compiled_number(&mut interpreter, &expression);
        let immediate = interpreter
            .process_immediate("R=A MOD B")
            .and_then(|()| eval_compiled_number(&mut interpreter, &result_variable));

        interpreter
            .process_immediate(&format!("10 {inputs}"))
            .unwrap();
        interpreter.process_immediate("20 R=A MOD B").unwrap();
        let run = interpreter.run_loaded();
        if let Err(error) = &run {
            assert_eq!(error.line, Some(20), "{left} MOD {right}");
        }
        let run = run.and_then(|_| eval_compiled_number(&mut interpreter, &result_variable));
        for (path, actual) in [
            ("value", value),
            ("number", number),
            ("immediate", immediate),
            ("RUN", run),
        ] {
            assert_eq!(
                actual.map(f64::to_bits).map_err(|error| error.code),
                expected.map(f64::to_bits),
                "{path}: {left} MOD {right}"
            );
        }
    }
}

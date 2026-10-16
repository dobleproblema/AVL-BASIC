use avl_basic::{ErrorCode, Interpreter};

fn program(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    for line in source.lines() {
        interpreter.process_immediate(line).unwrap();
    }
    interpreter
}

#[test]
fn interrupt_keywords_are_rejected_as_scalar_and_array_names() {
    for name in ["DI", "di", "Di", "EI", "ei", "Ei", "DI$", "eI$"] {
        let value = if name.ends_with('$') { "\"x\"" } else { "1" };
        for source in [
            format!("{name}={value}"),
            format!("LET {name}={value}"),
            format!("{name}(0)={value}"),
            format!("PRINT {name}"),
            format!("PRINT {name}(0)"),
        ] {
            let mut immediate = Interpreter::new();
            let error = immediate.process_immediate(&source).unwrap_err();
            assert_eq!(error.code, ErrorCode::Undefined, "{source}");

            let mut stored = program(&format!("10 {source}"));
            let error = stored.process_immediate("RUN").unwrap_err();
            assert_eq!(error.code, ErrorCode::Undefined, "{source}");
            assert_eq!(error.line, Some(10), "{source}");
        }
        for source in [format!("DIM {name}(1)"), format!("REDIM {name}(1)")] {
            let error = Interpreter::new().process_immediate(&source).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument, "{source}");
        }
    }
}

#[test]
fn interrupt_keywords_are_rejected_as_for_variables() {
    for name in ["DI", "di", "EI", "ei"] {
        let mut interpreter = program(&format!("10 FOR {name}=1 TO 2\n20 NEXT {name}\n30 END"));
        let error = interpreter.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{name}");
        assert_eq!(error.line, Some(10), "{name}");
    }
}

#[test]
fn interrupt_keywords_are_rejected_as_routine_parameters_and_locals() {
    for name in ["DI", "di", "EI", "ei", "DI$", "ei$"] {
        for definition in [
            format!("100 DEF SUB Test({name})\n110 SUBEND"),
            format!("100 DEF FNTest({name})=1"),
            format!("100 DEF FNTest({name})\n110 FNTest=1\n120 FNEND"),
            format!("100 DEF SUB Test\n110 LOCAL {name}\n120 SUBEND"),
            format!("100 DEF FNTest()\n110 LOCAL {name}\n120 FNTest=1\n130 FNEND"),
            format!("100 DEF SUB Test\n110 LOCAL {name}(1)\n120 SUBEND"),
        ] {
            let mut interpreter = program(&format!("10 END\n{definition}"));
            let error = interpreter.process_immediate("RUN").unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument, "{definition}");
        }
    }
}

#[test]
fn interrupt_commands_reject_trailing_arguments() {
    for source in ["DI = 1", "ei = 1", "Di 1", "EI nonsense"] {
        let mut immediate = Interpreter::new();
        let error = immediate.process_immediate(source).unwrap_err();
        assert_eq!(error.code, ErrorCode::Syntax, "{source}");

        let mut stored = program(&format!("10 {source}"));
        let error = stored.process_immediate("RUN").unwrap_err();
        assert_eq!(error.code, ErrorCode::Syntax, "{source}");
        assert_eq!(error.line, Some(10), "{source}");
    }
}

#[test]
fn keyword_prefixes_and_legacy_short_names_remain_valid() {
    let mut interpreter = program(
        "10 DiIndex=2:eiCount=3:DI1=5:EI1=7:DI_=11:EI_=13\n\
         20 AS=17:CD=19:SQ=29:AB=31:A1=37\n\
         30 DIM DiItems(0),EiItems$(0):DiItems(0)=41:EiItems$(0)=\"OK\"\n\
         40 CALL Combine(DiItems)\n\
         50 IF DiIndex+eiCount+DI1+EI1+DI_+EI_<>41 THEN PRINT \"BAD PREFIX\"\n\
         60 IF AS+CD+SQ+AB+A1<>133 THEN PRINT \"BAD LEGACY\"\n\
         70 PRINT DiItems(0);EiItems$(0):END\n\
         100 DEF SUB Combine(DiValues)\n\
         110 LOCAL EiDelta\n\
         120 EiDelta=1:DiValues(0)=DiValues(0)+EiDelta\n\
         130 SUBEND",
    );
    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(interpreter.take_output(), " 42 OK\n");
}

#[test]
fn bare_interrupt_commands_still_work_with_colons_and_comments() {
    let mut interpreter =
        program("10 di:Value=1:ei ' enable interrupts again\n20 DI:EI:PRINT Value\n30 END");
    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(interpreter.take_output(), " 1\n");
    interpreter.process_immediate("DI:EI").unwrap();
}

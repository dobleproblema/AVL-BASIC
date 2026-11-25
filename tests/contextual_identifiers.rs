use avl_basic::Interpreter;
use std::fs;

#[test]
fn contextual_syntax_and_identifiers_keep_their_case_in_both_orders() {
    for (name, syntax) in [
        ("full", "SCREEN FULL OFF"),
        ("sync", "SCREEN SYNC OFF"),
        ("del", "SPRITE DEL 1"),
        ("rgba", r#"BLOAD "asset.png",image$ AS RGBA"#),
        ("map", "MAT A=MAP(INT,B)"),
        ("solve", "MAT X=SOLVE(A,B)"),
        ("import", r#"IMPORT "lib.bas" AS helper"#),
        ("wheel", "ON MOUSE WHEEL GOSUB Handler"),
    ] {
        for syntax_first in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let mut interpreter = Interpreter::new();
            interpreter.root_dir = directory.path().to_path_buf();
            interpreter.current_dir = directory.path().to_path_buf();
            let syntax_line = format!("10 {}", syntax.to_ascii_lowercase());
            let variable_line = format!("20 {name}=1");
            let lines = if syntax_first {
                [&syntax_line, &variable_line]
            } else {
                [&variable_line, &syntax_line]
            };
            for line in lines {
                interpreter.process_immediate(line).unwrap();
            }
            let expected = format!(
                "10 {}\n20 {name}=1\n",
                avl_basic::console::normalize_code(&syntax.to_ascii_lowercase())
            );
            assert_eq!(
                interpreter.program.list(),
                expected,
                "{name}, syntax first {syntax_first}"
            );
            interpreter.process_immediate(r#"SAVE "case.bas""#).unwrap();
            assert_eq!(
                fs::read_to_string(directory.path().join("case.bas")).unwrap(),
                expected
            );
            interpreter.process_immediate("NEW").unwrap();
            interpreter.process_immediate(r#"LOAD "case.bas""#).unwrap();
            assert_eq!(interpreter.program.list(), expected, "{name}, after LOAD");
        }
    }
}

#[test]
fn compound_immediate_help_and_pack_do_not_define_variable_spelling() {
    for (command, names) in [
        ("HELP SCREEN FULL:PRINT 1", &["Help", "Full"][..]),
        (
            r#"PACK "missing.bas" TO "missing.exe" ID "x" INCLUDE "a" PERSIST "b" GUI:PRINT 1"#,
            &["Pack", "Id", "Include", "Persist", "Gui"][..],
        ),
    ] {
        let mut interpreter = Interpreter::new();
        let _ = interpreter.process_immediate(command);
        interpreter.take_output();
        for (index, name) in names.iter().enumerate() {
            let line = format!("{} {name}=1", (index + 1) * 10);
            interpreter.process_immediate(&line).unwrap();
            assert!(
                interpreter.program.list().contains(&line),
                "{command}, {name}, {:?}",
                interpreter.program.list()
            );
        }
    }
}

use avl_basic::expr::eval_expression;
use avl_basic::Interpreter;
use std::path::PathBuf;
use std::process::Command;

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
fn hp_blocks_rebase_and_vectors_bridge_rows_and_columns() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 1:DIM A(5,5),B(5,5),V(5)");
    for row in 1..=5 {
        for col in 1..=5 {
            command(
                &mut interpreter,
                &format!("A({row},{col})={}", 10 * row + col),
            );
        }
    }
    command(&mut interpreter, "MAT B=A(1:2,3:5)");
    assert_eq!(number(&mut interpreter, "UBOUND(B,1)"), 2.0);
    assert_eq!(number(&mut interpreter, "UBOUND(B,2)"), 3.0);
    assert_eq!(number(&mut interpreter, "B(1,1)"), 13.0);
    assert_eq!(number(&mut interpreter, "B(2,3)"), 25.0);
    assert_eq!(number(&mut interpreter, "B(0,0)"), 0.0);
    command(&mut interpreter, "MAT V=A(,2):MAT A(3,)=V");
    for col in 1..=5 {
        assert_eq!(
            number(&mut interpreter, &format!("A(3,{col})")),
            (10 * col + 2) as f64
        );
    }
}

#[test]
fn overlapping_copies_snapshot_source_before_resize_or_write() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM V(5)");
    for index in 0..=5 {
        command(&mut interpreter, &format!("V({index})={index}"));
    }
    command(&mut interpreter, "MAT V(1:5)=V(0:4)");
    for (index, expected) in [0, 0, 1, 2, 3, 4].iter().enumerate() {
        assert_eq!(
            number(&mut interpreter, &format!("V({index})")),
            *expected as f64
        );
    }
    command(&mut interpreter, "MAT V=V(2:4)");
    assert_eq!(number(&mut interpreter, "UBOUND(V)"), 2.0);
    for index in 0..=2 {
        assert_eq!(
            number(&mut interpreter, &format!("V({index})")),
            (index + 1) as f64
        );
    }
}

#[test]
fn reversed_hp_intervals_exclude_written_endpoints() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM V(3)");
    command(
        &mut interpreter,
        "V(0)=10:V(1)=11:V(2)=12:V(3)=13:MAT V=V(4:-1)",
    );
    for index in 0..=3 {
        assert_eq!(
            number(&mut interpreter, &format!("V({index})")),
            (13 - index) as f64
        );
    }
    command(&mut interpreter, "MAT V(1:0)=V(1:0)");
    assert_eq!(number(&mut interpreter, "V(0)"), 13.0);
    command(&mut interpreter, "MAT BASE 1:MAT V=V(1:0)");
    assert_eq!(number(&mut interpreter, "UBOUND(V)"), 0.0);
}

#[test]
fn invalid_copies_do_not_resize_or_partially_change_destination() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(3,3),B(2,2),S$(3,3):MAT A=3:MAT B=7",
    );
    for source in [
        "MAT B(0:1,0:1)=A(0:2,0:1)",
        "MAT B(0,0:2)=A(0:2,0)",
        "MAT B=A(0:4,0:1)",
        "MAT B=A(0:1)",
        "MAT B=S$(0:1,0:1)",
        "MAT B=A(1:0,0:1)",
        "MAT B=A(0:1,0:1)+A(0:1,0:1)",
    ] {
        assert!(interpreter.process_immediate(source).is_err(), "{source}");
        assert_eq!(number(&mut interpreter, "UBOUND(B,1)"), 2.0);
        assert_eq!(number(&mut interpreter, "UBOUND(B,2)"), 2.0);
        for row in 0..=2 {
            for col in 0..=2 {
                assert_eq!(
                    number(&mut interpreter, &format!("B({row},{col})")),
                    7.0,
                    "{source}"
                );
            }
        }
    }
    command(&mut interpreter, "MAT B=A(1,2)");
    for row in 0..=2 {
        for col in 0..=2 {
            assert_eq!(number(&mut interpreter, &format!("B({row},{col})")), 3.0);
        }
    }
}

#[test]
fn parameterless_array_functions_keep_working_with_full_and_selected_targets() {
    let mut interpreter = Interpreter::new();
    for line in "10 DIM A(2),B(1),C(3):A(0)=10:A(1)=11:A(2)=12:MAT C=99\n20 MAT B=FNCOPY()\n30 MAT C(1:3)=FNCOPY()\n40 END\n100 DEF FNCOPY()\n110 MAT FNCOPY=A\n120 FNEND".lines() {
        command(&mut interpreter, line);
    }
    interpreter.run_loaded().unwrap();
    assert_eq!(number(&mut interpreter, "UBOUND(B)"), 2.0);
    assert_eq!(number(&mut interpreter, "C(0)"), 99.0);
    for index in 0..=2 {
        assert_eq!(
            number(&mut interpreter, &format!("B({index})")),
            (10 + index) as f64
        );
        assert_eq!(
            number(&mut interpreter, &format!("C({})", index + 1)),
            (10 + index) as f64
        );
    }
}

#[test]
fn index_functions_snapshot_rhs_before_lhs_and_use_resized_arrays() {
    let mut interpreter = Interpreter::new();
    for line in "10 DIM A(3),B(3):ORDER=0\n20 FOR I=0 TO 3:A(I)=I+10:NEXT I\n30 MAT B(FNTARGET():3)=A(FNSOURCE():3)\n40 END\n100 DEF FNSOURCE()\n110 ORDER=ORDER*10+1:REDIM A(4):A(4)=50\n120 FNSOURCE=1\n130 FNEND\n200 DEF FNTARGET()\n210 ORDER=ORDER*10+2:A(1)=999:REDIM B(4):B(4)=777\n220 FNTARGET=1\n230 FNEND".lines() {
        command(&mut interpreter, line);
    }
    interpreter.run_loaded().unwrap();
    assert_eq!(number(&mut interpreter, "ORDER"), 12.0);
    assert_eq!(number(&mut interpreter, "A(1)"), 999.0);
    assert_eq!(number(&mut interpreter, "UBOUND(B)"), 4.0);
    assert_eq!(number(&mut interpreter, "B(4)"), 777.0);
    for index in 1..=3 {
        assert_eq!(
            number(&mut interpreter, &format!("B({index})")),
            (10 + index) as f64
        );
    }
}

#[test]
fn subarray_corpus_matches_both_runtimes_and_explicit_expected_results() {
    if std::env::var_os("AVL_BASIC_PY_REPO").is_none() {
        eprintln!("set AVL_BASIC_PY_REPO to enable MAT subarray Python parity");
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tools = PathBuf::from(
        std::env::var_os("AVL_BASIC_TOOLS_DIR")
            .expect("set AVL_BASIC_TOOLS_DIR to the local parity tools directory"),
    );
    let checker = tools.join("run_mat_subarray_parity.py");
    assert!(
        checker.is_file(),
        "Missing parity checker: {}",
        checker.display()
    );
    let output = Command::new(std::env::var("PYTHON").unwrap_or_else(|_| "python".into()))
        .arg(checker)
        .arg("--rust-bin")
        .arg(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_REPO", &root)
        .current_dir(root)
        .output()
        .expect("run MAT subarray parity checker");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

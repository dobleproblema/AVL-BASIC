use std::path::PathBuf;
use std::process::Command;

#[test]
fn immediate_loops_match_python_and_shared_expected_output() {
    if std::env::var_os("AVL_BASIC_PY_REPO").is_none() {
        eprintln!("set AVL_BASIC_PY_REPO to enable prompt-loop Python parity");
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tools = PathBuf::from(
        std::env::var_os("AVL_BASIC_TOOLS_DIR")
            .expect("set AVL_BASIC_TOOLS_DIR to the local parity tools directory"),
    );
    let checker = tools.join("run_immediate_loop_parity.py");
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
        .expect("run immediate-loop parity checker");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

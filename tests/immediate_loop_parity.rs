use std::path::PathBuf;
use std::process::Command;

#[test]
fn immediate_loops_match_python_and_shared_expected_output() {
    if std::env::var_os("AVL_BASIC_PY_REPO").is_none() {
        eprintln!("set AVL_BASIC_PY_REPO to enable prompt-loop Python parity");
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(std::env::var("PYTHON").unwrap_or_else(|_| "python".into()))
        .arg(root.join("tools/run_immediate_loop_parity.py"))
        .arg("--rust-bin")
        .arg(env!("CARGO_BIN_EXE_avl-basic"))
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

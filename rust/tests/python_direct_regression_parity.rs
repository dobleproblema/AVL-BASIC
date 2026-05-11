use std::path::PathBuf;
use std::process::Command;

fn python_command() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python".to_string())
}

fn rust_binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_avl-basic") {
        return PathBuf::from(path);
    }
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push(if cfg!(windows) {
        "avl-basic.exe"
    } else {
        "avl-basic"
    });
    path
}

#[test]
fn python_direct_non_graphics_regressions_match() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest_dir
        .join("tools")
        .join("run_python_direct_regression_parity.py");
    let output = Command::new(python_command())
        .arg(&script)
        .arg("--mode")
        .arg("all-text")
        .arg("--rust-bin")
        .arg(rust_binary())
        .current_dir(&manifest_dir)
        .output()
        .expect("failed to run direct Python regression parity cases");

    assert!(
        output.status.success(),
        "Direct Python regression parity cases failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

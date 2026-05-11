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
fn python_text_program_cases_can_be_extracted() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest_dir.join("tools").join("run_python_text_parity.py");
    let output = Command::new(python_command())
        .arg(&script)
        .arg("--mode")
        .arg("summary")
        .current_dir(&manifest_dir)
        .output()
        .expect("failed to run Python text parity summary");

    assert!(
        output.status.success(),
        "Python text parity summary failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn supported_python_text_program_cases_match() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest_dir.join("tools").join("run_python_text_parity.py");
    let output = Command::new(python_command())
        .arg(&script)
        .arg("--mode")
        .arg("supported")
        .arg("--rust-bin")
        .arg(rust_binary())
        .current_dir(&manifest_dir)
        .output()
        .expect("failed to run supported Python text parity cases");

    assert!(
        output.status.success(),
        "Supported Python text parity cases failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn all_python_text_program_cases_match() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest_dir.join("tools").join("run_python_text_parity.py");
    let output = Command::new(python_command())
        .arg(&script)
        .arg("--mode")
        .arg("all-text")
        .arg("--rust-bin")
        .arg(rust_binary())
        .current_dir(&manifest_dir)
        .output()
        .expect("failed to run all Python text parity cases");

    assert!(
        output.status.success(),
        "All Python text parity cases failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

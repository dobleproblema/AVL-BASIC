use std::path::PathBuf;
use std::process::Command;

fn python_command() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python".to_string())
}

fn parity_script(name: &str) -> PathBuf {
    let tools = PathBuf::from(
        std::env::var_os("AVL_BASIC_TOOLS_DIR")
            .expect("set AVL_BASIC_TOOLS_DIR to the local parity tools directory"),
    );
    let script = tools.join(name);
    assert!(
        script.is_file(),
        "Missing parity checker: {}",
        script.display()
    );
    script
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

fn skip_without_python_oracle() -> bool {
    if std::env::var_os("AVL_BASIC_PY_REPO").is_none() {
        eprintln!("skipping Python parity test; set AVL_BASIC_PY_REPO to enable it");
        return true;
    }
    false
}

#[test]
fn python_graphics_parity_summary_can_be_extracted() {
    if skip_without_python_oracle() {
        return;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = parity_script("run_python_graphics_parity.py");
    let output = Command::new(python_command())
        .arg(&script)
        .arg("--mode")
        .arg("summary")
        .env("AVL_BASIC_REPO", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("failed to run Python graphics parity summary");

    assert!(
        output.status.success(),
        "Python graphics parity summary failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "opens the Python Tk graphics window; run during graphics work"]
fn python_graphics_smoke_and_direct_cases_match() {
    if skip_without_python_oracle() {
        return;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = parity_script("run_python_graphics_parity.py");
    let output = Command::new(python_command())
        .arg(&script)
        .arg("--mode")
        .arg("all")
        .arg("--rust-bin")
        .arg(rust_binary())
        .env("AVL_BASIC_REPO", &manifest_dir)
        .current_dir(&manifest_dir)
        .output()
        .expect("failed to run Python graphics parity cases");

    assert!(
        output.status.success(),
        "Python graphics parity cases failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

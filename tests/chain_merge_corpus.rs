//! The same fixtures as Python's test_chain_merge.py, through the real prompt.
//! This test always runs; it does not require an optional Python oracle.
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BEGIN: &str = "__CHAIN_TEST_BEGIN__";
const END: &str = "__CHAIN_TEST_END__";

fn sections(path: &Path) -> BTreeMap<String, String> {
    let source = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    let mut sections = BTreeMap::new();
    let mut current = None;
    for line in source.split_inclusive('\n') {
        let header = line.trim_end_matches('\n');
        if header.starts_with('[') && header.ends_with(']') {
            let name = header[1..header.len() - 1].to_owned();
            assert!(sections.insert(name.clone(), String::new()).is_none());
            current = Some(name);
        } else if let Some(name) = &current {
            sections.get_mut(name).unwrap().push_str(line);
        } else {
            assert!(line.trim().is_empty() || line.starts_with('#'));
        }
    }
    assert!(sections.contains_key("commands") && sections.contains_key("output"));
    sections
}

fn child_path(root: &Path, name: &str) -> std::path::PathBuf {
    assert!(Path::new(name)
        .components()
        .all(|part| matches!(part, Component::Normal(_))));
    root.join(name)
}

fn normalize(output: &str) -> String {
    assert_eq!(output.matches(BEGIN).count(), 1, "{output}");
    assert_eq!(output.matches(END).count(), 1, "{output}");
    let body = output
        .split_once(BEGIN)
        .unwrap()
        .1
        .split_once(END)
        .unwrap()
        .0;
    body.lines()
        .filter(|line| line.trim() != "Ready")
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn run_case(path: &Path) -> Result<(), String> {
    let spec = sections(path);
    let directory = tempfile::tempdir().unwrap();
    for (header, content) in &spec {
        if let Some(name) = header
            .strip_prefix("input ")
            .or_else(|| header.strip_prefix("input-hex "))
        {
            let target = child_path(directory.path(), name);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let bytes = if header.starts_with("input-hex ") {
                let hex: String = content.chars().filter(|c| !c.is_whitespace()).collect();
                assert_eq!(hex.len() % 2, 0);
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect()
            } else {
                content.as_bytes().to_vec()
            };
            std::fs::write(target, bytes).unwrap();
        } else {
            assert!(header == "commands" || header == "output" || header.starts_with("expect "));
        }
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .current_dir(directory.path())
        .env("AVL_BASIC_PRINT_ZONE_DEFAULT", "8")
        .env("AVL_BASIC_WINDOW", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let commands = format!(
        "PRINT \"{BEGIN}\"\n{}\nNEW\nPRINT \"{END}\"\nEXIT\n",
        spec["commands"]
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(commands.as_bytes())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("session timed out".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    if !result.status.success() || !result.stderr.is_empty() {
        return Err(format!(
            "{}\n{}",
            stdout,
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let actual = normalize(&stdout);
    if actual != spec["output"].trim() {
        return Err(format!(
            "expected {:?}\nactual   {:?}",
            spec["output"].trim(),
            actual
        ));
    }
    for (header, content) in &spec {
        if let Some(name) = header.strip_prefix("expect ") {
            let actual = std::fs::read(child_path(directory.path(), name))
                .map_err(|e| format!("{name}: {e}"))?;
            if actual != content.as_bytes() {
                return Err(format!("different file bytes: {name}"));
            }
        }
    }
    Ok(())
}

#[test]
fn chain_merge_shared_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chain_merge");
    let mut cases = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "case")
        })
        .collect::<Vec<_>>();
    cases.sort();
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    for case in &cases {
        if let Err(error) = run_case(case) {
            failures.push(format!(
                "{}: {error}",
                case.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    eprintln!("{} shared CHAIN/MERGE cases passed", cases.len());
}

#[test]
fn chain_merge_corpus_matches_python_checkout() {
    let Some(python_root) = std::env::var_os("AVL_BASIC_PY_REPO") else {
        return; // The Rust contract test above always runs, without Python.
    };
    fn corpus(root: &Path) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(root.join("tests/fixtures/chain_merge"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "case")
            })
            .map(|path| {
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read(path).unwrap(),
                )
            })
            .collect()
    }
    assert_eq!(
        corpus(Path::new(env!("CARGO_MANIFEST_DIR"))),
        corpus(Path::new(&python_root)),
        "The shared CHAIN/MERGE fixtures have drifted between repositories"
    );
}

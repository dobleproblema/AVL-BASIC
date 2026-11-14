#![cfg(all(
    target_os = "windows",
    target_arch = "x86_64",
    target_feature = "crt-static"
))]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn pack(project: &Path, options: &str) -> String {
    let mut interpreter = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .current_dir(project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let command = format!(
        "PACK \"main.bas\" TO \"deliver/game.exe\" INCLUDE \"assets/\" PERSIST \"assets/score.csv\" ID \"pack.integration\" {options}\nEXIT\n"
    );
    interpreter
        .stdin
        .take()
        .unwrap()
        .write_all(command.as_bytes())
        .unwrap();
    let output = interpreter.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn windows_subsystem(executable: &Path) -> u16 {
    let bytes = fs::read(executable).unwrap();
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    let subsystem = pe + 24 + 68;
    u16::from_le_bytes(bytes[subsystem..subsystem + 2].try_into().unwrap())
}

#[test]
fn pack_checks_resources_and_runs_with_only_the_executable() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let assets = project.join("assets");
    let delivery = project.join("deliver");
    let profile = temporary.path().join("profile");
    fs::create_dir_all(&assets).unwrap();
    fs::create_dir(&delivery).unwrap();
    fs::write(assets.join("score.csv"), "999\n").unwrap();
    fs::write(
        project.join("main.bas"),
        "10 OPEN \"assets/required.csv\" FOR INPUT AS #1\n\
         20 INPUT #1,R\n\
         30 CLOSE #1\n\
         40 ON ERROR GOTO 100\n\
         50 OPEN \"assets/score.csv\" FOR INPUT AS #1\n\
         60 INPUT #1,S\n\
         70 CLOSE #1\n\
         80 PRINT \"REOPENED\";S\n\
         90 END\n\
         100 RESUME 110\n\
         110 ON ERROR GOTO 0\n\
         120 OPEN \"assets/score.csv\" FOR OUTPUT AS #1\n\
         130 WRITE #1,R\n\
         140 CLOSE #1\n\
         150 PRINT \"SAVED\";R\n\
         160 END\n",
    )
    .unwrap();

    let executable = delivery.join("game.exe");
    let diagnostic = pack(&project, "");
    assert!(
        diagnostic.contains("main.bas") && diagnostic.contains("assets/required.csv"),
        "{diagnostic}"
    );
    assert!(!executable.exists());

    fs::write(assets.join("required.csv"), "42\n").unwrap();
    let diagnostic = pack(&project, r#"TARGET "windows-x64""#);
    assert!(
        diagnostic.contains(r#"Unknown PACK option "TARGET"."#),
        "{diagnostic}"
    );
    assert!(!executable.exists());

    let diagnostic = pack(&project, "");
    assert!(diagnostic.contains("PACK created"), "{diagnostic}");
    assert_eq!(windows_subsystem(&executable), 3);
    let isolated = temporary.path().join("isolated");
    fs::rename(&delivery, &isolated).unwrap();
    let executable = isolated.join("game.exe");
    assert_eq!(fs::read_dir(&isolated).unwrap().count(), 1);

    for expected in ["SAVED42", "REOPENED42"] {
        let output = Command::new(&executable)
            .current_dir(&isolated)
            .env("PATH", "")
            .env("LOCALAPPDATA", &profile)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.split_whitespace().collect::<String>(), expected);
    }
    let score = profile.join("AVL BASIC/pack.integration/assets/score.csv");
    assert_eq!(fs::read_to_string(score).unwrap().trim(), "42");
    assert_eq!(
        fs::read_to_string(assets.join("score.csv")).unwrap().trim(),
        "999"
    );

    fs::create_dir(&delivery).unwrap();
    let diagnostic = pack(&project, "GUI");
    assert!(diagnostic.contains("PACK created"), "{diagnostic}");
    let executable = delivery.join("game.exe");
    assert_eq!(windows_subsystem(&executable), 2);
    let gui_profile = temporary.path().join("gui-profile");
    let output = Command::new(&executable)
        .current_dir(&delivery)
        .env("PATH", "")
        .env("LOCALAPPDATA", &gui_profile)
        .env("AVL_BASIC_WINDOW", "0")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let score = gui_profile.join("AVL BASIC/pack.integration/assets/score.csv");
    assert_eq!(fs::read_to_string(score).unwrap().trim(), "42");
}

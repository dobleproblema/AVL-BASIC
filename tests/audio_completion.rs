use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn launch(directory: &Path, program: Option<&Path>) -> (Process, Receiver<String>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_avl-basic"));
    command
        .current_dir(directory)
        .env("AVL_BASIC_AUDIO", "off")
        .env("AVL_BASIC_WINDOW", "off")
        .env("AVL_BASIC_COLOR", "never")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(program) = program {
        command.arg(program);
    }
    let mut child = command.spawn().unwrap();
    let output = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    (Process(child), receiver)
}

fn await_line(output: &Receiver<String>, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let line = output
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|error| panic!("waiting for {expected:?}: {error}"));
        if line.trim() == expected {
            break;
        }
    }
}

fn await_exit(process: &mut Process) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            return status.success();
        }
        assert!(Instant::now() < deadline, "interpreter did not exit");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cli_keeps_final_classic_and_sample_audio_alive_after_basic_has_finished() {
    let directory = tempfile::tempdir().unwrap();
    // Use the bundled decoder fixture at a slower rate so the sample's tail
    // outlasts observing BODY FINISHED even on a heavily loaded test machine.
    fs::write(
        directory.path().join("chime.wav"),
        include_bytes!("../samples/assets/audio/chime.wav"),
    )
    .unwrap();
    for source in [
        "10 SOUND 1,142,100\n20 PRINT \"BODY FINISHED\"\n30 END",
        "10 SOUND 1,142,100\n20 PRINT \"BODY FINISHED\"",
        "10 AUDIO LOAD 1,\"chime.wav\"\n20 AUDIO PLAY 1,1,0,1,0,0.5\n\
         30 PRINT \"BODY FINISHED\"\n40 END",
    ] {
        let program = directory.path().join("tail.bas");
        fs::write(&program, source).unwrap();
        let (mut process, output) = launch(directory.path(), Some(&program));
        await_line(&output, "BODY FINISHED");
        thread::sleep(Duration::from_millis(100));
        assert!(process.0.try_wait().unwrap().is_none(), "{source}");
        assert!(await_exit(&mut process), "{source}");
    }
}

#[test]
fn repl_returns_ready_with_pending_audio_and_accepts_channel_flush() {
    let directory = tempfile::tempdir().unwrap();
    let (mut process, output) = launch(directory.path(), None);
    await_line(&output, "Ready");
    let input = process.0.stdin.as_mut().unwrap();
    input
        .write_all(b"10 SOUND 1,248,30000\n20 END\nRUN\n")
        .unwrap();
    input.flush().unwrap();
    await_line(&output, "Ready");
    let input = process.0.stdin.as_mut().unwrap();
    input
        .write_all(b"IF (SQ(1) AND 128)<>0 THEN PRINT \"STILL PLAYING\"\n")
        .unwrap();
    input.flush().unwrap();
    await_line(&output, "STILL PLAYING");
    await_line(&output, "Ready");
    let input = process.0.stdin.as_mut().unwrap();
    input
        .write_all(b"SOUND 1+128,0\nPAUSE 250\nIF SQ(1)=4 THEN PRINT \"QUEUE CLEARED\"\nEXIT\n")
        .unwrap();
    input.flush().unwrap();
    await_line(&output, "QUEUE CLEARED");
    assert!(await_exit(&mut process));
}

#[test]
fn cli_stop_and_unhandled_error_do_not_wait_for_long_audio() {
    let directory = tempfile::tempdir().unwrap();
    for (ending, success) in [("STOP", true), ("ERROR 26", false)] {
        let program = directory.path().join("cancel.bas");
        fs::write(&program, format!("10 SOUND 1,248,30000\n20 {ending}")).unwrap();
        let (mut process, _) = launch(directory.path(), Some(&program));
        assert_eq!(await_exit(&mut process), success);
    }
}

use avl_basic::{console, ErrorCode, Interpreter};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

// This test has its own binary because the real Ctrl+C flag is process-wide.
#[test]
fn ctrl_c_received_during_an_immediate_loop_is_consumed_and_keeps_its_file_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("progress.txt");
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = dir.path().to_path_buf();
    interpreter.current_dir = dir.path().to_path_buf();
    interpreter
        .process_immediate("OPEN \"progress.txt\" FOR OUTPUT AS #1")
        .unwrap();
    console::clear_interrupt_requested();
    let watcher_path = path.clone();
    let request = thread::spawn(move || {
        let start = Instant::now();
        while fs::metadata(&watcher_path).unwrap().len() == 0 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "loop never wrote its first record"
            );
            thread::sleep(Duration::from_millis(1));
        }
        console::request_interrupt_for_test();
    });
    // Finite upper bound also lets the test fail without hanging if polling
    // regresses. The request is sent only after the loop has entered its body.
    let result = interpreter
        .process_immediate("A=0:WHILE A<100000:A=A+1:WRITE #1,A:WEND:WRITE #1,\"UNREACHABLE\"");
    request.join().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.code, ErrorCode::KeyboardInterrupt);
    assert_eq!(error.line, None);
    assert!(!console::interrupt_requested());
    interpreter
        .process_immediate("WRITE #1,\"AFTER\":CLOSE #1")
        .unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("1\n"), "{text}");
    assert!(text.ends_with("\"AFTER\"\n"), "{text}");
    assert!(!text.contains("UNREACHABLE"));
    interpreter
        .process_immediate("10 PRINT \"RUN OK\"")
        .unwrap();
    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(interpreter.take_output(), "RUN OK\n");

    let empty_loop_path = dir.path().join("empty-loop.txt");
    interpreter
        .process_immediate("OPEN \"empty-loop.txt\" FOR OUTPUT AS #1")
        .unwrap();
    let request = thread::spawn(move || {
        let start = Instant::now();
        while fs::metadata(&empty_loop_path).unwrap().len() == 0 {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(1));
        }
        // Give WEND time to repeat itself; without the empty-body fix this
        // statement returns before the interrupt and the assertion fails.
        thread::sleep(Duration::from_millis(10));
        console::request_interrupt_for_test();
    });
    let result =
        interpreter.process_immediate("WRITE #1,\"ENTERED\":WHILE 1:WEND:PRINT \"UNREACHABLE\"");
    request.join().unwrap();
    assert_eq!(result.unwrap_err().code, ErrorCode::KeyboardInterrupt);
    assert!(!console::interrupt_requested());
    assert_eq!(interpreter.take_output(), "");
    interpreter
        .process_immediate("CLOSE #1:PRINT \"READY AGAIN\"")
        .unwrap();
    assert_eq!(interpreter.take_output(), "READY AGAIN\n");
}

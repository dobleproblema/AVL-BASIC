use super::*;

fn headless_interpreter() -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = false;
    interpreter
}

#[test]
fn sync_preserves_canvas_and_survives_resets_until_close() {
    let mut interpreter = headless_interpreter();
    assert!(!interpreter.graphics_sync_enabled);
    interpreter
        .process_immediate("MODE 1280:PLOT 10,20,7")
        .unwrap();
    let pixels = interpreter.graphics.buffer().to_vec();
    interpreter
        .process_immediate("SCREEN FULL ON:SCREEN SYNC ON")
        .unwrap();
    assert!(interpreter.graphics_sync_enabled);
    assert_eq!(interpreter.graphics.buffer(), pixels);
    interpreter
        .process_immediate("SCREEN FULL OFF:SCREEN")
        .unwrap();
    assert!(!interpreter.requested_graphics_fullscreen());
    assert!(interpreter.graphics_sync_enabled);
    assert_eq!(interpreter.graphics.buffer(), pixels);
    interpreter.process_immediate("MODE 960").unwrap();
    assert!(interpreter.graphics_sync_enabled);
    interpreter.process_immediate("SCREEN FULL ON").unwrap();
    interpreter
        .process_immediate("SCREEN FULL ON:SCREEN CLOSE")
        .unwrap();
    assert!(!interpreter.graphics_sync_enabled);
    assert!(!interpreter.requested_graphics_fullscreen());
}

#[test]
fn invalid_sync_and_fullscreen_options_do_not_change_presentation_state() {
    let mut interpreter = headless_interpreter();
    interpreter
        .process_immediate("SCREEN FULL ON:SCREEN SYNC ON")
        .unwrap();
    for command in [
        "SCREEN SYNC",
        "SCREEN SYNC 1",
        "SCREEN SYNC ON OFF",
        "SCREEN SYNC REPORT",
        "SCREEN SYNC ON,720",
        "SCREEN FULL ON,720",
        "SCREEN FULL ON,1080",
        "SCREEN FULL ON,0",
        "SCREEN FULL ON,720.5",
        "SCREEN FULL OFF,720",
        "SCREEN FULL ON,720,1",
        "SCREEN FULL ON,",
        "SCREEN FULL ON,INF",
    ] {
        interpreter.process_immediate(command).expect_err(command);
        assert!(interpreter.graphics_sync_enabled, "{command}");
        assert!(interpreter.requested_graphics_fullscreen(), "{command}");
    }
    interpreter.process_immediate("SCREEN SYNC OFF").unwrap();
    assert!(!interpreter.graphics_sync_enabled);
    assert!(interpreter.requested_graphics_fullscreen());
}

#[test]
fn sync_is_a_screen_option_not_a_new_reserved_variable_and_frame_remains_compatible() {
    let mut interpreter = headless_interpreter();
    interpreter
        .program
        .load_text(
            "10 SYNC=42:MODE 1280:INK RGB(255,255,255):PLOT 10,20:SYNC$=SCREEN$\n\
         20 SCREEN FULL ON:SCREEN SYNC ON:FRAME:FRAME 60\n\
         30 CLG OFFSCREEN:SCREEN SYNC$:SCREEN SYNC OFF:FRAME\n\
         40 END",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.numeric_variables.get("SYNC"), Some(&42.0));
    assert!(!interpreter.graphics_sync_enabled);
    assert_eq!(
        interpreter.graphics.buffer()[(720 - 1 - 20) * 1280 + 10],
        0xFFFFFF
    );
}

fn dimensions(interpreter: &Interpreter) -> (usize, usize) {
    (interpreter.graphics.width, interpreter.graphics.height)
}

#[test]
fn mouse_visibility_is_preserved_by_mode_screen_and_fullscreen_until_close() {
    let mut interpreter = headless_interpreter();
    assert!(interpreter.graphics_mouse_cursor_visible);
    for command in ["MOUSE OFF", "MOUSE 0", "MOUSE OFF"] {
        interpreter.process_immediate(command).unwrap();
        assert!(!interpreter.graphics_mouse_cursor_visible, "{command}");
    }
    for command in [
        "MODE 960",
        "SCREEN FULL ON",
        "SCREEN FULL OFF",
        "SCREEN",
        "MODE 1280",
    ] {
        interpreter.process_immediate(command).unwrap();
        assert!(!interpreter.graphics_mouse_cursor_visible, "{command}");
    }
    for command in ["MOUSE", "MOUSE 2", "MOUSE -1", "MOUSE 0.5", "MOUSE OFF,ON"] {
        interpreter.process_immediate(command).expect_err(command);
        assert!(!interpreter.graphics_mouse_cursor_visible, "{command}");
    }
    for command in ["MOUSE ON", "MOUSE 1", "MOUSE ON"] {
        interpreter.process_immediate(command).unwrap();
        assert!(interpreter.graphics_mouse_cursor_visible, "{command}");
    }
    interpreter
        .process_immediate("MOUSE OFF:SCREEN CLOSE")
        .unwrap();
    assert!(interpreter.graphics_mouse_cursor_visible);
}

#[test]
fn low_widescreen_survives_screen_reset_and_close_while_mode_640_stays_legacy() {
    let mut interpreter = headless_interpreter();
    interpreter.process_immediate("MODE 640,360").unwrap();
    assert_eq!(dimensions(&interpreter), (640, 360));
    interpreter
        .process_immediate("INK 7:PLOT 639,359:MOVE 23,42")
        .unwrap();
    let image = interpreter.graphics.capture_screen();
    assert!(image.starts_with("640x360:"));
    interpreter.process_immediate("SCREEN").unwrap();
    assert_eq!(dimensions(&interpreter), (640, 360));
    assert_eq!(interpreter.graphics.capture_screen(), image);
    assert_eq!(interpreter.graphics.xpos(), 0.0);
    assert_eq!(interpreter.graphics.ypos(), 0.0);

    interpreter.process_immediate("SCREEN FULL ON").unwrap();
    assert!(interpreter.requested_graphics_fullscreen());
    interpreter.process_immediate("SCREEN CLOSE").unwrap();
    assert_eq!(dimensions(&interpreter), (640, 360));
    assert_eq!(interpreter.graphics.buffer().len(), 640 * 360);
    assert!(interpreter
        .graphics
        .buffer()
        .iter()
        .all(|pixel| *pixel == 0));
    assert!(!interpreter.requested_graphics_fullscreen());
    assert!(!interpreter.graphics_fullscreen_requested);

    interpreter.process_immediate("MODE 640").unwrap();
    assert_eq!(dimensions(&interpreter), (640, 480));
    assert_eq!(interpreter.graphics.buffer().len(), 640 * 480);
}

#[test]
fn mode_accepts_dimension_expressions_and_preserves_all_legacy_dimensions() {
    let mut interpreter = headless_interpreter();
    interpreter
        .process_immediate("W=320:H=180:MODE W*2,H*2")
        .unwrap();
    assert_eq!(dimensions(&interpreter), (640, 360));
    for (command, expected) in [
        ("MODE 640", (640, 480)),
        ("MODE 800", (800, 600)),
        ("MODE 960", (960, 540)),
        ("MODE 1024", (1024, 768)),
        ("MODE 1280", (1280, 720)),
        ("MODE 1280,720", (1280, 720)),
        ("MODE 640,480", (640, 480)),
        ("MODE 800,600", (800, 600)),
        ("MODE 960,540", (960, 540)),
        ("MODE 1024,768", (1024, 768)),
    ] {
        interpreter.process_immediate(command).unwrap();
        assert_eq!(dimensions(&interpreter), expected, "{command}");
    }

    interpreter
        .program
        .load_text("10 W=1280:H=720\n20 MODE W/2,H/2\n30 END")
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(dimensions(&interpreter), (640, 360));
    interpreter
        .program
        .load_text("10 W=1280:H=720\n20 MODE W,H\n30 PLOT 1279,719,7\n40 SCREEN\n50 END")
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(dimensions(&interpreter), (1280, 720));
}

#[test]
fn invalid_mode_dimensions_do_not_change_canvas_pixels_or_presentation_request() {
    let mut interpreter = headless_interpreter();
    interpreter
        .process_immediate("MODE 640,360:PLOT 639,359,7:SCREEN FULL ON")
        .unwrap();
    interpreter
        .numeric_variables
        .insert("BADWIDTH".into(), f64::NAN);
    interpreter
        .numeric_variables
        .insert("BADHEIGHT".into(), f64::NAN);
    let before = interpreter.graphics.buffer().to_vec();
    for command in [
        "MODE 640,480.5",
        "MODE 640.5,360",
        "MODE 640,0",
        "MODE 0,360",
        "MODE -640,360",
        "MODE 640,-360",
        "MODE 800,360",
        "MODE 960,600",
        "MODE 1280,768",
        "MODE 1920,1080",
        "MODE INF,360",
        "MODE 640,INF",
        "MODE BADWIDTH,360",
        "MODE 640,BADHEIGHT",
    ] {
        let error = interpreter.process_immediate(command).expect_err(command);
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{command}");
        assert_eq!(dimensions(&interpreter), (640, 360), "{command}");
        assert_eq!(interpreter.graphics.buffer(), before, "{command}");
        assert!(interpreter.requested_graphics_fullscreen(), "{command}");
    }
    for command in ["MODE", "MODE 640,", "MODE ,360", "MODE 640,360,1"] {
        interpreter.process_immediate(command).expect_err(command);
        assert_eq!(dimensions(&interpreter), (640, 360), "{command}");
        assert_eq!(interpreter.graphics.buffer(), before, "{command}");
    }
}

#[test]
fn screen_full_changes_only_presentation_in_immediate_and_program_execution() {
    let mut interpreter = headless_interpreter();
    interpreter
        .process_immediate("MODE 640,360:PLOT 639,359,7:MOVE 12,34")
        .unwrap();
    let before = interpreter.graphics.buffer().to_vec();
    for command in ["SCREEN FULL ON", "screen full on", "SCREEN FULL OFF"] {
        interpreter.process_immediate(command).unwrap();
        assert_eq!(dimensions(&interpreter), (640, 360));
        assert_eq!(interpreter.graphics.buffer(), before);
        assert_eq!(interpreter.graphics.xpos(), 12.0);
        assert_eq!(interpreter.graphics.ypos(), 34.0);
        assert_eq!(
            interpreter.requested_graphics_fullscreen(),
            command.to_ascii_uppercase().ends_with(" ON")
        );
        assert!(interpreter.graphics_window.is_none());
    }

    interpreter
        .program
        .load_text("10 SCREEN FULL ON\n20 END")
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert!(interpreter.requested_graphics_fullscreen());
    assert_eq!(interpreter.graphics.buffer(), before);
    interpreter
        .program
        .load_text("10 SCREEN FULL OFF\n20 END")
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert!(!interpreter.requested_graphics_fullscreen());
    assert_eq!(interpreter.graphics.buffer(), before);
}

#[test]
fn screen_full_requires_on_or_off_and_does_not_change_state_on_invalid_input() {
    let mut interpreter = headless_interpreter();
    interpreter
        .process_immediate("MODE 640,360:PLOT 20,30,7:SCREEN FULL ON")
        .unwrap();
    let before = interpreter.graphics.buffer().to_vec();
    for command in [
        "SCREEN FULL",
        "SCREEN FULL 1",
        "SCREEN FULL ON OFF",
        "SCREEN FULL ON,OFF",
        "SCREEN FULL OFF EXTRA",
    ] {
        interpreter.process_immediate(command).expect_err(command);
        assert!(interpreter.requested_graphics_fullscreen(), "{command}");
        assert_eq!(dimensions(&interpreter), (640, 360), "{command}");
        assert_eq!(interpreter.graphics.buffer(), before, "{command}");
    }
}

#[test]
fn full_remains_a_variable_and_full_string_can_restore_a_captured_screen() {
    let mut interpreter = headless_interpreter();
    interpreter
        .process_immediate("MODE 640,360:FULL=42:PLOT 639,359,7:FULL$=SCREEN$")
        .unwrap();
    let image = interpreter.graphics.capture_screen();
    assert_eq!(interpreter.numeric_variables.get("FULL"), Some(&42.0));
    interpreter.process_immediate("CLG OFFSCREEN").unwrap();
    assert_ne!(interpreter.graphics.capture_screen(), image);
    interpreter.process_immediate("SCREEN FULL$").unwrap();
    assert_eq!(interpreter.graphics.capture_screen(), image);
    assert!(!interpreter.requested_graphics_fullscreen());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
#[ignore = "Opens native windows to verify MOUSE visibility state across recreation; Windows also enters fullscreen"]
fn native_mouse_visibility_survives_window_recreation() {
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = true;
    interpreter
        .process_immediate("MODE 640,360:MOUSE OFF")
        .unwrap();
    let fullscreen = if cfg!(windows) {
        "SCREEN FULL ON"
    } else {
        "SCREEN FULL OFF"
    };
    for command in [
        "MODE 960",
        "SCREEN",
        fullscreen,
        "MODE 1280",
        "SCREEN FULL OFF",
    ] {
        interpreter.process_immediate(command).unwrap();
        assert!(!interpreter.graphics_mouse_cursor_visible, "{command}");
        assert!(
            !interpreter
                .graphics_window
                .as_ref()
                .unwrap()
                .mouse_cursor_visible(),
            "{command}"
        );
    }
    interpreter.process_immediate("MOUSE ON").unwrap();
    assert!(interpreter
        .graphics_window
        .as_ref()
        .unwrap()
        .mouse_cursor_visible());
    interpreter
        .process_immediate("MOUSE OFF:SCREEN CLOSE:MODE 640")
        .unwrap();
    assert!(interpreter.graphics_mouse_cursor_visible);
    assert!(interpreter
        .graphics_window
        .as_ref()
        .unwrap()
        .mouse_cursor_visible());
    interpreter.process_immediate("SCREEN CLOSE").unwrap();
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
#[ignore = "Opens a native window to check SYNC toggles, FRAME, MODE and close/reopen; Windows also exercises fullscreen"]
fn native_sync_survives_mode_reset_and_reopen() {
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = true;
    let presentation = if cfg!(windows) {
        "SCREEN FULL ON"
    } else {
        // WSLg fullscreen is not representative of the accepted native Ubuntu run.
        "SCREEN FULL OFF"
    };
    interpreter
        .program
        .load_text(&format!(
            "10 MODE 640,360:{presentation}:SCREEN SYNC ON\n\
         20 FOR I=1 TO 12\n\
         30 CLG OFFSCREEN:FRECTANGLE I*20,50,I*20+12,300,7:FRAME\n\
         40 NEXT I\n\
         50 MODE 1280:SCREEN:FRAME 60\n\
         60 SCREEN SYNC OFF:FRAME:SCREEN SYNC ON:FRAME\n\
         70 SCREEN CLOSE\n\
         80 MODE 960:SCREEN SYNC ON:FRAME\n\
         90 SCREEN CLOSE\n\
         100 END"
        ))
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert!(interpreter.graphics_window.is_none());
    assert!(!interpreter.graphics_sync_enabled);
    assert_eq!(dimensions(&interpreter), (960, 540));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
#[ignore = "Opens and changes a native graphics window; run explicitly with an available desktop"]
fn native_program_presentation_preserves_mode_and_reveals_console_on_exit() {
    fn assert_native_state(interpreter: &mut Interpreter, expected: bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            interpreter.pump_graphics_window_now().unwrap();
            let window = interpreter.graphics_window.as_ref().unwrap();
            if window.is_fullscreen() == expected
                && (expected || window.physical_size() == dimensions(interpreter))
                && !window.presentation_needs_refresh()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native presentation did not settle"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            interpreter
                .graphics_window
                .as_ref()
                .unwrap()
                .is_fullscreen(),
            expected
        );
    }

    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = true;
    interpreter.process_immediate("MODE 640,360").unwrap();
    interpreter.process_immediate("SCREEN FULL ON").unwrap();
    assert!(interpreter.requested_graphics_fullscreen());
    assert_native_state(&mut interpreter, true);
    assert!(!interpreter
        .graphics_window
        .as_ref()
        .unwrap()
        .presentation_needs_refresh());
    interpreter.process_immediate("MODE 640").unwrap();
    assert_eq!(dimensions(&interpreter), (640, 480));
    assert!(interpreter.requested_graphics_fullscreen());
    assert_native_state(&mut interpreter, true);
    interpreter.process_immediate("SCREEN").unwrap();
    assert!(interpreter.requested_graphics_fullscreen());
    assert_native_state(&mut interpreter, true);

    interpreter.prepare_console_io();
    assert_native_state(&mut interpreter, false);
    assert!(interpreter.requested_graphics_fullscreen());
    interpreter.pump_graphics_window_now().unwrap();
    assert_native_state(&mut interpreter, false);
    interpreter.current_line = Some(10);
    interpreter.graphics_window_used_this_run = true;
    interpreter.read_inkey();
    assert_native_state(&mut interpreter, true);
    interpreter.prepare_console_io();
    interpreter.key_down(32);
    assert_native_state(&mut interpreter, true);
    interpreter.prepare_console_io();
    interpreter.pause_user_input_ready().unwrap();
    assert_native_state(&mut interpreter, true);
    interpreter.prepare_console_io();
    interpreter
        .execute_frame(
            "",
            &mut Cursor {
                line_idx: 0,
                cmd_idx: 0,
            },
        )
        .unwrap();
    assert_native_state(&mut interpreter, true);
    interpreter.current_line = None;
    interpreter.process_immediate("SCREEN FULL OFF").unwrap();
    assert_native_state(&mut interpreter, false);
    assert!(!interpreter.requested_graphics_fullscreen());
    assert_eq!(
        interpreter
            .graphics_window
            .as_ref()
            .unwrap()
            .physical_size(),
        (640, 480)
    );
    interpreter.process_immediate("SCREEN CLOSE").unwrap();

    for (ending, should_error) in [("END", false), ("STOP", false), ("A=1/0", true)] {
        interpreter
            .program
            .load_text(&format!("10 MODE 640,360\n20 SCREEN FULL ON\n30 {ending}"))
            .unwrap();
        let result = interpreter.run_loaded();
        assert_eq!(result.is_err(), should_error, "{ending}: {result:?}");
        assert_native_state(&mut interpreter, false);
        assert!(interpreter.requested_graphics_fullscreen(), "{ending}");
        interpreter.pump_graphics_window_now().unwrap();
        assert_native_state(&mut interpreter, false);
        interpreter.process_immediate("SCREEN CLOSE").unwrap();
        assert!(!interpreter.requested_graphics_fullscreen(), "{ending}");
    }
}

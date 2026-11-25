use super::*;

#[test]
fn queued_mouse_wheel_keeps_each_delta_and_position_until_its_handler() {
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = false;
    interpreter
        .process_immediate("DIM Wx(2), Wy(2), Delta(2)")
        .unwrap();
    interpreter
        .program
        .load_text(
            r#"100 RecordWheel:
110 N=N+1
120 Delta(N)=MOUSEWHEEL : Wx(N)=MOUSEX : Wy(N)=MOUSEY
130 EventName$=MOUSEEVENT$
140 RETURN
200 RecordClick:
210 Clicks=Clicks+1 : ClickWheel=MOUSEWHEEL
220 RETURN"#,
        )
        .unwrap();
    interpreter.rebuild_command_cache();
    interpreter
        .execute_on_mouse("ON MOUSE WHEEL GOSUB RecordWheel")
        .unwrap();
    interpreter
        .execute_on_mouse("ON MOUSE LEFTDOWN GOSUB RecordClick")
        .unwrap();
    interpreter.current_line = Some(10);
    interpreter.run_depth = 1;
    interpreter.graphics_window_used_this_run = true;
    interpreter.interrupts_enabled = false;

    let wheel_events = [
        MouseWheelEvent {
            x: 10,
            y: 20,
            delta: 1.5,
        },
        MouseWheelEvent {
            x: 30,
            y: 40,
            delta: -2.0,
        },
    ];
    interpreter.queue_mouse_wheel_events(wheel_events.to_vec());
    interpreter.update_mouse_state(MouseSnapshot {
        x: 50,
        y: 60,
        left: true,
        event: "LEFTDOWN".to_string(),
        ..MouseSnapshot::default()
    });
    interpreter.process_mouse_event().unwrap();
    assert_eq!(interpreter.mouse_event_queue.len(), 3);
    assert_eq!(interpreter.eval_number("MOUSEWHEEL").unwrap(), 0.0);

    interpreter.interrupts_enabled = true;
    for event in wheel_events {
        interpreter.process_mouse_event().unwrap();
        let (x, y) = interpreter
            .graphics
            .logical_screen_to_user(event.x, event.y);
        assert_eq!(interpreter.eval_number("Delta(N)").unwrap(), event.delta);
        assert_eq!(interpreter.eval_number("Wx(N)").unwrap(), x);
        assert_eq!(interpreter.eval_number("Wy(N)").unwrap(), y);
        assert_eq!(
            interpreter
                .string_variables
                .get("EVENTNAME$")
                .map(String::as_str),
            Some("WHEEL")
        );
        assert_eq!(interpreter.eval_number("MOUSEWHEEL").unwrap(), 0.0);
    }
    interpreter.process_mouse_event().unwrap();
    assert_eq!(interpreter.eval_number("Clicks").unwrap(), 1.0);
    assert_eq!(interpreter.eval_number("ClickWheel").unwrap(), 0.0);

    interpreter.queue_mouse_wheel_events(wheel_events.to_vec());
    interpreter
        .execute_on_mouse("ON MOUSE WHEEL GOSUB 0")
        .unwrap();
    assert!(interpreter.mouse_event_queue.is_empty());
}

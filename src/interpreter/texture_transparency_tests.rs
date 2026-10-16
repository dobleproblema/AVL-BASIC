//! Colour-key contracts shared by the affine and depth-tested texture commands.
use super::*;

const TEXEL: &str = "1x1:c08040";
const FORMS: [(&str, &str); 8] = [
    ("TRECTANGLE", "T$,10,10,110,110"),
    ("TRECTANGLE", "T$,10,10,110,110,0,0,4,4"),
    ("TTRIANGLE", "T$,10,10,0,0,110,10,4,0,10,110,0,4"),
    ("TTRIANGLE", "T$,D,10,10,.5,0,0,110,10,1,4,0,10,110,.25,0,4"),
    ("TTRIANGLE", "T$,D,10,10,.5,0,0,255,128,0,110,10,1,4,0,255,128,0,10,110,.25,0,4,255,128,0"),
    ("TQUAD", "T$,10,10,0,0,110,10,4,0,110,110,4,4,10,110,0,4"),
    ("TQUAD", "T$,D,10,10,.5,0,0,110,10,1,4,0,110,110,1,4,4,10,110,.25,0,4"),
    ("TQUAD", "T$,D,10,10,.5,0,0,255,128,0,110,10,1,4,0,255,128,0,110,110,1,4,4,255,128,0,10,110,.25,0,4,255,128,0"),
];

fn setup() -> String {
    format!("MODE 640:DIM D(639,479):T$=\"{TEXEL}\":PLOT 7,8")
}

fn headless() -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = false;
    interpreter.process_immediate(&setup()).unwrap();
    interpreter
}

fn execute_cached(interpreter: &mut Interpreter, command: &CachedCommand) -> BasicResult<()> {
    match command {
        CachedCommand::TexturedRect(compiled) => {
            interpreter.execute_compiled_textured_rect(compiled)
        }
        CachedCommand::TexturedTriangle(compiled) => {
            interpreter.execute_compiled_textured_triangle(compiled)
        }
        CachedCommand::TexturedQuad(compiled) => {
            interpreter.execute_compiled_textured_quad(compiled)
        }
        _ => panic!("valid texture form did not compile: {command:?}"),
    }
}

fn depth_values(interpreter: &Interpreter, name: &str) -> Vec<f64> {
    match &interpreter.array_ref(name).unwrap().data {
        ArrayData::Number(values) => values.clone(),
        ArrayData::Str(_) => panic!("expected numeric depth array"),
    }
}

fn snapshot(interpreter: &Interpreter) -> (Vec<u32>, Vec<f64>, f64, f64) {
    (
        interpreter.graphics.buffer().to_vec(),
        depth_values(interpreter, "D"),
        interpreter.graphics.xpos(),
        interpreter.graphics.ypos(),
    )
}

#[test]
fn every_keyed_form_matches_immediate_compiled_and_run_execution() {
    for (keyword, arguments) in FORMS {
        let source = format!("{keyword} {arguments},RGB(192,128,64)");
        let mut immediate = headless();
        let original_pixels = immediate.graphics.buffer().to_vec();
        immediate.process_immediate(&source).unwrap();
        assert_eq!(immediate.graphics.buffer(), original_pixels, "{source}");
        assert!(depth_values(&immediate, "D").iter().all(|q| *q == 0.0));
        assert_eq!(
            (immediate.graphics.xpos(), immediate.graphics.ypos()),
            (10.0, 110.0)
        );

        let mut cached = headless();
        let compiled = cached.compile_cached_command(&source);
        execute_cached(&mut cached, &compiled).unwrap();

        let mut stored = Interpreter::new();
        stored.graphics_window_enabled = false;
        stored
            .program
            .load_text(&format!("10 {}\n20 {source}\n30 END", setup()))
            .unwrap();
        stored.run_loaded().unwrap();
        assert_eq!(snapshot(&immediate), snapshot(&cached), "{source}");
        assert_eq!(snapshot(&immediate), snapshot(&stored), "{source}");
    }
}

#[test]
fn cached_transparency_expressions_follow_changes_without_recompilation() {
    for (keyword, arguments) in FORMS {
        let mut interpreter = headless();
        interpreter
            .process_immediate("KEY=RGB(192,128,64):KEY$=\"#c08040\"")
            .unwrap();
        for key_expression in ["KEY", "KEY+0", "KEY$"] {
            let source = format!("{keyword} {arguments},{key_expression}");
            let compiled = interpreter.compile_cached_command(&source);
            execute_cached(&mut interpreter, &compiled).unwrap();
            assert_eq!(interpreter.graphics.test(30.0, 30.0), 0, "{source}");
            interpreter
                .process_immediate("KEY=RGB(18,52,86):KEY$=\"#123456\"")
                .unwrap();
            execute_cached(&mut interpreter, &compiled).unwrap();
            assert_ne!(interpreter.graphics.test(30.0, 30.0), 0, "{source}");
            interpreter
                .process_immediate("CLG:MAT D=0:KEY=RGB(192,128,64):KEY$=\"#c08040\"")
                .unwrap();
        }
    }
}

#[test]
fn omitted_key_keeps_every_original_form_opaque() {
    for (keyword, arguments) in FORMS {
        let mut original = headless();
        original
            .process_immediate(&format!("{keyword} {arguments}"))
            .unwrap();
        assert_ne!(original.graphics.test(30.0, 30.0), 0);
        let mut unmatched = headless();
        unmatched
            .process_immediate(&format!("{keyword} {arguments},RGB(18,52,86)"))
            .unwrap();
        assert_eq!(
            snapshot(&original),
            snapshot(&unmatched),
            "{keyword} {arguments}"
        );
    }
}

#[test]
fn palette_rgb_hex_and_named_keys_have_sprite_colour_semantics() {
    for key in ["2", "RGB(255,0,0)", "\"#f00\"", "\"red\"", "\"255,0,0\""] {
        let mut sprite = headless();
        sprite
            .process_immediate(&format!("SPRITE \"1x1:ff0000\",30,30,{key}"))
            .unwrap();
        for (keyword, arguments) in FORMS {
            let mut interpreter = headless();
            interpreter.process_immediate("T$=\"1x1:ff0000\"").unwrap();
            let compiled =
                interpreter.compile_cached_command(&format!("{keyword} {arguments},{key}"));
            execute_cached(&mut interpreter, &compiled).unwrap();
            assert_eq!(
                interpreter.graphics.test(30.0, 30.0),
                sprite.graphics.test(30.0, 30.0)
            );
            assert_eq!(interpreter.graphics.test(30.0, 30.0), 0, "{keyword}, {key}");
            assert!(depth_values(&interpreter, "D").iter().all(|q| *q == 0.0));
        }
    }
}

#[test]
fn invalid_late_keys_are_atomic_and_keep_the_current_line() {
    for (keyword, arguments) in FORMS {
        for (key, expected) in [
            ("1/0", ErrorCode::DivisionByZero),
            ("-1", ErrorCode::InvalidArgument),
            ("16777216", ErrorCode::InvalidArgument),
            ("\"#badcolour\"", ErrorCode::InvalidArgument),
        ] {
            for cached in [false, true] {
                let mut interpreter = headless();
                interpreter.process_immediate("D(30,30)=.125").unwrap();
                interpreter.current_line = Some(240);
                let before = snapshot(&interpreter);
                let source = format!("{keyword} {arguments},{key}");
                let error = if cached {
                    let compiled = interpreter.compile_cached_command(&source);
                    execute_cached(&mut interpreter, &compiled).unwrap_err()
                } else {
                    interpreter.process_immediate(&source).unwrap_err()
                };
                assert_eq!(error.code, expected, "{source}; cached={cached}");
                assert_eq!(
                    error.line,
                    if cached { Some(240) } else { None },
                    "{source}; cached={cached}"
                );
                assert_eq!(snapshot(&interpreter), before, "{source}; cached={cached}");
            }
        }
    }
}

#[test]
fn malformed_key_arity_and_empty_fields_do_not_draw() {
    for (keyword, arguments) in FORMS {
        for source in [
            format!("{keyword} {arguments},0,0"),
            format!("{keyword} {arguments},"),
            format!("{keyword} {arguments},,0"),
        ] {
            for cached in [false, true] {
                let mut interpreter = headless();
                let before = snapshot(&interpreter);
                let error = if cached {
                    interpreter
                        .program
                        .load_text(&format!("10 {}\n20 {source}", setup()))
                        .unwrap();
                    interpreter.run_loaded().unwrap_err()
                } else {
                    interpreter.process_immediate(&source).unwrap_err()
                };
                assert_eq!(
                    error.code,
                    ErrorCode::ArgumentMismatch,
                    "{source}; cached={cached}"
                );
                assert_eq!(snapshot(&interpreter), before, "{source}; cached={cached}");
            }
        }
    }
}

#[test]
fn texture_vertices_and_key_evaluate_once_in_left_to_right_source_order() {
    for (keyword, arguments) in FORMS {
        for cached in [false, true] {
            let mut interpreter = headless();
            interpreter.program.load_text(
                "10 ORDER$=\"\":CALLS=0:END\n\
                 100 DEF FNTEXTURE$()\n110 ORDER$=ORDER$+\"T\"\n120 FNTEXTURE$=T$\n130 FNEND\n\
                 200 DEF FNARG(N,V)\n210 CALLS=CALLS+1:ORDER$=ORDER$+CHR$(64+N)\n220 FNARG=V\n230 FNEND\n\
                 300 DEF FNKEY()\n310 ORDER$=ORDER$+\"K\":CALLS=CALLS+1\n320 FNKEY=RGB(192,128,64)\n330 FNEND"
            ).unwrap();
            interpreter.run_loaded().unwrap();
            interpreter.end_requested = false;
            interpreter
                .process_immediate(&format!("DIM D(639,479):T$=\"{TEXEL}\""))
                .unwrap();
            let mut parts = split_arguments(arguments);
            parts[0] = "FNTEXTURE$()".to_owned();
            let start = if arguments.contains(",D,") { 2 } else { 1 };
            let count = parts.len() - start;
            for (index, value) in parts.iter_mut().skip(start).enumerate() {
                *value = format!("FNARG({},{value})", index + 1);
            }
            let source = format!("{keyword} {},FNKEY()", parts.join(","));
            let compiled = interpreter.compile_cached_command(&source);
            assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&0.0));
            if cached {
                execute_cached(&mut interpreter, &compiled).unwrap();
            } else {
                interpreter.process_immediate(&source).unwrap();
            }
            let expected = format!(
                "T{}K",
                (1..=count)
                    .map(|i| (64 + i as u8) as char)
                    .collect::<String>()
            );
            assert_eq!(
                interpreter
                    .string_variables
                    .get("ORDER$")
                    .map(String::as_str),
                Some(expected.as_str()),
                "{source}"
            );
            assert_eq!(
                interpreter.numeric_variables.get("CALLS"),
                Some(&((count + 1) as f64)),
                "{source}"
            );
            assert_eq!(interpreter.graphics.test(30.0, 30.0), 0);
        }
    }
}

#[test]
fn a_key_function_can_redim_aliased_depth_before_final_resolution() {
    for (keyword, arguments) in FORMS
        .into_iter()
        .filter(|(_, arguments)| arguments.contains(",D,"))
    {
        let mut interpreter = headless();
        let arguments = arguments.replacen(",D,", ",P,", 1);
        interpreter.program.load_text(&format!(
            "10 MODE 640:DIM D(2,2),E(2,2):T$=\"{TEXEL}\":CALLS=0\n\
             20 DEF SUB DRAWTEXTURE(P)\n30 {keyword} {arguments},FNKEY(P)\n40 SUBEND\n\
             50 CALL DRAWTEXTURE(D):CALL DRAWTEXTURE(E)\n60 END\n\
             100 DEF FNKEY(A)\n110 REDIM A(639,479):CALLS=CALLS+1\n120 FNKEY=RGB(18,52,86)\n130 FNEND"
        )).unwrap();
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&2.0));
        for name in ["D", "E"] {
            let array = interpreter.array_ref(name).unwrap();
            assert_eq!(array.dims, vec![639, 479]);
            assert!(
                array.get_number(&[30, 30]).unwrap() > 0.0,
                "{keyword}, {name}"
            );
        }
        assert!(interpreter.array_aliases.is_empty());
        assert!(!interpreter.array_exists("P"));
        assert!(!interpreter.array_exists("A"));
    }
}

#[test]
fn cached_keyed_draws_follow_depth_alias_rebinding_and_key_changes() {
    for (keyword, arguments) in FORMS
        .into_iter()
        .filter(|(_, arguments)| arguments.contains(",D,"))
    {
        let mut interpreter = headless();
        interpreter
            .process_immediate("DIM E(639,479):KEY=RGB(192,128,64)")
            .unwrap();
        interpreter.set_array_alias_binding("P", "D".to_owned());
        let source = format!("{keyword} {},KEY", arguments.replacen(",D,", ",P,", 1));
        let compiled = interpreter.compile_cached_command(&source);
        execute_cached(&mut interpreter, &compiled).unwrap();
        assert!(depth_values(&interpreter, "D").iter().all(|q| *q == 0.0));
        interpreter.set_array_alias_binding("P", "E".to_owned());
        interpreter.process_immediate("KEY=0").unwrap();
        execute_cached(&mut interpreter, &compiled).unwrap();
        assert!(depth_values(&interpreter, "D").iter().all(|q| *q == 0.0));
        assert!(
            interpreter
                .array_ref("E")
                .unwrap()
                .get_number(&[30, 30])
                .unwrap()
                > 0.0
        );
        interpreter.process_immediate("REDIM E(2,2)").unwrap();
        let before = snapshot(&interpreter);
        assert_eq!(
            execute_cached(&mut interpreter, &compiled)
                .unwrap_err()
                .code,
            ErrorCode::InvalidValue
        );
        assert_eq!(snapshot(&interpreter), before);
        assert_eq!(depth_values(&interpreter, "E"), vec![0.0; 9]);
    }
}

#[test]
fn transparent_texture_sample_compares_the_same_four_affine_shapes() {
    let source = include_str!("../../samples/g-texture-transparent.bas")
        .replace("FRAME 60", "FRAME")
        .replace("440 IF INKEY$<>CHR$(27) THEN GOTO 430", "440 GOTO 450")
        .replace("SCREEN CLOSE : END", "END");
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = false;
    interpreter.program.load_text(&source).unwrap();
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    for (left, right) in [(16, 160), (176, 320), (336, 480), (496, 624)] {
        let mut holes = 0;
        for y in 256..368 {
            for x in left..right {
                let opaque = interpreter.graphics.test(x as f64, y as f64);
                let transparent = interpreter.graphics.test(x as f64, (y - 192) as f64);
                if opaque == 0xff00ff {
                    let background = if (x / 16 + y / 16) % 2 == 0 {
                        0x203041
                    } else {
                        0x334758
                    };
                    assert_eq!(transparent, background);
                    holes += 1;
                } else {
                    assert_eq!(transparent, opaque);
                }
            }
        }
        assert!(holes > 1500, "column {left}..{right}");
    }
}

#[test]
fn transparent_cube_sample_runs_both_modes_and_reveals_the_far_depth() {
    let source = include_str!("../../samples/g-cube-transparent.bas")
        .replace("MAXFRAMES=0", "MAXFRAMES=2")
        .replace("FIXEDDT=0", "FIXEDDT=1/60")
        .replace("FRAME 60", "FRAME")
        .replace("SCREEN CLOSE : END", "END");
    let mut background_pixels = Vec::new();
    for cutout in [0, 1] {
        let mut interpreter = Interpreter::new();
        interpreter.graphics_window_enabled = false;
        interpreter
            .program
            .load_text(&source.replace("CUTOUT=1", &format!("CUTOUT={cutout}")))
            .unwrap();
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
        assert_eq!(interpreter.numeric_variables.get("FRAMEI"), Some(&2.0));
        let depth = depth_values(&interpreter, "D");
        background_pixels.push(depth.iter().filter(|q| (**q - 0.05).abs() < 1e-12).count());
        assert!(depth.iter().any(|q| *q > 0.2));
    }
    assert!(background_pixels[1] > background_pixels[0] + 1000);
}

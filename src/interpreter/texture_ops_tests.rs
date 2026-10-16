//! Texture statement contracts across immediate, cached, and stored execution.
use super::*;

const TEXTURE: &str = "1x1:c08040";
const TRIANGLE_AFFINE: &str = "T$,10,10,0,0,110,10,4,0,10,110,0,4";
const TRIANGLE_SHORT: &str = "T$,D,10,10,.5,0,0,110,10,1,4,0,10,110,.25,0,4";
const TRIANGLE_FULL: &str =
    "T$,D,10,10,.5,0,0,255,128,0,110,10,1,4,0,255,128,0,10,110,.25,0,4,255,128,0";
const QUAD_AFFINE: &str = "T$,10,10,0,0,110,10,4,0,110,110,4,4,10,110,0,4";
const QUAD_SHORT: &str = "T$,D,10,10,.5,0,0,110,10,1,4,0,110,110,1,4,4,10,110,.25,0,4";
const QUAD_FULL: &str = "T$,D,10,10,.5,0,0,255,128,0,110,10,1,4,0,255,128,0,110,110,1,4,4,255,128,0,10,110,.25,0,4,255,128,0";

fn headless() -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.graphics_window_enabled = false;
    interpreter
        .process_immediate(&format!("MODE 640:DIM D(639,479):T$=\"{TEXTURE}\""))
        .unwrap();
    interpreter
}

fn execute_cached(interpreter: &mut Interpreter, command: &CachedCommand) -> BasicResult<()> {
    match command {
        CachedCommand::TexturedTriangle(compiled) => {
            interpreter.execute_compiled_textured_triangle(compiled)
        }
        CachedCommand::TexturedQuad(compiled) => {
            interpreter.execute_compiled_textured_quad(compiled)
        }
        _ => panic!("texture statement was not compiled"),
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
fn all_texture_forms_match_in_immediate_cached_and_run_execution() {
    for (keyword, arguments, color, has_depth) in [
        ("TTRIANGLE", TRIANGLE_AFFINE, 0xc08040, false),
        ("TTRIANGLE", TRIANGLE_SHORT, 0xc08040, true),
        ("TTRIANGLE", TRIANGLE_FULL, 0xc04000, true),
        ("TQUAD", QUAD_AFFINE, 0xc08040, false),
        ("TQUAD", QUAD_SHORT, 0xc08040, true),
        ("TQUAD", QUAD_FULL, 0xc04000, true),
    ] {
        let source = format!("{keyword} {arguments}");
        let mut immediate = headless();
        immediate.process_immediate(&source).unwrap();

        let mut cached = headless();
        let compiled = cached.compile_cached_command(&source);
        execute_cached(&mut cached, &compiled).unwrap();

        let mut stored = headless();
        stored
            .program
            .load_text(&format!(
                "10 MODE 640:DIM D(639,479):T$=\"{TEXTURE}\"\n20 {source}\n30 END"
            ))
            .unwrap();
        stored.run_loaded().unwrap();

        assert_eq!(snapshot(&immediate), snapshot(&cached), "{source}");
        assert_eq!(snapshot(&immediate), snapshot(&stored), "{source}");
        assert_eq!(immediate.graphics.test(30.0, 30.0), color, "{source}");
        assert_eq!(immediate.graphics.xpos(), 10.0, "{source}");
        assert_eq!(immediate.graphics.ypos(), 110.0, "{source}");
        let depth = immediate
            .array_ref("D")
            .unwrap()
            .get_number(&[30, 30])
            .unwrap();
        assert_eq!(depth > 0.0, has_depth, "{source}");
    }
}

#[test]
fn omitted_and_explicit_white_lighting_preserve_texels_at_extreme_depth() {
    for (keyword, vertices) in [
        (
            "TTRIANGLE",
            vec!["10,10,1E307,0,0", "110,10,1E307,0,0", "10,110,1E307,0,0"],
        ),
        (
            "TQUAD",
            vec![
                "10,10,1E307,0,0",
                "110,10,1E307,0,0",
                "110,110,1E307,0,0",
                "10,110,1E307,0,0",
            ],
        ),
    ] {
        let mut frames = Vec::new();
        for explicit_rgb in [false, true] {
            let arguments = vertices
                .iter()
                .map(|vertex| {
                    if explicit_rgb {
                        format!("{vertex},255,255,255")
                    } else {
                        (*vertex).to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            let mut interpreter = headless();
            let source = format!("{keyword} T$,D,{arguments}");
            let compiled = interpreter.compile_cached_command(&source);
            execute_cached(&mut interpreter, &compiled).unwrap();
            // q and UV are valid; the unused product 255*q would overflow.
            assert_eq!(interpreter.graphics.test(30.0, 30.0), 0xc08040);
            frames.push(snapshot(&interpreter));
        }
        assert_eq!(frames[0], frames[1], "{keyword}: white is an identity");
    }
}

#[test]
fn cached_texture_sources_and_coordinates_follow_runtime_changes() {
    for source in ["T$", "T$+\"\"", "T$(0)"] {
        let mut interpreter = headless();
        interpreter
            .process_immediate(&format!("DIM T$(0):T$(0)=\"{TEXTURE}\":X=10"))
            .unwrap();
        let statement = format!("TTRIANGLE {source},D,X,10,1,0,0,X+100,10,1,0,0,X,110,1,0,0");
        let compiled = interpreter.compile_cached_command(&statement);
        execute_cached(&mut interpreter, &compiled).unwrap();
        assert_eq!(interpreter.graphics.test(30.0, 30.0), 0xc08040);

        interpreter
            .process_immediate("T$=\"1x1:20a0e0\":T$(0)=T$:MAT D=0:CLG:X=210")
            .unwrap();
        execute_cached(&mut interpreter, &compiled).unwrap();
        assert_eq!(interpreter.graphics.test(230.0, 30.0), 0x20a0e0, "{source}");
        assert_eq!(interpreter.graphics.test(30.0, 30.0), 0, "{source}");
        assert_eq!(interpreter.graphics.xpos(), 210.0);
    }
}

#[test]
fn cached_depth_aliases_follow_rebinding_and_redim() {
    for (keyword, arguments) in [("TTRIANGLE", TRIANGLE_SHORT), ("TQUAD", QUAD_SHORT)] {
        let mut interpreter = headless();
        interpreter.process_immediate("DIM E(639,479)").unwrap();
        interpreter.set_array_alias_binding("P", "D".to_owned());
        let source = format!("{keyword} {}", arguments.replacen(",D,", ",P,", 1));
        let compiled = interpreter.compile_cached_command(&source);
        execute_cached(&mut interpreter, &compiled).unwrap();
        assert!(
            interpreter
                .array_ref("D")
                .unwrap()
                .get_number(&[30, 30])
                .unwrap()
                > 0.0
        );

        interpreter.set_array_alias_binding("P", "E".to_owned());
        execute_cached(&mut interpreter, &compiled).unwrap();
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
        let error = execute_cached(&mut interpreter, &compiled).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidValue);
        assert_eq!(snapshot(&interpreter), before);
        assert_eq!(depth_values(&interpreter, "E"), vec![0.0; 9]);

        interpreter.process_immediate("REDIM E(639,479)").unwrap();
        execute_cached(&mut interpreter, &compiled).unwrap();
        assert!(
            interpreter
                .array_ref("E")
                .unwrap()
                .get_number(&[30, 30])
                .unwrap()
                > 0.0
        );
    }
}

#[test]
fn invalid_depth_arrays_keep_error_lines_and_graphics_unchanged() {
    for (keyword, arguments) in [("TTRIANGLE", TRIANGLE_SHORT), ("TQUAD", QUAD_SHORT)] {
        for (setup, name, expected) in [
            ("", "MISSING", ErrorCode::InvalidArgument),
            ("DIM P(639)", "P", ErrorCode::InvalidIndex),
            ("DIM P(638,479)", "P", ErrorCode::InvalidValue),
            ("DIM P(639,478)", "P", ErrorCode::InvalidValue),
        ] {
            let mut interpreter = headless();
            if !setup.is_empty() {
                interpreter.process_immediate(setup).unwrap();
            }
            interpreter.process_immediate("PLOT 7,8").unwrap();
            interpreter.current_line = Some(240);
            let source = format!(
                "{keyword} {}",
                arguments.replacen(",D,", &format!(",{name},"), 1)
            );
            let compiled = interpreter.compile_cached_command(&source);
            let before = snapshot(&interpreter);
            let error = execute_cached(&mut interpreter, &compiled).unwrap_err();
            assert_eq!(error.code, expected, "{source}");
            assert_eq!(error.line, Some(240), "{source}");
            assert_eq!(snapshot(&interpreter), before, "{source}");
        }

        let mut interpreter = headless();
        interpreter.process_immediate("DIM S$(639,479)").unwrap();
        interpreter.set_array_alias_binding("P", "S$".to_owned());
        interpreter.current_line = Some(250);
        let compiled = interpreter.compile_cached_command(&format!(
            "{keyword} {}",
            arguments.replacen(",D,", ",P,", 1)
        ));
        let before = snapshot(&interpreter);
        let error = execute_cached(&mut interpreter, &compiled).unwrap_err();
        assert_eq!(error.code, ErrorCode::TypeMismatch);
        assert_eq!(error.line, Some(250));
        assert_eq!(snapshot(&interpreter), before);
    }
}

#[test]
fn texture_syntax_rejects_partial_vertices_empty_fields_and_depth_expressions() {
    for (is_quad, arguments) in [(false, TRIANGLE_SHORT), (true, QUAD_SHORT)] {
        let compile = |source: &str| {
            if is_quad {
                compile_textured_quad_statement(source).map(|_| ())
            } else {
                compile_textured_triangle_statement(source).map(|_| ())
            }
        };
        for source in [
            format!("{arguments},0,0"),
            arguments.rsplit_once(',').unwrap().0.to_owned(),
            arguments.replacen(",10,", ",,", 1),
            arguments.replacen(",D,", ",,", 1),
        ] {
            assert_eq!(
                compile(&source).unwrap_err().code,
                ErrorCode::ArgumentMismatch,
                "{source}"
            );
        }
        for name in ["D(0,0)", "D+1"] {
            let source = arguments.replacen(",D,", &format!(",{name},"), 1);
            assert_eq!(
                compile(&source).unwrap_err().code,
                ErrorCode::Syntax,
                "{source}"
            );
        }
        let source = arguments.replacen(",D,", ",D$,", 1);
        assert_eq!(compile(&source).unwrap_err().code, ErrorCode::TypeMismatch);
    }
}

#[test]
fn late_numeric_failures_do_not_partially_draw_or_move_cursor() {
    for (keyword, arguments) in [("TTRIANGLE", TRIANGLE_FULL), ("TQUAD", QUAD_FULL)] {
        let mut interpreter = headless();
        interpreter
            .process_immediate("PLOT 7,8:D(30,30)=.125")
            .unwrap();
        interpreter.current_line = Some(260);
        let source = format!("{keyword} {},1/0", arguments.rsplit_once(',').unwrap().0);
        let compiled = interpreter.compile_cached_command(&source);
        let before = snapshot(&interpreter);
        let error = execute_cached(&mut interpreter, &compiled).unwrap_err();
        assert_eq!(error.code, ErrorCode::DivisionByZero);
        assert_eq!(error.line, Some(260));
        assert_eq!(snapshot(&interpreter), before);
    }

    // Every expression is finite, but the final vertex's q*u overflows.
    // The first half of a quad must not draw before that failure is detected.
    let mut interpreter = headless();
    interpreter.process_immediate("PLOT 7,8").unwrap();
    interpreter.current_line = Some(270);
    let compiled = interpreter.compile_cached_command(
        "TQUAD T$,D,10,10,1,0,0,110,10,1,0,0,110,110,1,0,0,10,110,1E308,1E308,0",
    );
    let before = snapshot(&interpreter);
    let error = execute_cached(&mut interpreter, &compiled).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(error.line, Some(270));
    assert_eq!(snapshot(&interpreter), before);
}

#[test]
fn texture_and_vertex_expressions_run_once_in_source_order() {
    for cached in [false, true] {
        let mut interpreter = headless();
        interpreter.program.load_text(
            "10 ORDER$=\"\":CALLS=0:END\n\
             100 DEF FNTEXTURE$()\n110 ORDER$=ORDER$+\"T\"\n120 FNTEXTURE$=T$\n130 FNEND\n\
             200 DEF FNARG(N,V)\n210 CALLS=CALLS+1:ORDER$=ORDER$+CHR$(64+N)\n220 FNARG=V\n230 FNEND"
        ).unwrap();
        interpreter.run_loaded().unwrap();
        interpreter.end_requested = false;
        // RUN clears variables, so initialize the graphics inputs afterwards.
        interpreter
            .process_immediate(&format!("DIM D(639,479):T$=\"{TEXTURE}\""))
            .unwrap();
        let values = [
            10.0, 10.0, 0.5, 0.0, 0.0, 110.0, 10.0, 1.0, 4.0, 0.0, 10.0, 110.0, 0.25, 0.0, 4.0,
        ];
        let arguments = values
            .iter()
            .enumerate()
            .map(|(index, value)| format!("FNARG({},{value})", index + 1))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!("TTRIANGLE FNTEXTURE$(),D,{arguments}");
        let compiled = interpreter.compile_cached_command(&source);
        assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&0.0));
        assert_eq!(
            interpreter
                .string_variables
                .get("ORDER$")
                .map(String::as_str),
            Some("")
        );
        if cached {
            execute_cached(&mut interpreter, &compiled).unwrap();
        } else {
            interpreter.process_immediate(&source).unwrap();
        }
        assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&15.0));
        assert_eq!(
            interpreter
                .string_variables
                .get("ORDER$")
                .map(String::as_str),
            Some("TABCDEFGHIJKLMNO")
        );
        assert_eq!(interpreter.graphics.test(30.0, 30.0), 0xc08040);
    }
}

#[test]
fn run_sub_depth_parameters_are_resolved_after_expression_redim() {
    for (keyword, arguments) in [("TTRIANGLE", TRIANGLE_SHORT), ("TQUAD", QUAD_SHORT)] {
        let mut interpreter = headless();
        let arguments = arguments.replacen(",D,10,", ",P,FNRESIZE(P),", 1);
        interpreter
            .program
            .load_text(&format!(
                "10 MODE 640:DIM D(2,2),E(2,2):T$=\"{TEXTURE}\":CALLS=0\n\
             20 DEF SUB DRAWTEXTURE(P)\n30 {keyword} {arguments}\n40 SUBEND\n\
             50 CALL DRAWTEXTURE(D):CALL DRAWTEXTURE(E)\n60 END\n\
             100 DEF FNRESIZE(A)\n110 REDIM A(639,479):CALLS=CALLS+1\n\
             120 FNRESIZE=10\n130 FNEND"
            ))
            .unwrap();
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&2.0));
        for name in ["D", "E"] {
            let array = interpreter.array_ref(name).unwrap();
            assert_eq!(array.dims, vec![639, 479], "{keyword} {name}");
            assert!(
                array.get_number(&[30, 30]).unwrap() > 0.0,
                "{keyword} {name}"
            );
        }
        assert!(interpreter.array_aliases.is_empty());
        assert!(!interpreter.array_exists("P"));
        assert!(!interpreter.array_exists("A"));
    }
}

#[test]
fn textured_cube_sample_runs_all_lighting_and_polygon_modes() {
    let source = include_str!("../../samples/g-cube-textured-light.bas")
        .replace("MAXFRAMES=0", "MAXFRAMES=2")
        .replace("FIXEDDT=0", "FIXEDDT=1/60")
        .replace("CAP=1", "CAP=0")
        // Leave the final frame available for visual parity assertions.
        .replace("SCREEN CLOSE : END", "END")
        // Keep the label fixed so complete frame buffers can be compared.
        .replace(
            "PR$=\"TTRIANGLE\" : IF QUADS THEN PR$=\"TQUAD\"",
            "PR$=\"TEXTURED\"",
        );
    let mut frames = Vec::new();
    for lighting in [0, 1] {
        for quads in [0, 1] {
            let mut interpreter = Interpreter::new();
            interpreter.graphics_window_enabled = false;
            interpreter
                .program
                .load_text(
                    &source
                        .replace("LIGHTING=1", &format!("LIGHTING={lighting}"))
                        .replace("QUADS=1", &format!("QUADS={quads}")),
                )
                .unwrap();
            assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
            assert_eq!(interpreter.numeric_variables.get("FRAMEI"), Some(&2.0));
            assert!(depth_values(&interpreter, "D")
                .iter()
                .any(|value| *value > 0.0));
            frames.push(interpreter.graphics.buffer().to_vec());
        }
    }
    assert_eq!(frames[0], frames[1], "unlit triangles and quads differ");
    assert_eq!(frames[2], frames[3], "lit triangles and quads differ");
    // The cube's interior changes too, independently of the lighting HUD label.
    assert!(
        (150..330).any(|y| (200..440).any(|x| frames[0][y * 640 + x] != frames[2][y * 640 + x]))
    );
}

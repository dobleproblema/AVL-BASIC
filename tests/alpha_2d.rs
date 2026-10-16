use avl_basic::expr::eval_expression;
use avl_basic::{ErrorCode, Interpreter};

const BACKGROUND: u32 = 0x204060;
const SOURCE: u32 = 0xc08040;
const HALF: u32 = 0x706050;

// All coordinates include (30,30). FCIRCLER starts at the MOVE in setup.
const FORMS: [&str; 9] = [
    "SPRITE T$,30,30",
    "TRECTANGLE T$,10,10,110,110",
    "TRECTANGLE T$,10,10,110,110,0,0,1,1",
    "TTRIANGLE T$,10,10,0,0,110,10,1,0,10,110,0,1",
    "TQUAD T$,10,10,0,0,110,10,1,0,110,110,1,1,10,110,0,1",
    "FRECTANGLE 10,10,110,110,RGB(192,128,64)",
    "FTRIANGLE 10,10,110,10,10,110,RGB(192,128,64)",
    "FCIRCLE 50,50,40,RGB(192,128,64)",
    "FCIRCLER 20,20,40,RGB(192,128,64)",
];

fn setup() -> &'static str {
    "MODE 640:PAPER RGB(32,64,96):CLG:INK RGB(192,128,64):T$=\"1x1:c08040\":MOVE 30,30"
}

fn command(interpreter: &mut Interpreter, source: &str) {
    interpreter
        .process_immediate(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
}

fn number(interpreter: &mut Interpreter, expression: &str) -> f64 {
    eval_expression(interpreter, expression)
        .unwrap()
        .as_number()
        .unwrap()
}

fn execute(source: &str, stored: bool) -> Interpreter {
    let mut interpreter = Interpreter::new();
    if stored {
        interpreter
            .program
            .load_text(&format!("10 {}\n20 {source}\n30 END", setup()))
            .unwrap();
        interpreter
            .run_loaded()
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    } else {
        command(&mut interpreter, setup());
        command(&mut interpreter, source);
    }
    interpreter
}

fn pixel(interpreter: &Interpreter, x: usize, y: usize) -> u32 {
    let graphics = &interpreter.graphics;
    graphics.buffer()[(graphics.height - 1 - y) * graphics.width + x]
}

#[test]
fn every_2d_form_blends_equally_in_immediate_and_stored_programs() {
    for form in FORMS {
        let source = format!("{form} ALPHA .5");
        let immediate = execute(&source, false);
        let stored = execute(&source, true);
        assert_eq!(pixel(&immediate, 30, 30), HALF, "{source}");
        assert_eq!(
            immediate.graphics.buffer(),
            stored.graphics.buffer(),
            "{source}"
        );
        assert_eq!(
            (immediate.graphics.xpos(), immediate.graphics.ypos()),
            (stored.graphics.xpos(), stored.graphics.ypos()),
            "{source}"
        );
        assert_eq!(pixel(&immediate, 400, 400), BACKGROUND);
    }
}

#[test]
fn alpha_endpoints_preserve_the_old_opaque_result_and_leave_zero_invisible() {
    for form in FORMS {
        for stored in [false, true] {
            let opaque = execute(form, stored);
            let one = execute(&format!("{form} ALPHA 1"), stored);
            assert_eq!(opaque.graphics.buffer(), one.graphics.buffer(), "{form}");
            assert_eq!(pixel(&one, 30, 30), SOURCE, "{form}");
            let zero = execute(&format!("{form} ALPHA 0"), stored);
            assert!(
                zero.graphics
                    .buffer()
                    .iter()
                    .all(|color| *color == BACKGROUND),
                "{form}; stored={stored}"
            );
        }
    }
}

#[test]
fn colour_keys_leave_holes_and_other_texels_blend_for_every_affine_form() {
    let forms = [
        ("SPRITE T$,30,30", (31, 30)),
        ("TRECTANGLE T$,10,10,110,110", (80, 20)),
        ("TRECTANGLE T$,10,10,110,110,0,0,2,0", (80, 20)),
        ("TTRIANGLE T$,10,10,0,0,110,10,2,0,10,110,0,0", (80, 20)),
        (
            "TQUAD T$,10,10,0,0,110,10,2,0,110,110,2,0,10,110,0,0",
            (80, 20),
        ),
    ];
    for (form, (x, y)) in forms {
        for key in ["6", "RGB(255,0,255)", "\"magenta\"", "\"#f0f\""] {
            for stored in [false, true] {
                let source = format!("T$=\"2x1:ff00ffc08040\":{form},{key} ALPHA .5");
                let interpreter = execute(&source, stored);
                assert_eq!(pixel(&interpreter, 30, 30), BACKGROUND, "{source}");
                assert_eq!(pixel(&interpreter, x, y), HALF, "{source}");
            }
        }
    }
}

#[test]
fn sprite_move_reuses_opacity_and_an_override_is_remembered() {
    for stored in [false, true] {
        let interpreter = execute(
            "SPRITE T$,10,20,6,7 ALPHA .5:\
             SPRITE MOVE 7,20,20:\
             SPRITE MOVE 7,30,20 ALPHA .25:\
             SPRITE MOVE 7,40,20:\
             SPRITE MOVE 7,50,20 ALPHA 0:\
             SPRITE MOVE 7,60,20",
            stored,
        );
        assert_eq!(pixel(&interpreter, 10, 20), HALF);
        assert_eq!(pixel(&interpreter, 20, 20), HALF);
        assert_eq!(pixel(&interpreter, 30, 20), 0x485058);
        assert_eq!(pixel(&interpreter, 40, 20), 0x485058);
        assert_eq!(pixel(&interpreter, 50, 20), BACKGROUND);
        assert_eq!(pixel(&interpreter, 60, 20), BACKGROUND);
    }
}

#[test]
fn invisible_sprites_keep_the_original_collision_mask_and_key_holes() {
    for stored in [false, true] {
        let mut interpreter = execute(
            "COLMODE 2:SPRITE \"2x1:ff00ffc08040\",30,30,6,17 ALPHA 0:\
             SPRITE HITTEST T$,30,30",
            stored,
        );
        assert_eq!(number(&mut interpreter, "HITSPRITE"), 0.0);
        command(&mut interpreter, "SPRITE HITTEST T$,31,30");
        assert_ne!(number(&mut interpreter, "HITSPRITE"), 0.0);
        assert_eq!(number(&mut interpreter, "HITID"), 17.0);
        assert!(interpreter
            .graphics
            .buffer()
            .iter()
            .all(|color| *color == BACKGROUND));
    }
}

#[test]
fn alpha_is_reserved_as_a_scalar_array_and_parameter_name() {
    for stored in [false, true] {
        for (source, expected) in [
            ("ALPHA=7", ErrorCode::Undefined),
            ("LET alpha=.5", ErrorCode::Undefined),
            ("AlPhA$=\"x\"", ErrorCode::Undefined),
            ("ALPHA(0)=1", ErrorCode::Undefined),
            ("PRINT ALPHA", ErrorCode::Undefined),
            ("DIM ALPHA(1)", ErrorCode::InvalidArgument),
            ("REDIM alpha$(1)", ErrorCode::InvalidArgument),
        ] {
            let mut interpreter = Interpreter::new();
            let error = if stored {
                interpreter
                    .program
                    .load_text(&format!("10 {source}\n20 END"))
                    .unwrap();
                interpreter.run_loaded().unwrap_err()
            } else {
                interpreter.process_immediate(source).unwrap_err()
            };
            assert_eq!(error.code, expected, "{source}, stored={stored}");
        }
    }
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text("10 END\n100 DEF SUB Test(ALPHA)\n110 SUBEND")
        .unwrap();
    assert_eq!(
        interpreter.run_loaded().unwrap_err().code,
        ErrorCode::InvalidArgument
    );
}

#[test]
fn opacity_variables_work_in_coordinates_colours_and_expressions() {
    for stored in [false, true] {
        let mut interpreter = execute(
            "OPACITY=10:FRECTANGLE OPACITY,OPACITY,110,110,RGB(192,128,64):\
             OPACITY=.5:TRECTANGLE T$,10,10,110,110 ALPHA OPACITY",
            stored,
        );
        // The underlying rectangle has the source colour, so blending the
        // same source changes nothing.
        assert_eq!(pixel(&interpreter, 30, 30), SOURCE);
        assert_eq!(number(&mut interpreter, "OPACITY"), 0.5);
        command(&mut interpreter, "CLG:OPACITY=RGB(192,128,64)");
        command(&mut interpreter, "FRECTANGLE 10,10,110,110,OPACITY");
        assert_eq!(pixel(&interpreter, 30, 30), SOURCE);
        command(
            &mut interpreter,
            "CLG:FRECTANGLE 10,10,110,110,0+OPACITY ALPHA .5",
        );
        assert_eq!(pixel(&interpreter, 30, 30), HALF);
        command(
            &mut interpreter,
            "CLG:OPACITY=.5:FRECTANGLE 10,10,110,110 ALPHA (.25+OPACITY/2)",
        );
        assert_eq!(pixel(&interpreter, 30, 30), HALF);
    }
}

#[test]
fn sprite_subcommands_accept_variable_ids_with_alpha_clauses() {
    for stored in [false, true] {
        let mut interpreter = execute(
            "SPRITEID=17:SPRITE T$,20,20,6,SPRITEID:\
             SPRITE MOVE SPRITEID ,30,30 ALPHA .5:SPRITE DEL SPRITEID",
            stored,
        );
        assert_eq!(pixel(&interpreter, 30, 30), HALF);
        let before = interpreter.graphics.buffer().to_vec();
        assert!(interpreter
            .process_immediate("SPRITE MOVE 17,40,40")
            .is_err());
        assert_eq!(interpreter.graphics.buffer(), before);
    }
}

#[test]
fn alpha_tokens_inside_strings_functions_or_parentheses_are_not_clauses() {
    for stored in [false, true] {
        let source = "OPACITY=10:\
            TRECTANGLE T$,LEN(\"ALPHA\")+5,(OPACITY),110,110 ALPHA .5";
        let interpreter = execute(source, stored);
        assert_eq!(pixel(&interpreter, 30, 30), HALF);
        let interpreter = execute(
            "OPACITY=.5:FCIRCLE 50,50,40,RGB(192,128,64),ABS(OPACITY)",
            stored,
        );
        assert_eq!(pixel(&interpreter, 50, 50), SOURCE);
    }
}

#[test]
fn opacity_is_evaluated_once_after_texture_coordinates_key_and_sprite_id() {
    for original in FORMS {
        // Wrapping every argument exposes the source evaluation order, with
        // the texture first and the optional key/id before the opacity.
        let original = original.replace("RGB(192,128,64)", "12615744");
        let original = if original.starts_with("SPRITE") {
            format!("{original},6,7")
        } else if original.starts_with('T') {
            format!("{original},6")
        } else {
            original
        };
        let (keyword, arguments) = original.split_once(' ').unwrap();
        let mut count = 0;
        let arguments = arguments
            .split(',')
            .map(|value| {
                if value == "T$" {
                    "FNTEX$()".to_owned()
                } else {
                    count += 1;
                    format!("FNARG({count},{value})")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let form = format!("{keyword} {arguments}");
        let mut interpreter = Interpreter::new();
        interpreter.program.load_text(&format!(
            "10 {}:ORDER$=\"\":CALLS=0\n\
             20 {form} ALPHA FNARG({},.5)\n30 END\n\
             100 DEF FNTEX$()\n110 ORDER$=ORDER$+\"T\"\n120 FNTEX$=T$\n130 FNEND\n\
             200 DEF FNARG(N,V)\n210 ORDER$=ORDER$+CHR$(64+N):CALLS=CALLS+1\n220 FNARG=V\n230 FNEND",
            setup(), count+1
        )).unwrap();
        interpreter.run_loaded().unwrap();
        assert_eq!(
            number(&mut interpreter, "CALLS"),
            (count + 1) as f64,
            "{form}"
        );
        let expected = format!(
            "{}{}",
            if form.starts_with('F') { "" } else { "T" },
            (1..=count + 1)
                .map(|n| char::from(64 + n as u8))
                .collect::<String>()
        );
        assert_eq!(
            eval_expression(&mut interpreter, "ORDER$")
                .unwrap()
                .into_string()
                .unwrap(),
            expected,
            "{form}"
        );
        assert_eq!(pixel(&interpreter, 30, 30), HALF);
    }
}

#[test]
fn malformed_out_of_range_and_nonfinite_opacity_never_draw() {
    for form in FORMS {
        for suffix in [
            " ALPHA",
            " ALPHA .5 ALPHA .5",
            " ALPHA .5,.5",
            " ALPHA -0.01",
            " ALPHA 1.01",
            " ALPHA 1E309",
            " ALPHA -1E309",
            " ALPHA (1E309-1E309)",
            " ALPHA \"half\"",
        ] {
            let source = format!("{form}{suffix}");
            for stored in [false, true] {
                let mut interpreter = Interpreter::new();
                command(&mut interpreter, setup());
                let before = interpreter.graphics.buffer().to_vec();
                let error = if stored {
                    interpreter
                        .program
                        .load_text(&format!("10 {}\n20 {source}", setup()))
                        .unwrap();
                    interpreter.run_loaded()
                } else {
                    interpreter
                        .process_immediate(&source)
                        .map(|_| avl_basic::RunOutcome::End)
                };
                assert!(error.is_err(), "accepted {source}; stored={stored}");
                assert_eq!(
                    interpreter.graphics.buffer(),
                    before,
                    "{source}; stored={stored}"
                );
            }
        }
    }
}

#[test]
fn alpha_on_depth_textures_and_gouraud_is_rejected_without_mutating_pixels_or_depth() {
    let forms = [
        "TTRIANGLE T$,D,10,10,.5,0,0,110,10,1,1,0,10,110,.25,0,1",
        "TTRIANGLE T$,D,10,10,.5,0,0,255,128,0,110,10,1,1,0,255,128,0,10,110,.25,0,1,255,128,0",
        "TQUAD T$,D,10,10,.5,0,0,110,10,1,1,0,110,110,1,1,1,10,110,.25,0,1",
        "TQUAD T$,D,10,10,.5,0,0,255,128,0,110,10,1,1,0,255,128,0,110,110,1,1,1,255,128,0,10,110,.25,0,1,255,128,0",
        "GTRIANGLE D,10,10,.5,255,0,0,110,10,1,0,255,0,10,110,.25,0,0,255",
    ];
    for form in forms {
        for alpha in ["0", ".5", "1"] {
            for stored in [false, true] {
                let mut interpreter = Interpreter::new();
                command(&mut interpreter, setup());
                command(&mut interpreter, "DIM D(639,479):MAT D=.125");
                let before = interpreter.graphics.buffer().to_vec();
                let source = format!("{form} ALPHA {alpha}");
                let error = if stored {
                    interpreter
                        .program
                        .load_text(&format!(
                            "10 {}:DIM D(639,479):MAT D=.125\n20 {source}",
                            setup()
                        ))
                        .unwrap();
                    interpreter.run_loaded()
                } else {
                    interpreter
                        .process_immediate(&source)
                        .map(|_| avl_basic::RunOutcome::End)
                };
                assert!(error.is_err(), "accepted {source}");
                assert_eq!(interpreter.graphics.buffer(), before, "{source}");
                assert_eq!(number(&mut interpreter, "D(30,30)"), 0.125, "{source}");
                assert_eq!(number(&mut interpreter, "D(80,20)"), 0.125, "{source}");
            }
        }
    }
}

#[test]
fn alpha_is_not_accepted_by_hit_testing_or_unfilled_shapes() {
    for form in [
        "SPRITE HITTEST T$,30,30",
        "RECTANGLE 10,10,110,110",
        "TRIANGLE 10,10,110,10,10,110",
        "CIRCLE 50,50,40",
    ] {
        for stored in [false, true] {
            let mut interpreter = Interpreter::new();
            command(&mut interpreter, setup());
            command(&mut interpreter, "SPRITE T$,20,20,6,17");
            let before = interpreter.graphics.buffer().to_vec();
            let source = format!("{form} ALPHA .5");
            let error = if stored {
                interpreter
                    .program
                    .load_text(&format!("10 {}:SPRITE T$,20,20,6,17\n20 {source}", setup()))
                    .unwrap();
                interpreter.run_loaded()
            } else {
                interpreter
                    .process_immediate(&source)
                    .map(|_| avl_basic::RunOutcome::End)
            };
            assert!(error.is_err(), "accepted {source}");
            assert_eq!(interpreter.graphics.buffer(), before, "{source}");
        }
    }
}

#[test]
fn a_quad_and_two_adjacent_affine_triangles_blend_once_on_the_shared_diagonal() {
    let quad = "TQUAD T$,10,10,0,0,110,10,1,0,110,110,1,1,10,110,0,1 ALPHA .5";
    let triangles = "TTRIANGLE T$,10,10,0,0,110,10,1,0,110,110,1,1 ALPHA .5:\
        TTRIANGLE T$,10,10,0,0,110,110,1,1,10,110,0,1 ALPHA .5";
    for stored in [false, true] {
        let single = execute(quad, stored);
        let pair = execute(triangles, stored);
        assert_eq!(single.graphics.buffer(), pair.graphics.buffer());
        for x in 12..108 {
            assert_eq!(pixel(&single, x, x), HALF, "diagonal ({x},{x})");
            assert_eq!(pixel(&single, x, x + 1), HALF);
            assert_eq!(pixel(&single, x + 1, x), HALF);
        }
    }
}

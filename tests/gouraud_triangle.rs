use avl_basic::{ErrorCode, Graphics};

type Triangle = [[f64; 6]; 3];

fn depth_buffer(graphics: &Graphics) -> Vec<f64> {
    vec![0.0; graphics.width * graphics.height]
}

fn draw(graphics: &mut Graphics, depth: &mut [f64], triangle: Triangle) {
    graphics
        .gouraud_triangle(triangle, depth, graphics.width, graphics.height)
        .unwrap();
}

fn pixel(graphics: &Graphics, x: usize, y: usize) -> u32 {
    graphics.buffer()[(graphics.height - 1 - y) * graphics.width + x]
}

fn colourful_triangle() -> Triangle {
    [
        [10.0, 10.0, 0.5, 255.0, 0.0, 0.0],
        [110.0, 10.0, 1.0, 0.0, 255.0, 0.0],
        [10.0, 110.0, 0.25, 0.0, 0.0, 255.0],
    ]
}

#[test]
fn depth_and_colour_use_perspective_and_x_major_bottom_left_storage() {
    for mode in [640, 800] {
        let mut graphics = Graphics::new(mode);
        let mut depth = depth_buffer(&graphics);
        draw(&mut graphics, &mut depth, colourful_triangle());

        // At the pixel centre (30.5,30.5), barycentrics are .59,.205,.205.
        // q=.55125 and RGB=(136.46,94.83,23.71), not the affine (150,52,52).
        assert_eq!(pixel(&graphics, 30, 30), 0x885f18);
        assert!((depth[30 * graphics.height + 30] - 0.55125).abs() < 1e-12);
        assert_eq!(pixel(&graphics, 0, 0), 0);
        assert_eq!(depth[0], 0.0);
    }
}

#[test]
fn crossing_depths_are_order_independent_at_each_pixel() {
    let red = [
        [10.0, 10.0, 0.2, 255.0, 0.0, 0.0],
        [110.0, 10.0, 0.8, 255.0, 0.0, 0.0],
        [10.0, 110.0, 0.2, 255.0, 0.0, 0.0],
    ];
    let blue = [
        [10.0, 10.0, 0.8, 0.0, 0.0, 255.0],
        [110.0, 10.0, 0.2, 0.0, 0.0, 255.0],
        [10.0, 110.0, 0.8, 0.0, 0.0, 255.0],
    ];
    let mut first = Graphics::new(640);
    let mut first_depth = depth_buffer(&first);
    draw(&mut first, &mut first_depth, red);
    draw(&mut first, &mut first_depth, blue);
    let mut reverse = Graphics::new(640);
    let mut reverse_depth = depth_buffer(&reverse);
    draw(&mut reverse, &mut reverse_depth, blue);
    draw(&mut reverse, &mut reverse_depth, red);

    assert_eq!(first.buffer(), reverse.buffer());
    assert_eq!(first_depth, reverse_depth);
    assert_eq!(pixel(&first, 30, 30), 0x0000ff);
    assert_eq!(pixel(&first, 80, 20), 0xff0000);
    assert!((first_depth[30 * first.height + 30] - 0.677).abs() < 1e-12);
    assert!((first_depth[80 * first.height + 20] - 0.623).abs() < 1e-12);
}

#[test]
fn all_vertex_permutations_preserve_coverage_depth_and_colour() {
    let triangle = colourful_triangle();
    let mut reference = Graphics::new(640);
    let mut reference_depth = depth_buffer(&reference);
    draw(&mut reference, &mut reference_depth, triangle);
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut graphics = Graphics::new(640);
        let mut depth = depth_buffer(&graphics);
        draw(
            &mut graphics,
            &mut depth,
            order.map(|index| triangle[index]),
        );
        for (actual, expected) in depth.iter().zip(&reference_depth) {
            assert!((actual - expected).abs() < 1e-12);
        }
        for (&actual, &expected) in graphics.buffer().iter().zip(reference.buffer()) {
            // Reordering floating sums can round a half-channel differently.
            // Pixel coverage and depth still must be identical.
            assert_eq!(actual == 0, expected == 0);
            for shift in [0, 8, 16] {
                let a = ((actual >> shift) & 255) as i32;
                let e = ((expected >> shift) & 255) as i32;
                assert!((a - e).abs() <= 1);
            }
        }
        assert_eq!(pixel(&graphics, 30, 30), 0x885f18);
    }
}

#[test]
fn clipping_and_full_rgb_do_not_interpret_palette_indices() {
    let mut graphics = Graphics::new(640);
    graphics.set_ink_rgb(2, 3, 7).unwrap();
    let mut depth = depth_buffer(&graphics);
    draw(
        &mut graphics,
        &mut depth,
        [
            [-20.0, -20.0, 1.0, 0.0, 0.0, 7.0],
            [60.0, -20.0, 1.0, 0.0, 0.0, 7.0],
            [-20.0, 60.0, 1.0, 0.0, 0.0, 7.0],
        ],
    );
    assert_eq!(pixel(&graphics, 0, 0), 7);
    assert!((depth[0] - 1.0).abs() < 1e-12);
    assert_eq!(pixel(&graphics, 40, 0), 0);
    assert_eq!(depth[40 * graphics.height], 0.0);

    // Per-vertex colours do not change the current INK.
    graphics.plot(200.0, 200.0, None);
    assert_eq!(pixel(&graphics, 200, 200), 0x020307);
}

#[test]
fn degenerate_offscreen_and_nonpositive_q_are_no_ops() {
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    let original = graphics.buffer().to_vec();
    for triangle in [
        [
            [10.0, 10.0, 1.0, 255.0, 255.0, 255.0],
            [20.0, 20.0, 1.0, 255.0, 255.0, 255.0],
            [30.0, 30.0, 1.0, 255.0, 255.0, 255.0],
        ],
        [
            [-30.0, -30.0, 1.0, 255.0, 255.0, 255.0],
            [-20.0, -30.0, 1.0, 255.0, 255.0, 255.0],
            [-30.0, -20.0, 1.0, 255.0, 255.0, 255.0],
        ],
    ] {
        draw(&mut graphics, &mut depth, triangle);
    }
    for q in [0.0, -1.0] {
        let mut triangle = colourful_triangle();
        triangle[1][2] = q;
        draw(&mut graphics, &mut depth, triangle);
    }
    assert_eq!(graphics.buffer(), original);
    assert!(depth.iter().all(|q| *q == 0.0));
}

#[test]
fn equal_or_epsilon_close_depth_preserves_the_first_fragment() {
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    let red = [
        [0.0, 0.0, 1.0, 255.0, 0.0, 0.0],
        [100.0, 0.0, 1.0, 255.0, 0.0, 0.0],
        [0.0, 100.0, 1.0, 255.0, 0.0, 0.0],
    ];
    draw(&mut graphics, &mut depth, red);
    let reference = graphics.buffer().to_vec();
    let mut blue = red;
    for vertex in &mut blue {
        vertex[2] += 0.5e-9;
        vertex[3] = 0.0;
        vertex[5] = 255.0;
    }
    draw(&mut graphics, &mut depth, blue);
    assert_eq!(graphics.buffer(), reference);
    for vertex in &mut blue {
        vertex[2] += 2.0e-9;
    }
    draw(&mut graphics, &mut depth, blue);
    assert_eq!(pixel(&graphics, 30, 30), 0x0000ff);
}

#[test]
fn dimensions_overflow_and_nonfinite_values_fail_before_mutation() {
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    let original = graphics.buffer().to_vec();
    for (width, height) in [(1, 640 * 480), (usize::MAX, 2), (0, 0)] {
        let error = graphics
            .gouraud_triangle(colourful_triangle(), &mut depth, width, height)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidValue);
    }
    let error = graphics
        .gouraud_triangle(colourful_triangle(), &mut depth[..10], 640, 480)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidValue);
    for (component, value) in [(0, f64::NAN), (2, f64::INFINITY), (4, f64::NEG_INFINITY)] {
        let mut triangle = colourful_triangle();
        triangle[0][component] = value;
        let error = graphics
            .gouraud_triangle(triangle, &mut depth, 640, 480)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
    assert_eq!(graphics.buffer(), original);
    assert!(depth.iter().all(|q| *q == 0.0));
}

#[test]
fn hidden_triangles_move_the_cursor_and_preserve_ink_and_collision_state() {
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    graphics.set_ink_rgb(2, 3, 7).unwrap();
    graphics.move_to(23.0, 31.0);
    graphics.colmode(2).unwrap();
    let sprite = "1x1:050607";
    graphics
        .draw_sprite(sprite, 12.0, 14.0, None, Some(17), false)
        .unwrap();
    graphics
        .draw_sprite(sprite, 12.0, 14.0, None, None, true)
        .unwrap();
    let collision = (
        graphics.hit(),
        graphics.hitcolor(),
        graphics.hitsprite(),
        graphics.hitid(),
    );
    let mut triangle = colourful_triangle();
    triangle[2][2] = 0.0;
    draw(&mut graphics, &mut depth, triangle);
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 110.0));
    assert_eq!(
        (
            graphics.hit(),
            graphics.hitcolor(),
            graphics.hitsprite(),
            graphics.hitid(),
        ),
        collision
    );
    let error = graphics
        .gouraud_triangle(colourful_triangle(), &mut depth, 1, 1)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidValue);
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 110.0));
    graphics.plot(300.0, 300.0, None);
    assert_eq!(pixel(&graphics, 300, 300), 0x020307);

    // A regular colour write must leave the sprite ownership/collision result
    // untouched, as INK+PLOT does; HITTEST still finds owner 17 afterwards.
    draw(&mut graphics, &mut depth, colourful_triangle());
    assert_eq!(graphics.hitid(), collision.3);
    graphics
        .draw_sprite(sprite, 12.0, 14.0, None, None, true)
        .unwrap();
    assert_eq!(graphics.hitid(), 17);
}

#[test]
fn derived_nonfinite_arithmetic_is_rejected_atomically() {
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    let before = graphics.buffer().to_vec();
    let mut triangle = colourful_triangle();
    triangle[0][0] = -f64::MAX;
    triangle[1][0] = f64::MAX;
    let error = graphics
        .gouraud_triangle(triangle, &mut depth, 640, 480)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    let mut triangle = colourful_triangle();
    triangle[0][2] = f64::MAX;
    let error = graphics
        .gouraud_triangle(triangle, &mut depth, 640, 480)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(graphics.buffer(), before);
    assert!(depth.iter().all(|q| *q == 0.0));
}

#[test]
fn scale_and_viewport_transform_vertices_and_clip_both_buffers() {
    let mut graphics = Graphics::new(640);
    graphics
        .set_origin(100, 50, Some((10, 20, 20, 10)))
        .unwrap();
    graphics.set_scale(Some((-1.0, 1.0, -1.0, 1.0, 0))).unwrap();
    graphics.set_pen_width(4).unwrap();
    graphics.set_mask(Some(0)).unwrap();
    let mut depth = depth_buffer(&graphics);
    draw(
        &mut graphics,
        &mut depth,
        [
            [-2.0, -2.0, 1.0, 300.0, -20.0, 7.0],
            [10.0, -2.0, 1.0, 300.0, -20.0, 7.0],
            [-2.0, 10.0, 1.0, 300.0, -20.0, 7.0],
        ],
    );
    assert_eq!(depth.iter().filter(|q| **q != 0.0).count(), 121);
    for x in 0..30 {
        for y in 0..30 {
            let inside = (10..=20).contains(&x) && (10..=20).contains(&y);
            assert_eq!(pixel(&graphics, x, y), if inside { 0xff0007 } else { 0 });
        }
    }
    assert_eq!(graphics.xpos(), -2.0);
    assert_eq!(graphics.ypos(), 10.0);
}

#[test]
fn origin_preserves_fractional_vertices_and_physical_depth_indices() {
    let mut reference = Graphics::new(640);
    let mut shifted = Graphics::new(640);
    shifted.set_origin(20, 30, None).unwrap();
    let mut reference_depth = depth_buffer(&reference);
    let mut shifted_depth = depth_buffer(&shifted);
    let mut triangle = colourful_triangle();
    for vertex in &mut triangle {
        vertex[0] += 0.25;
        vertex[1] += 0.75;
    }
    draw(&mut reference, &mut reference_depth, triangle);
    draw(&mut shifted, &mut shifted_depth, triangle);
    assert_eq!(pixel(&shifted, 50, 60), pixel(&reference, 30, 30));
    assert_eq!(shifted_depth[50 * 480 + 60], reference_depth[30 * 480 + 30]);
    assert_eq!(shifted_depth[30 * 480 + 30], 0.0);
    assert_eq!((shifted.xpos(), shifted.ypos()), (10.25, 110.75));
}

#[test]
fn scale_keeps_subpixel_precision_and_the_cursor_in_user_units() {
    let mut scaled = Graphics::new(640);
    scaled.set_scale(Some((0.0, 2.0, 0.0, 2.0, 0))).unwrap();
    let mut reference = Graphics::new(640);
    let mut scaled_depth = depth_buffer(&scaled);
    let mut reference_depth = depth_buffer(&reference);
    let user = [
        [0.25, 0.25, 0.5, 255.0, 0.0, 0.0],
        [1.25, 0.25, 1.0, 0.0, 255.0, 0.0],
        [0.25, 1.25, 0.25, 0.0, 0.0, 255.0],
    ];
    let physical = user.map(|mut vertex| {
        vertex[0] *= 639.0 / 2.0;
        vertex[1] *= 479.0 / 2.0;
        vertex
    });
    draw(&mut scaled, &mut scaled_depth, user);
    draw(&mut reference, &mut reference_depth, physical);
    assert_eq!(scaled.buffer(), reference.buffer());
    assert_eq!(scaled_depth, reference_depth);
    assert_eq!((scaled.xpos(), scaled.ypos()), (0.25, 1.25));
}

#[test]
fn all_valid_no_ops_leave_ink_and_finish_at_the_third_vertex() {
    let mut graphics = Graphics::new(640);
    graphics.set_ink_rgb(2, 3, 7).unwrap();
    let mut depth = depth_buffer(&graphics);
    for vertices in [
        [
            [10.0, 10.0, 1.0, 255.0, 0.0, 0.0],
            [20.0, 20.0, 1.0, 255.0, 0.0, 0.0],
            [30.0, 30.0, 1.0, 255.0, 0.0, 0.0],
        ],
        [
            [-30.0, -30.0, 1.0, 255.0, 0.0, 0.0],
            [-20.0, -30.0, 1.0, 255.0, 0.0, 0.0],
            [-30.0, -20.0, 1.0, 255.0, 0.0, 0.0],
        ],
    ] {
        draw(&mut graphics, &mut depth, vertices);
        assert_eq!(
            (graphics.xpos(), graphics.ypos()),
            (vertices[2][0], vertices[2][1])
        );
    }
    assert!(graphics.buffer().iter().all(|colour| *colour == 0));
    assert!(depth.iter().all(|q| *q == 0.0));
    depth.fill(2.0);
    draw(&mut graphics, &mut depth, colourful_triangle());
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 110.0));
    assert!(graphics.buffer().iter().all(|colour| *colour == 0));
    graphics.plot(200.0, 200.0, None);
    assert_eq!(pixel(&graphics, 200, 200), 0x020307);
}

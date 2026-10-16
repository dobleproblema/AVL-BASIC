use avl_basic::graphics::Texture;
use avl_basic::{ErrorCode, Graphics};

type Triangle = [[f64; 8]; 3];

fn depth_buffer(graphics: &Graphics) -> Vec<f64> {
    vec![0.0; graphics.width * graphics.height]
}

fn draw(graphics: &mut Graphics, texture: &Texture, depth: &mut [f64], vertices: Triangle) {
    graphics
        .perspective_textured_triangle(texture, vertices, depth, graphics.width, graphics.height)
        .unwrap();
}

fn draw_quad(
    graphics: &mut Graphics,
    texture: &Texture,
    depth: &mut [f64],
    vertices: [[f64; 8]; 4],
) {
    graphics
        .perspective_textured_quad(texture, vertices, depth, graphics.width, graphics.height)
        .unwrap();
}

fn pixel(graphics: &Graphics, x: usize, y: usize) -> u32 {
    graphics.buffer()[(graphics.height - 1 - y) * graphics.width + x]
}

fn texture_pixel(x: usize, y: usize) -> u32 {
    (((x + 1) * 25) << 16 | ((y + 1) * 20) << 8 | ((x + y + 1) * 10)) as u32
}

fn patterned_texture() -> Texture {
    let mut screen = String::from("8x8:");
    for y in (0..8).rev() {
        for x in 0..8 {
            screen.push_str(&format!("{:06x}", texture_pixel(x, y)));
        }
    }
    Texture::from_gscr(&screen).unwrap()
}

fn triangle() -> Triangle {
    [
        [10.0, 10.0, 0.5, 0.0, 0.0, 255.0, 255.0, 255.0],
        [110.0, 10.0, 1.0, 8.0, 0.0, 255.0, 255.0, 255.0],
        [10.0, 110.0, 0.25, 0.0, 8.0, 255.0, 255.0, 255.0],
    ]
}

fn quad() -> [[f64; 8]; 4] {
    [
        [10.25, 10.25, 0.5, 0.0, 0.0, 200.0, 60.0, 240.0],
        [110.25, 10.25, 1.0, 4.0, 0.0, 150.0, 30.0, 120.0],
        [110.25, 110.25, 1.25, 3.2, 3.2, 120.0, 88.0, 96.0],
        [
            10.25,
            110.25,
            0.75,
            0.0,
            4.0 / 0.75,
            100.0 / 0.75,
            110.0 / 0.75,
            160.0,
        ],
    ]
}

#[test]
fn uv_uses_perspective_depth_and_wrapped_bottom_left_texels() {
    let texture = patterned_texture();
    for mode in [640, 800] {
        let mut graphics = Graphics::new(mode);
        let mut depth = depth_buffer(&graphics);
        draw(&mut graphics, &texture, &mut depth, triangle());
        // Centre (30.5,30.5): barycentrics .59,.205,.205; q=.55125,
        // u=2.975..., v=.743..., whereas affine u=v=1.64.
        assert_eq!(pixel(&graphics, 30, 30), texture_pixel(2, 0));
        assert_ne!(pixel(&graphics, 30, 30), texture_pixel(1, 1));
        assert!((depth[30 * graphics.height + 30] - 0.55125).abs() < 1e-12);
        let mut wrapped = triangle();
        for vertex in &mut wrapped {
            vertex[3] -= 16.0;
            vertex[4] += 24.0;
        }
        let mut other = Graphics::new(mode);
        let mut other_depth = depth_buffer(&other);
        draw(&mut other, &texture, &mut other_depth, wrapped);
        assert_eq!(pixel(&other, 30, 30), texture_pixel(2, 0));
    }
}

#[test]
fn rgb_is_perspective_interpolated_then_modulates_the_texture() {
    let mut graphics = Graphics::new(640);
    let texture = patterned_texture();
    let mut depth = depth_buffer(&graphics);
    let mut vertices = triangle();
    vertices[0][5..].copy_from_slice(&[255.0, 0.0, 0.0]);
    vertices[1][5..].copy_from_slice(&[0.0, 255.0, 0.0]);
    vertices[2][5..].copy_from_slice(&[0.0, 0.0, 255.0]);
    draw(&mut graphics, &texture, &mut depth, vertices);
    // Sample RGB=(75,20,30), perspective light=(136.46,94.83,23.71).
    assert_eq!(pixel(&graphics, 30, 30), 0x280703);

    let texture = Texture::from_gscr("1x1:6496c8").unwrap();
    let mut vertices = triangle();
    for vertex in &mut vertices {
        vertex[5..].copy_from_slice(&[-255.0, 300.0, 127.5]);
        vertex[2] = 2.0;
    }
    draw(&mut graphics, &texture, &mut depth, vertices);
    assert_eq!(pixel(&graphics, 30, 30), 0x009664);
}

#[test]
fn textured_and_gouraud_crossing_depths_are_order_independent() {
    let texture = Texture::from_gscr("1x1:ff0000").unwrap();
    let red = [
        [10.0, 10.0, 0.2, 0.0, 0.0, 255.0, 255.0, 255.0],
        [110.0, 10.0, 0.8, 0.0, 0.0, 255.0, 255.0, 255.0],
        [10.0, 110.0, 0.2, 0.0, 0.0, 255.0, 255.0, 255.0],
    ];
    let blue = [
        [10.0, 10.0, 0.8, 0.0, 0.0, 255.0],
        [110.0, 10.0, 0.2, 0.0, 0.0, 255.0],
        [10.0, 110.0, 0.8, 0.0, 0.0, 255.0],
    ];
    let mut first = Graphics::new(640);
    let mut first_depth = depth_buffer(&first);
    draw(&mut first, &texture, &mut first_depth, red);
    first
        .gouraud_triangle(blue, &mut first_depth, 640, 480)
        .unwrap();
    let mut reverse = Graphics::new(640);
    let mut reverse_depth = depth_buffer(&reverse);
    reverse
        .gouraud_triangle(blue, &mut reverse_depth, 640, 480)
        .unwrap();
    draw(&mut reverse, &texture, &mut reverse_depth, red);
    assert_eq!(first.buffer(), reverse.buffer());
    assert_eq!(first_depth, reverse_depth);
    assert_eq!(pixel(&first, 30, 30), 0x0000ff);
    assert_eq!(pixel(&first, 80, 20), 0xff0000);
}

#[test]
fn all_windings_keep_coverage_depth_and_texture() {
    let texture = patterned_texture();
    let mut reference = Graphics::new(640);
    let mut reference_depth = depth_buffer(&reference);
    draw(&mut reference, &texture, &mut reference_depth, triangle());
    for indices in [[0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        let mut graphics = Graphics::new(640);
        let mut depth = depth_buffer(&graphics);
        draw(
            &mut graphics,
            &texture,
            &mut depth,
            indices.map(|i| triangle()[i]),
        );
        for (actual, expected) in depth.iter().zip(&reference_depth) {
            assert!((actual - expected).abs() < 1e-12);
        }
        assert_eq!(pixel(&graphics, 30, 30), pixel(&reference, 30, 30));
        for x in 10..110 {
            for y in 10..110 {
                assert_eq!(pixel(&graphics, x, y) != 0, pixel(&reference, x, y) != 0);
            }
        }
    }
}

#[test]
fn quad_has_no_diagonal_holes_and_keeps_planar_perspective_attributes() {
    let texture = patterned_texture();
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    draw_quad(&mut graphics, &texture, &mut depth, quad());
    assert_eq!(depth.iter().filter(|q| **q > 0.0).count(), 10_000);
    for x in 10..110 {
        for y in 10..110 {
            let dx = (x as f64 + 0.5 - 10.25) / 100.0;
            let dy = (y as f64 + 0.5 - 10.25) / 100.0;
            let q = 0.5 + 0.5 * dx + 0.25 * dy;
            assert!((depth[x * 480 + y] - q).abs() < 1e-12, "({x},{y})");
            let u = 4.0 * dx / q;
            let v = 4.0 * dy / q;
            // Texel boundaries and half-channel rounding can differ by an ULP.
            if (u - u.round()).abs() < 1e-8 || (v - v.round()).abs() < 1e-8 {
                continue;
            }
            let texel = texture_pixel(u.floor() as usize, v.floor() as usize);
            let light = [(100.0 + 50.0 * dx) / q, (30.0 + 80.0 * dy) / q, 120.0 / q];
            for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                let sample = ((texel >> shift) & 255) as f64;
                let expected = (sample * light[channel].clamp(0.0, 255.0) / 255.0).round() as i32;
                let actual = ((pixel(&graphics, x, y) >> shift) & 255) as i32;
                assert!(
                    (actual - expected).abs() <= 1,
                    "({x},{y}) channel {channel}"
                );
            }
        }
    }
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.25, 110.25));
}

#[test]
fn late_quad_input_and_derived_errors_are_atomic_including_cursor() {
    let texture = patterned_texture();
    let mut graphics = Graphics::new(640);
    graphics.move_to(23.0, 31.0);
    let mut depth = depth_buffer(&graphics);
    let original = graphics.buffer().to_vec();
    let mut invalid = quad();
    invalid[3][7] = f64::NAN;
    let mut overflow = quad();
    overflow[3][3] = f64::MAX;
    overflow[3][2] = 2.0;
    let mut gradient_overflow = quad();
    gradient_overflow[3] = [10.25, 10.25001, 1.0, f64::MAX, 0.0, 255.0, 255.0, 255.0];
    let mut area_overflow = quad();
    area_overflow[3][0] = f64::MAX;
    for vertices in [invalid, overflow, gradient_overflow, area_overflow] {
        let error = graphics
            .perspective_textured_quad(&texture, vertices, &mut depth, 640, 480)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(graphics.buffer(), original);
        assert!(depth.iter().all(|q| *q == 0.0));
        assert_eq!((graphics.xpos(), graphics.ypos()), (23.0, 31.0));
    }
}

#[test]
fn bad_dimensions_and_nonfinite_triangle_values_fail_before_mutation() {
    let texture = patterned_texture();
    let mut graphics = Graphics::new(640);
    graphics.move_to(23.0, 31.0);
    let mut depth = depth_buffer(&graphics);
    for (width, height) in [(1, 640 * 480), (usize::MAX, 2), (0, 0)] {
        let error = graphics
            .perspective_textured_triangle(&texture, triangle(), &mut depth, width, height)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidValue);
    }
    let error = graphics
        .perspective_textured_triangle(&texture, triangle(), &mut depth[..10], 640, 480)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidValue);
    for component in 0..8 {
        let mut vertices = triangle();
        vertices[2][component] = f64::INFINITY;
        let error = graphics
            .perspective_textured_triangle(&texture, vertices, &mut depth, 640, 480)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
    assert!(graphics.buffer().iter().all(|pixel| *pixel == 0));
    assert!(depth.iter().all(|q| *q == 0.0));
    assert_eq!((graphics.xpos(), graphics.ypos()), (23.0, 31.0));
}

#[test]
fn nonpositive_q_skips_the_entire_quad_and_valid_no_ops_finish_the_cursor() {
    let texture = patterned_texture();
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    for q in [0.0, -1.0] {
        let mut vertices = quad();
        // The first triangle is drawable; it must not be drawn when vertex 4
        // makes the second triangle cross the caller's unclipped near plane.
        vertices[3][2] = q;
        draw_quad(&mut graphics, &texture, &mut depth, vertices);
        assert!(graphics.buffer().iter().all(|pixel| *pixel == 0));
        assert!(depth.iter().all(|q| *q == 0.0));
        assert_eq!((graphics.xpos(), graphics.ypos()), (10.25, 110.25));
    }
    let mut degenerate = triangle();
    degenerate[1][0..2].copy_from_slice(&[20.0, 20.0]);
    degenerate[2][0..2].copy_from_slice(&[30.0, 30.0]);
    draw(&mut graphics, &texture, &mut depth, degenerate);
    assert_eq!((graphics.xpos(), graphics.ypos()), (30.0, 30.0));
    let offscreen = triangle().map(|mut vertex| {
        vertex[0] -= 1000.0;
        vertex
    });
    draw(&mut graphics, &texture, &mut depth, offscreen);
    assert_eq!((graphics.xpos(), graphics.ypos()), (-990.0, 110.0));
    depth.fill(2.0);
    draw(&mut graphics, &texture, &mut depth, triangle());
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 110.0));
    assert!(graphics.buffer().iter().all(|pixel| *pixel == 0));
}

#[test]
fn origin_and_scale_preserve_subpixels_and_clip_both_buffers() {
    let texture = patterned_texture();
    let mut reference = Graphics::new(640);
    let mut shifted = Graphics::new(640);
    shifted.set_origin(20, 30, None).unwrap();
    let mut reference_depth = depth_buffer(&reference);
    let mut shifted_depth = depth_buffer(&shifted);
    let fractional = triangle().map(|mut vertex| {
        vertex[0] += 0.25;
        vertex[1] += 0.75;
        vertex
    });
    draw(&mut reference, &texture, &mut reference_depth, fractional);
    draw(&mut shifted, &texture, &mut shifted_depth, fractional);
    assert_eq!(pixel(&shifted, 50, 60), pixel(&reference, 30, 30));
    assert_eq!(shifted_depth[50 * 480 + 60], reference_depth[30 * 480 + 30]);
    assert_eq!((shifted.xpos(), shifted.ypos()), (10.25, 110.75));

    let mut scaled = Graphics::new(640);
    scaled.set_scale(Some((0.0, 2.0, 0.0, 2.0, 0))).unwrap();
    let user = fractional.map(|mut vertex| {
        vertex[0] /= 100.0;
        vertex[1] /= 100.0;
        vertex
    });
    let physical = user.map(|mut vertex| {
        vertex[0] *= 639.0 / 2.0;
        vertex[1] *= 479.0 / 2.0;
        vertex
    });
    let mut physical_graphics = Graphics::new(640);
    let mut scaled_depth = depth_buffer(&scaled);
    let mut physical_depth = depth_buffer(&physical_graphics);
    draw(&mut scaled, &texture, &mut scaled_depth, user);
    draw(
        &mut physical_graphics,
        &texture,
        &mut physical_depth,
        physical,
    );
    assert_eq!(scaled.buffer(), physical_graphics.buffer());
    assert_eq!(scaled_depth, physical_depth);

    let mut clipped = Graphics::new(640);
    clipped.set_origin(100, 50, Some((10, 20, 20, 10))).unwrap();
    clipped.set_scale(Some((-1.0, 1.0, -1.0, 1.0, 0))).unwrap();
    clipped.set_mask(Some(0)).unwrap();
    clipped.set_pen_width(4).unwrap();
    let mut clipped_depth = depth_buffer(&clipped);
    let solid = Texture::from_gscr("1x1:123456").unwrap();
    let vertices = [
        [-2.0, -2.0, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0],
        [10.0, -2.0, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0],
        [-2.0, 10.0, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0],
    ];
    draw(&mut clipped, &solid, &mut clipped_depth, vertices);
    assert_eq!(clipped_depth.iter().filter(|q| **q > 0.0).count(), 121);
    for x in 0..30 {
        for y in 0..30 {
            let inside = (10..=20).contains(&x) && (10..=20).contains(&y);
            assert_eq!(pixel(&clipped, x, y), if inside { 0x123456 } else { 0 });
        }
    }
}

#[test]
fn equal_depth_tolerance_and_special_caller_depths_match_gouraud() {
    let texture = Texture::from_gscr("1x1:123456").unwrap();
    let other = Texture::from_gscr("1x1:abcdef").unwrap();
    let mut graphics = Graphics::new(640);
    let mut depth = depth_buffer(&graphics);
    let vertices = triangle().map(|mut vertex| {
        vertex[2] = 1.0;
        vertex
    });
    draw(&mut graphics, &texture, &mut depth, vertices);
    let close = vertices.map(|mut vertex| {
        vertex[2] += 0.5e-9;
        vertex
    });
    draw(&mut graphics, &other, &mut depth, close);
    assert_eq!(pixel(&graphics, 30, 30), 0x123456);
    let nearer = vertices.map(|mut vertex| {
        vertex[2] += 2.5e-9;
        vertex
    });
    draw(&mut graphics, &other, &mut depth, nearer);
    assert_eq!(pixel(&graphics, 30, 30), 0xabcdef);
    for initial in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY] {
        let mut textured = Graphics::new(640);
        let mut gouraud = Graphics::new(640);
        let mut texture_depth = vec![initial; 640 * 480];
        let mut gouraud_depth = texture_depth.clone();
        draw(&mut textured, &texture, &mut texture_depth, vertices);
        let colours = vertices.map(|v| [v[0], v[1], v[2], 18.0, 52.0, 86.0]);
        gouraud
            .gouraud_triangle(colours, &mut gouraud_depth, 640, 480)
            .unwrap();
        assert_eq!(textured.buffer(), gouraud.buffer());
    }
}

#[test]
fn illumination_writes_preserve_ink_sprite_ownership_and_collision_state() {
    let texture = Texture::from_gscr("1x1:ffffff").unwrap();
    let mut graphics = Graphics::new(640);
    graphics.set_ink_rgb(2, 3, 7).unwrap();
    graphics.colmode(2).unwrap();
    graphics
        .draw_sprite("1x1:050607", 12.0, 14.0, None, Some(17), false)
        .unwrap();
    graphics
        .draw_sprite("1x1:050607", 12.0, 14.0, None, None, true)
        .unwrap();
    let collisions = (
        graphics.hit(),
        graphics.hitcolor(),
        graphics.hitsprite(),
        graphics.hitid(),
    );
    let mut depth = depth_buffer(&graphics);
    draw(&mut graphics, &texture, &mut depth, triangle());
    assert_eq!(
        (
            graphics.hit(),
            graphics.hitcolor(),
            graphics.hitsprite(),
            graphics.hitid()
        ),
        collisions
    );
    graphics
        .draw_sprite("1x1:050607", 12.0, 14.0, None, None, true)
        .unwrap();
    assert_eq!(graphics.hitid(), 17);
    graphics.plot(300.0, 300.0, None);
    assert_eq!(pixel(&graphics, 300, 300), 0x020307);
}

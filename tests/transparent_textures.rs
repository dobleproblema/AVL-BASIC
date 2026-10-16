use avl_basic::graphics::Texture;
use avl_basic::Graphics;

#[derive(Clone, Copy, Debug)]
enum Shape {
    Rect,
    Triangle,
    Quad,
}

const SHAPES: [Shape; 3] = [Shape::Rect, Shape::Triangle, Shape::Quad];

fn pixel(graphics: &Graphics, x: usize, y: usize) -> u32 {
    graphics.buffer()[(graphics.height - 1 - y) * graphics.width + x]
}

fn triangle(q: f64, u: f64, rgb: [f64; 3]) -> [[f64; 8]; 3] {
    [
        [10.0, 10.0, q, u, 0.0, rgb[0], rgb[1], rgb[2]],
        [110.0, 10.0, q, u + 2.0, 0.0, rgb[0], rgb[1], rgb[2]],
        [10.0, 110.0, q, u, 0.0, rgb[0], rgb[1], rgb[2]],
    ]
}

fn quad(q: f64, u: f64, rgb: [f64; 3]) -> [[f64; 8]; 4] {
    let [a, b, d] = triangle(q, u, rgb);
    let mut c = b;
    c[1] = 110.0;
    [a, b, c, d]
}

fn affine(
    graphics: &mut Graphics,
    texture: &Texture,
    shape: Shape,
    u: f64,
    transparent: Option<i32>,
) {
    match shape {
        Shape::Rect => graphics.textured_rect_with_transparency(
            texture,
            10.0,
            10.0,
            110.0,
            110.0,
            Some((u, 0.0, u + 2.0, 0.0)),
            transparent,
        ),
        Shape::Triangle => graphics.textured_triangle_with_transparency(
            texture,
            10.0,
            10.0,
            u,
            0.0,
            110.0,
            10.0,
            u + 2.0,
            0.0,
            10.0,
            110.0,
            u,
            0.0,
            transparent,
        ),
        Shape::Quad => graphics.textured_quad_with_transparency(
            texture,
            10.0,
            10.0,
            u,
            0.0,
            110.0,
            10.0,
            u + 2.0,
            0.0,
            110.0,
            110.0,
            u + 2.0,
            0.0,
            10.0,
            110.0,
            u,
            0.0,
            transparent,
        ),
    }
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn perspective(
    graphics: &mut Graphics,
    texture: &Texture,
    depth: &mut [f64],
    shape: Shape,
    q: f64,
    u: f64,
    rgb: [f64; 3],
    transparent: Option<i32>,
) {
    match shape {
        Shape::Triangle => graphics.perspective_textured_triangle_with_transparency(
            texture,
            triangle(q, u, rgb),
            depth,
            graphics.width,
            graphics.height,
            transparent,
        ),
        Shape::Quad => graphics.perspective_textured_quad_with_transparency(
            texture,
            quad(q, u, rgb),
            depth,
            graphics.width,
            graphics.height,
            transparent,
        ),
        Shape::Rect => unreachable!(),
    }
    .unwrap();
}

fn background(graphics: &mut Graphics, depth: &mut [f64], shape: Shape) {
    let texture = Texture::from_gscr("1x1:2030d0").unwrap();
    perspective(graphics, &texture, depth, shape, 0.5, 0.0, [255.0; 3], None);
}

#[test]
fn affine_holes_leave_the_existing_background_in_every_primitive() {
    let texture = Texture::from_gscr("2x1:ff00ff20d040").unwrap();
    for shape in SHAPES {
        let mut graphics = Graphics::new(640);
        graphics.rectangle(0.0, 0.0, 200.0, 200.0, Some(0x2030d0), true);
        affine(&mut graphics, &texture, shape, 0.0, Some(6));
        assert_eq!(pixel(&graphics, 30, 30), 0x2030d0, "{shape:?}");
        assert_eq!(pixel(&graphics, 80, 20), 0x20d040, "{shape:?}");
    }
}

#[test]
fn transparent_foreground_never_occludes_background_in_either_draw_order() {
    let texture = Texture::from_gscr("2x1:ff00ff20d040").unwrap();
    for shape in [Shape::Triangle, Shape::Quad] {
        for rgb in [[255.0; 3], [127.5, 255.0, 127.5]] {
            let mut results = Vec::new();
            for foreground_first in [false, true] {
                let mut graphics = Graphics::new(640);
                let mut depth = vec![0.0; graphics.width * graphics.height];
                if !foreground_first {
                    background(&mut graphics, &mut depth, shape);
                }
                perspective(
                    &mut graphics,
                    &texture,
                    &mut depth,
                    shape,
                    1.0,
                    0.0,
                    rgb,
                    Some(6),
                );
                if foreground_first {
                    // A skipped texel must leave both colour and depth untouched.
                    assert_eq!(pixel(&graphics, 30, 30), 0);
                    assert_eq!(depth[30 * graphics.height + 30], 0.0);
                    background(&mut graphics, &mut depth, shape);
                }
                assert_eq!(pixel(&graphics, 30, 30), 0x2030d0, "{shape:?}");
                assert_eq!(depth[30 * graphics.height + 30], 0.5);
                assert_eq!(depth[80 * graphics.height + 20], 1.0);
                assert_eq!(
                    pixel(&graphics, 80, 20),
                    if rgb[0] == 255.0 { 0x20d040 } else { 0x10d020 }
                );
                results.push((graphics.buffer().to_vec(), depth));
            }
            assert_eq!(results[0], results[1], "{shape:?}, RGB={rgb:?}");
        }
    }
}

#[test]
fn key_is_compared_before_lighting_and_an_opaque_texel_can_shade_to_the_key() {
    let texture = Texture::from_gscr("2x1:ff00ffffffff").unwrap();
    for shape in [Shape::Triangle, Shape::Quad] {
        let mut graphics = Graphics::new(640);
        let mut depth = vec![0.125; graphics.width * graphics.height];
        perspective(
            &mut graphics,
            &texture,
            &mut depth,
            shape,
            1.0,
            0.0,
            [255.0, 0.0, 255.0],
            Some(6),
        );
        assert_eq!(pixel(&graphics, 30, 30), 0, "{shape:?}");
        assert_eq!(depth[30 * graphics.height + 30], 0.125);
        assert_eq!(pixel(&graphics, 80, 20), 0xff00ff, "{shape:?}");
        assert_eq!(depth[80 * graphics.height + 20], 1.0);
    }
}

#[test]
fn wrapped_uvs_select_the_same_key_for_affine_and_perspective_textures() {
    let texture = Texture::from_gscr("2x1:ff00ff20d040").unwrap();
    for u in [-200.0, -2.0, 0.0, 2.0, 200.0] {
        for shape in SHAPES {
            let mut graphics = Graphics::new(640);
            affine(&mut graphics, &texture, shape, u, Some(6));
            assert_eq!(pixel(&graphics, 30, 30), 0, "{shape:?}, u={u}");
            assert_eq!(pixel(&graphics, 80, 20), 0x20d040);
            if matches!(shape, Shape::Rect) {
                continue;
            }
            let mut graphics = Graphics::new(640);
            let mut depth = vec![0.0; graphics.width * graphics.height];
            perspective(
                &mut graphics,
                &texture,
                &mut depth,
                shape,
                1.0,
                u,
                [255.0; 3],
                Some(6),
            );
            assert_eq!(pixel(&graphics, 30, 30), 0, "{shape:?}, u={u}");
            assert_eq!(depth[30 * graphics.height + 30], 0.0);
            assert_eq!(pixel(&graphics, 80, 20), 0x20d040);
        }
    }
}

#[test]
fn black_key_zero_is_distinct_from_omitted_transparency() {
    let texture = Texture::from_gscr("1x1:000000").unwrap();
    for transparent in [None, Some(0)] {
        for shape in SHAPES {
            let mut graphics = Graphics::new(640);
            graphics.rectangle(0.0, 0.0, 200.0, 200.0, Some(0x2030d0), true);
            affine(&mut graphics, &texture, shape, 0.0, transparent);
            assert_eq!(
                pixel(&graphics, 30, 30),
                if transparent.is_some() { 0x2030d0 } else { 0 }
            );
            if matches!(shape, Shape::Rect) {
                continue;
            }
            let mut depth = vec![0.0; graphics.width * graphics.height];
            perspective(
                &mut graphics,
                &texture,
                &mut depth,
                shape,
                1.0,
                0.0,
                [255.0; 3],
                transparent,
            );
            assert_eq!(
                depth[30 * graphics.height + 30],
                if transparent.is_some() { 0.0 } else { 1.0 }
            );
        }
    }
}

#[test]
fn palette_and_rgb_keys_follow_sprite_colour_resolution() {
    let texture = Texture::from_gscr("1x1:ff0000").unwrap();
    for key in [2, 0xff0000] {
        let mut sprite = Graphics::new(640);
        sprite
            .draw_sprite("1x1:ff0000", 30.0, 30.0, Some(key), None, false)
            .unwrap();
        for shape in SHAPES {
            let mut graphics = Graphics::new(640);
            affine(&mut graphics, &texture, shape, 0.0, Some(key));
            assert_eq!(
                pixel(&graphics, 30, 30),
                pixel(&sprite, 30, 30),
                "{shape:?}"
            );
            assert_eq!(pixel(&graphics, 30, 30), 0);
            if matches!(shape, Shape::Rect) {
                continue;
            }
            let mut depth = vec![0.0; graphics.width * graphics.height];
            perspective(
                &mut graphics,
                &texture,
                &mut depth,
                shape,
                1.0,
                0.0,
                [255.0; 3],
                Some(key),
            );
            assert_eq!(depth[30 * graphics.height + 30], 0.0);
        }
    }
}

#[test]
fn transparent_draws_preserve_ink_cursor_and_sprite_collision_ownership() {
    let texture = Texture::from_gscr("1x1:ff00ff").unwrap();
    let opaque = Texture::from_gscr("1x1:20d040").unwrap();
    for shape in SHAPES {
        for is_perspective in [false, true] {
            if is_perspective && matches!(shape, Shape::Rect) {
                continue;
            }
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
            for source in [&texture, &opaque] {
                if is_perspective {
                    let mut depth = vec![0.0; graphics.width * graphics.height];
                    perspective(
                        &mut graphics,
                        source,
                        &mut depth,
                        shape,
                        1.0,
                        0.0,
                        [255.0; 3],
                        Some(6),
                    );
                } else {
                    affine(&mut graphics, source, shape, 0.0, Some(6));
                }
                assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 110.0));
                assert_eq!(
                    (
                        graphics.hit(),
                        graphics.hitcolor(),
                        graphics.hitsprite(),
                        graphics.hitid()
                    ),
                    collisions
                );
            }
            graphics
                .draw_sprite("1x1:050607", 12.0, 14.0, None, None, true)
                .unwrap();
            assert_eq!(
                graphics.hitid(),
                17,
                "{shape:?}, perspective={is_perspective}"
            );
            graphics.plot(300.0, 300.0, None);
            assert_eq!(pixel(&graphics, 300, 300), 0x020307);
        }
    }
}

#[test]
fn omitting_the_key_and_using_an_unmatched_key_preserve_opaque_api_output() {
    let texture = Texture::from_gscr("2x1:ff00ff20d040").unwrap();
    for shape in [Shape::Triangle, Shape::Quad] {
        let mut old = Graphics::new(640);
        let mut old_depth = vec![0.0; old.width * old.height];
        match shape {
            Shape::Triangle => old.perspective_textured_triangle(
                &texture,
                triangle(1.0, 0.0, [128.0; 3]),
                &mut old_depth,
                640,
                480,
            ),
            Shape::Quad => old.perspective_textured_quad(
                &texture,
                quad(1.0, 0.0, [128.0; 3]),
                &mut old_depth,
                640,
                480,
            ),
            Shape::Rect => unreachable!(),
        }
        .unwrap();
        for key in [None, Some(0x123456)] {
            let mut new = Graphics::new(640);
            let mut new_depth = vec![0.0; new.width * new.height];
            perspective(
                &mut new,
                &texture,
                &mut new_depth,
                shape,
                1.0,
                0.0,
                [128.0; 3],
                key,
            );
            assert_eq!(new.buffer(), old.buffer());
            assert_eq!(new_depth, old_depth);
            assert_eq!((new.xpos(), new.ypos()), (old.xpos(), old.ypos()));
        }
    }
}

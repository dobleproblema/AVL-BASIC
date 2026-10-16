use super::*;

fn background() -> Graphics {
    let mut graphics = Graphics::new(640);
    graphics.buffer.fill(0x204060);
    graphics.clear_buffer_dirty();
    graphics
}

#[test]
fn sprite_alpha_blends_rounds_and_tests_the_original_color_key() {
    let mut graphics = background();
    graphics
        .draw_sprite_with_alpha("2x1:ff00ff804020", 20.0, 30.0, Some(6), None, false, 0.5)
        .unwrap();
    assert_eq!(graphics.test(20.0, 30.0), 0x204060);
    assert_eq!(graphics.test(21.0, 30.0), 0x504040);
    graphics
        .draw_sprite_with_alpha("1x1:ffffff", 30.0, 30.0, None, None, false, 0.5)
        .unwrap();
    assert_eq!(graphics.test(30.0, 30.0), 0x90a0b0);
}

#[test]
fn invisible_sprites_keep_their_collision_silhouette_and_move_opacity() {
    let mut graphics = background();
    graphics.colmode(2).unwrap();
    graphics
        .draw_sprite_with_alpha("2x1:ffffff000000", 10.0, 10.0, Some(0), Some(1), false, 0.0)
        .unwrap();
    assert_eq!(graphics.test(10.0, 10.0), 0x204060);
    assert!(!graphics.buffer_dirty());
    graphics
        .draw_sprite("1x1:ff0000", 10.0, 10.0, None, Some(2), false)
        .unwrap();
    assert_eq!(graphics.hitid(), 1);
    assert_eq!(graphics.test(10.0, 10.0), 0x204060);
    graphics
        .sprite_move_with_alpha(1, 20.0, 10.0, None, Some(0.5))
        .unwrap();
    assert_eq!(graphics.test(20.0, 10.0), 0x90a0b0);
    graphics.sprite_move(1, 30.0, 10.0, None).unwrap();
    assert_eq!(graphics.test(30.0, 10.0), 0x90a0b0);
    assert_eq!(graphics.sprites[&1].alpha, 0.5);
    assert_eq!(graphics.sprites[&1].transparent, Some(0));
}

#[test]
fn collision_color_uses_the_destination_before_blending() {
    let mut graphics = background();
    graphics.colmode(3).unwrap();
    graphics.colcolor(Some(0x204060));
    graphics
        .draw_sprite_with_alpha("1x1:ffffff", 10.0, 10.0, None, Some(1), false, 0.5)
        .unwrap();
    assert_eq!(graphics.hitcolor(), 0x204060);
    assert_eq!(graphics.test(10.0, 10.0), 0x90a0b0);
    graphics
        .draw_sprite_with_alpha("1x1:ff0000", 10.0, 10.0, None, Some(2), false, 0.5)
        .unwrap();
    assert_eq!(graphics.hitid(), 1);
    assert_eq!(graphics.test(10.0, 10.0), 0x90a0b0);
}

#[test]
fn alpha_one_uses_the_opaque_rendering_paths_exactly() {
    let texture = Texture::from_gscr("2x2:ff000000ff000000ffffff00").unwrap();
    let mut actual = background();
    let mut expected = actual.clone();
    actual
        .textured_rect_with_alpha(&texture, 10.0, 10.0, 31.0, 33.0, None, Some(6), 1.0)
        .unwrap();
    expected
        .textured_rect_with_transparency(&texture, 10.0, 10.0, 31.0, 33.0, None, Some(6))
        .unwrap();
    actual
        .filled_rectangle_with_alpha(40.0, 40.0, 70.0, 60.0, Some(8), 1.0)
        .unwrap();
    expected.rectangle(40.0, 40.0, 70.0, 60.0, Some(8), true);
    actual
        .filled_triangle_with_alpha(80.0, 80.0, 100.0, 90.0, 70.0, 110.0, Some(12), 1.0)
        .unwrap();
    expected.triangle(80.0, 80.0, 100.0, 90.0, 70.0, 110.0, Some(12), true);
    actual
        .filled_circle_with_alpha(150.0, 150.0, 30.0, Some(3), Some(0.1), Some(4.0), 1.3, 1.0)
        .unwrap();
    expected
        .circle_arc(150.0, 150.0, 30.0, Some(3), true, Some(0.1), Some(4.0), 1.3)
        .unwrap();
    actual
        .draw_sprite_with_alpha("1x1:123456", 12.0, 34.0, None, Some(1), false, 1.0)
        .unwrap();
    expected
        .draw_sprite("1x1:123456", 12.0, 34.0, None, Some(1), false)
        .unwrap();
    assert_eq!(actual.buffer, expected.buffer);
    assert_eq!(actual.owner, expected.owner);
    assert_eq!(
        (actual.xpos(), actual.ypos()),
        (expected.xpos(), expected.ypos())
    );
}

fn white_quad(graphics: &mut Graphics, texture: &Texture) {
    graphics
        .textured_quad_with_alpha(
            texture, 10.0, 10.0, 0.0, 0.0, 30.0, 10.0, 1.0, 0.0, 30.0, 30.0, 1.0, 1.0, 10.0, 30.0,
            0.0, 1.0, None, 0.5,
        )
        .unwrap();
}

#[test]
fn partial_quads_and_two_triangles_blend_the_diagonal_once_for_both_windings() {
    let texture = Texture::from_gscr("1x1:ffffff").unwrap();
    let mut quad = background();
    white_quad(&mut quad, &texture);
    for y in 11..30 {
        for x in 10..30 {
            assert_eq!(quad.test(x as f64, y as f64), 0x90a0b0, "({x}, {y})");
        }
    }
    for reverse in [false, true] {
        let mut triangles = background();
        for points in [
            [[10.0, 10.0], [30.0, 10.0], [30.0, 30.0]],
            [[10.0, 10.0], [30.0, 30.0], [10.0, 30.0]],
        ] {
            let mut p = points;
            if reverse {
                p.swap(1, 2);
            }
            triangles
                .textured_triangle_with_alpha(
                    &texture, p[0][0], p[0][1], 0.0, 0.0, p[1][0], p[1][1], 0.0, 0.0, p[2][0],
                    p[2][1], 0.0, 0.0, None, 0.5,
                )
                .unwrap();
        }
        assert_eq!(triangles.buffer, quad.buffer);
    }
}

#[test]
fn partial_textures_keep_wrapping_and_color_key_with_viewport_clipping() {
    let texture = Texture::from_gscr("2x1:ff00ffff0000").unwrap();
    let mut graphics = background();
    graphics.set_origin(0, 0, Some((12, 25, 25, 12))).unwrap();
    graphics
        .textured_rect_with_alpha(
            &texture,
            10.0,
            10.0,
            26.0,
            26.0,
            Some((-2.0, 0.0, 2.0, 1.0)),
            Some(6),
            0.5,
        )
        .unwrap();
    assert_eq!(graphics.test(12.0, 14.0), 0x204060);
    assert_eq!(graphics.test(16.0, 14.0), 0x902030);
    assert_eq!(graphics.test(24.0, 14.0), 0x902030);
    assert_eq!(graphics.test(16.0, 11.0), 0x204060);
    assert_eq!(graphics.test(26.0, 14.0), 0x204060);
}

#[test]
fn filled_triangle_preserves_coverage_without_blending_the_middle_row_twice() {
    for points in [
        [10.0, 10.0, 30.0, 20.0, 15.0, 40.0],
        [30.0, 10.0, 10.0, 20.0, 25.0, 40.0],
        [10.0, 10.0, 30.0, 10.0, 20.0, 40.0],
        [20.0, 10.0, 10.0, 40.0, 30.0, 40.0],
    ] {
        let [x0, y0, x1, y1, x2, y2] = points;
        let mut actual = background();
        let mut opaque = background();
        actual
            .filled_triangle_with_alpha(x0, y0, x1, y1, x2, y2, Some(1), 0.5)
            .unwrap();
        opaque.triangle(x0, y0, x1, y1, x2, y2, Some(1), true);
        for (a, o) in actual.buffer.iter().zip(&opaque.buffer) {
            assert_eq!(*a, if *o == 0xffffff { 0x90a0b0 } else { 0x204060 });
        }
    }
}

#[test]
fn circle_and_arc_blend_each_covered_pixel_once() {
    for (start, end, aspect) in [(None, None, 1.0), (Some(0.3), Some(5.0), 1.7)] {
        let mut actual = background();
        let mut opaque = background();
        actual
            .filled_circle_with_alpha(80.0, 70.0, 20.0, Some(1), start, end, aspect, 0.5)
            .unwrap();
        opaque
            .circle_arc(80.0, 70.0, 20.0, Some(1), true, start, end, aspect)
            .unwrap();
        for (a, o) in actual.buffer.iter().zip(&opaque.buffer) {
            assert_eq!(*a, if *o == 0xffffff { 0x90a0b0 } else { 0x204060 });
        }
    }
}

#[test]
fn random_filled_triangle_coverage_matches_opaque_including_clipped_flat_cases() {
    let mut state = 0x9e3779b9u32;
    let mut actual = background();
    actual.set_origin(0, 0, Some((10, 55, 55, 10))).unwrap();
    let mut opaque = actual.clone();
    for index in 0..180 {
        let mut next = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            ((state >> 16) % 96) as f64 - 16.0
        };
        let x0 = next();
        let y0 = next();
        let x1 = next();
        let mut y1 = next();
        let x2 = next();
        let mut y2 = next();
        if index % 5 == 0 {
            y1 = y0;
        }
        if index % 7 == 0 {
            y2 = y1;
        }
        actual.buffer.fill(0x204060);
        opaque.buffer.fill(0x204060);
        actual
            .filled_triangle_with_alpha(x0, y0, x1, y1, x2, y2, Some(1), 0.5)
            .unwrap();
        opaque.triangle(x0, y0, x1, y1, x2, y2, Some(1), true);
        for y in 0..80 {
            for x in 0..80 {
                let expected = if opaque.test(x as f64, y as f64) == 0xffffff {
                    0x90a0b0
                } else {
                    0x204060
                };
                assert_eq!(
                    actual.test(x as f64, y as f64),
                    expected,
                    "triangle {index}: {x0},{y0},{x1},{y1},{x2},{y2}; pixel {x},{y}"
                );
            }
        }
    }
}

#[test]
fn alpha_zero_changes_no_pixels_and_preserves_primitive_cursor_rules() {
    let texture = Texture::from_gscr("1x1:ffffff").unwrap();
    let mut graphics = background();
    let before = graphics.buffer.clone();
    graphics.move_to(4.0, 5.0);
    graphics
        .filled_rectangle_with_alpha(0.0, 0.0, 50.0, 50.0, Some(1), 0.0)
        .unwrap();
    graphics
        .filled_circle_with_alpha(20.0, 20.0, 10.0, Some(1), None, None, 1.0, 0.0)
        .unwrap();
    assert_eq!((graphics.xpos(), graphics.ypos()), (4.0, 5.0));
    graphics
        .filled_triangle_with_alpha(0.0, 0.0, 10.0, 20.0, 40.0, 50.0, Some(1), 0.0)
        .unwrap();
    assert_eq!((graphics.xpos(), graphics.ypos()), (40.0, 50.0));
    graphics
        .textured_rect_with_alpha(&texture, 10.0, 20.0, 30.0, 40.0, None, None, 0.0)
        .unwrap();
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 40.0));
    assert_eq!(graphics.buffer, before);
    assert!(!graphics.buffer_dirty());
}

#[test]
fn invalid_alpha_and_fourth_vertex_leave_everything_unchanged() {
    let texture = Texture::from_gscr("1x1:ffffff").unwrap();
    for value in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        let mut graphics = background();
        graphics
            .draw_sprite("1x1:0000ff", 1.0, 1.0, None, Some(1), false)
            .unwrap();
        let before = graphics.clone();
        assert!(graphics
            .draw_sprite_with_alpha("1x1:ffffff", 1.0, 1.0, None, Some(1), false, value)
            .is_err());
        assert!(graphics
            .sprite_move_with_alpha(1, 2.0, 2.0, None, Some(value))
            .is_err());
        assert!(graphics
            .textured_rect_with_alpha(&texture, 0.0, 0.0, 20.0, 20.0, None, None, value)
            .is_err());
        assert!(graphics
            .filled_rectangle_with_alpha(0.0, 0.0, 20.0, 20.0, Some(1), value)
            .is_err());
        assert!(graphics
            .filled_triangle_with_alpha(0.0, 0.0, 20.0, 0.0, 10.0, 20.0, Some(1), value)
            .is_err());
        assert!(graphics
            .filled_circle_with_alpha(10.0, 10.0, 5.0, Some(1), None, None, 1.0, value)
            .is_err());
        assert_eq!(graphics.buffer, before.buffer);
        assert_eq!(graphics.owner, before.owner);
        assert_eq!(graphics.sprites[&1].alpha, before.sprites[&1].alpha);
        assert_eq!(
            (graphics.xpos(), graphics.ypos()),
            (before.xpos(), before.ypos())
        );
        assert_eq!(graphics.buffer_dirty, before.buffer_dirty);
    }
    let mut graphics = background();
    let before = graphics.clone();
    assert!(graphics
        .textured_quad_with_alpha(
            &texture,
            0.0,
            0.0,
            0.0,
            0.0,
            20.0,
            0.0,
            1.0,
            0.0,
            20.0,
            20.0,
            1.0,
            1.0,
            0.0,
            20.0,
            f64::NAN,
            1.0,
            None,
            0.5
        )
        .is_err());
    assert_eq!(graphics.buffer, before.buffer);
    assert_eq!(
        (graphics.xpos(), graphics.ypos()),
        (before.xpos(), before.ypos())
    );
}

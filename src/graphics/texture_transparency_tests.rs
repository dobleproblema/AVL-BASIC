use super::*;

const BACKGROUND: u32 = 0x123456;

fn graphics_with_collision_state() -> Graphics {
    let mut graphics = Graphics::new(640);
    graphics.buffer.fill(BACKGROUND);
    graphics.owner.fill(23);
    graphics.current_color = 0xa173c5;
    graphics.collision_mode = 3;
    graphics.collision_color = Some(0x234567);
    graphics.hit = true;
    graphics.hit_color = true;
    graphics.hit_sprite = true;
    graphics.hit_id = 23;
    graphics.hit_color_rgb = 0x234567;
    graphics.mask = 0x5a;
    graphics.pen_width = 7;
    graphics.clear_buffer_dirty();
    graphics
}

fn assert_drawing_state_unchanged(actual: &Graphics, before: &Graphics) {
    assert_eq!(actual.owner, before.owner);
    assert_eq!(actual.current_color, before.current_color);
    assert_eq!(actual.background_color, before.background_color);
    assert_eq!(actual.mask, before.mask);
    assert_eq!(actual.pen_width, before.pen_width);
    assert_eq!(actual.collision_mode, before.collision_mode);
    assert_eq!(actual.collision_color, before.collision_color);
    assert_eq!(actual.hit, before.hit);
    assert_eq!(actual.hit_color, before.hit_color);
    assert_eq!(actual.hit_sprite, before.hit_sprite);
    assert_eq!(actual.hit_id, before.hit_id);
    assert_eq!(actual.hit_color_rgb, before.hit_color_rgb);
}

fn quad(q: f64, light: f64) -> [[f64; 8]; 4] {
    [
        [10.0, 10.0, q, 0.0, 0.0, light, 255.0, 255.0],
        [18.0, 10.0, q, 2.0, 0.0, light, 255.0, 255.0],
        [18.0, 18.0, q, 2.0, 1.0, light, 255.0, 255.0],
        [10.0, 18.0, q, 0.0, 1.0, light, 255.0, 255.0],
    ]
}

#[test]
fn affine_texture_key_uses_palette_resolution_and_wrapped_source_texels() {
    let texture = Texture::from_gscr("2x1:ff00ffff0000").unwrap();
    let mut graphics = graphics_with_collision_state();
    let before = graphics.clone();
    graphics
        .textured_rect_with_transparency(
            &texture,
            10.0,
            10.0,
            26.0,
            18.0,
            Some((-2.0, 0.0, 2.0, 1.0)),
            Some(6), // SPRITE palette entry 6 is magenta.
        )
        .unwrap();
    assert_eq!(graphics.test(12.0, 12.0), BACKGROUND as i32);
    assert_eq!(graphics.test(16.0, 12.0), 0xff0000);
    assert_eq!(graphics.test(20.0, 12.0), BACKGROUND as i32);
    assert_eq!(graphics.test(24.0, 12.0), 0xff0000);
    assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 18.0));
    assert_drawing_state_unchanged(&graphics, &before);
}

#[test]
fn perspective_key_is_tested_before_lighting_and_holes_accept_later_background() {
    // Palette color 30 matches the first original texel. Lighting would turn
    // it into 0x400000; the second texel becomes the key color after lighting
    // and must nevertheless remain visible.
    let foreground = Texture::from_gscr("2x1:800000ff0000").unwrap();
    let background = Texture::from_gscr("1x1:0000ff").unwrap();
    for light in [255.0, 128.0] {
        for foreground_first in [false, true] {
            let mut graphics = graphics_with_collision_state();
            let before = graphics.clone();
            let (width, height) = (graphics.width, graphics.height);
            let mut depth = vec![0.0; width * height];
            for front in [foreground_first, !foreground_first] {
                if front {
                    graphics
                        .perspective_textured_quad_with_transparency(
                            &foreground,
                            quad(1.0, light),
                            &mut depth,
                            width,
                            height,
                            Some(30),
                        )
                        .unwrap();
                } else {
                    graphics
                        .perspective_textured_quad(
                            &background,
                            quad(0.5, 255.0),
                            &mut depth,
                            width,
                            height,
                        )
                        .unwrap();
                }
            }
            assert_eq!(graphics.test(11.0, 12.0), 0x0000ff);
            assert_eq!(depth[11 * height + 12], 0.5);
            let expected = if light == 255.0 { 0xff0000 } else { 0x800000 };
            assert_eq!(graphics.test(16.0, 12.0), expected);
            assert_eq!(depth[16 * height + 12], 1.0);
            assert_drawing_state_unchanged(&graphics, &before);
        }
    }
}

#[test]
fn fully_transparent_perspective_quad_keeps_depth_image_and_dirty_flag() {
    let texture = Texture::from_gscr("1x1:ff00ff").unwrap();
    for light in [255.0, 64.0] {
        let mut graphics = graphics_with_collision_state();
        graphics.set_origin(13, -2, None).unwrap();
        let before = graphics.clone();
        let (width, height) = (graphics.width, graphics.height);
        let mut depth = vec![0.25; width * height];
        graphics
            .perspective_textured_quad_with_transparency(
                &texture,
                quad(1.0, light),
                &mut depth,
                width,
                height,
                Some(0xff00ff),
            )
            .unwrap();
        assert_eq!(graphics.buffer, before.buffer);
        assert!(depth.iter().all(|q| *q == 0.25));
        assert!(!graphics.buffer_dirty());
        assert_eq!((graphics.xpos(), graphics.ypos()), (10.0, 18.0));
        assert_eq!((graphics.cursor_x, graphics.cursor_y), (23, 16));
        assert_drawing_state_unchanged(&graphics, &before);
    }
}

#[test]
fn invalid_fourth_vertex_is_atomic_even_with_a_transparent_texture() {
    let texture = Texture::from_gscr("1x1:ff00ff").unwrap();
    for transparent in [None, Some(6)] {
        let mut graphics = graphics_with_collision_state();
        let before = graphics.clone();
        let error = graphics
            .textured_quad_with_transparency(
                &texture,
                10.0,
                10.0,
                0.0,
                0.0,
                18.0,
                10.0,
                1.0,
                0.0,
                18.0,
                18.0,
                1.0,
                1.0,
                10.0,
                18.0,
                f64::NAN,
                1.0,
                transparent,
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(graphics.buffer, before.buffer);
        assert_eq!(graphics.buffer_dirty(), before.buffer_dirty());
        assert_eq!(
            (graphics.xpos(), graphics.ypos()),
            (before.xpos(), before.ypos())
        );
        assert_drawing_state_unchanged(&graphics, &before);

        let (width, height) = (graphics.width, graphics.height);
        let mut depth = vec![0.0; width * height];
        let mut vertices = quad(1.0, 255.0);
        vertices[3][3] = f64::MAX;
        vertices[3][2] = 2.0; // Finite inputs, but q*u overflows.
        let error = graphics
            .perspective_textured_quad_with_transparency(
                &texture,
                vertices,
                &mut depth,
                width,
                height,
                transparent,
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(graphics.buffer, before.buffer);
        assert!(depth.iter().all(|q| *q == 0.0));
        assert_eq!(graphics.buffer_dirty(), before.buffer_dirty());
        assert_eq!(
            (graphics.xpos(), graphics.ypos()),
            (before.xpos(), before.ypos())
        );
        assert_drawing_state_unchanged(&graphics, &before);
    }
}

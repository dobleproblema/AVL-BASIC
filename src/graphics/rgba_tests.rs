use super::*;
use std::fs::File;

#[test]
fn rgba_sprites_reject_invalid_dimensions_and_signed_pixels() {
    let mut graphics = Graphics::new(640);
    for sprite in [
        "0x2147483648@RGBA:",
        "2147483648x0@RGBA:",
        "0x1@RGBA:",
        "1x1@RGBA:+abcdef0",
    ] {
        assert_eq!(
            graphics
                .draw_sprite(sprite, 0.0, 0.0, None, None, false)
                .unwrap_err()
                .code,
            ErrorCode::InvalidValue
        );
    }
}

#[test]
fn crop_copies_exact_rgb_and_rgba_rows_from_the_upper_left() {
    let rgb = "3x2:110000120000130000210000220000230000";
    let rgba = "3x2@RGBA:110000001200007f130000ff210000002200007f230000ff";
    assert_eq!(
        Graphics::crop_gscr(rgb, 1, 1, 2, 1).unwrap(),
        "2x1:220000230000"
    );
    assert_eq!(
        Graphics::crop_gscr(rgba, 1, 0, 2, 2).unwrap(),
        "2x2@RGBA:1200007f130000ff2200007f230000ff"
    );
    for (x, y, width, height) in [
        (3, 0, 1, 1),
        (0, 2, 1, 1),
        (0, 0, 0, 1),
        (usize::MAX, 0, 1, 1),
    ] {
        assert_eq!(
            Graphics::crop_gscr(rgba, x, y, width, height)
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
    }
    assert_eq!(
        Graphics::crop_gscr("2x1@RGBA:0000000zffffffff", 1, 0, 1, 1)
            .unwrap_err()
            .code,
        ErrorCode::InvalidValue
    );
    assert_eq!(
        Graphics::crop_gscr(&format!("{}x2@RGBA:", usize::MAX), 0, 0, 1, 1)
            .unwrap_err()
            .code,
        ErrorCode::InvalidValue
    );
}

#[test]
fn rgba_png_load_expands_palette_transparency_and_save_round_trips_alpha() {
    let dir = tempfile::tempdir().unwrap();
    let indexed = dir.path().join("indexed.png");
    let saved = dir.path().join("saved.png");
    let mut encoder = png::Encoder::new(File::create(&indexed).unwrap(), 2, 1);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::One);
    encoder.set_palette(vec![0xff, 0, 0, 0, 0xff, 0]);
    encoder.set_trns(vec![0, 128]);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0b0100_0000]).unwrap();
    writer.finish().unwrap();

    let encoded = Graphics::load_png_to_gscr_rgba(&indexed).unwrap();
    assert_eq!(encoded, "2x1@RGBA:ff00000000ff0080");
    Graphics::save_gscr_png(&encoded, &saved).unwrap();
    assert_eq!(Graphics::load_png_to_gscr_rgba(&saved).unwrap(), encoded);
    assert_eq!(
        Graphics::load_png_to_gscr(&saved).unwrap(),
        "2x1:ff000000ff00"
    );
}

#[test]
fn rgba_sprites_apply_source_and_uniform_alpha_but_keep_the_source_collision_mask() {
    let mut graphics = Graphics::new(640);
    graphics.buffer.fill(0);
    graphics.colmode(2).unwrap();
    let sprite = "4x1@RGBA:ff00000000ff00800000ffffff00ff80";
    graphics
        .draw_sprite_with_alpha(sprite, 10.0, 10.0, Some(6), Some(1), false, 0.5)
        .unwrap();
    assert_eq!(graphics.test(10.0, 10.0), 0);
    assert_eq!(graphics.test(11.0, 10.0), 0x004000);
    assert_eq!(graphics.test(12.0, 10.0), 0x000080);
    assert_eq!(graphics.test(13.0, 10.0), 0);

    for (x, owner) in [(10.0, 0), (11.0, 1), (12.0, 1), (13.0, 0)] {
        graphics
            .draw_sprite("1x1:ffffff", x, 10.0, None, None, true)
            .unwrap();
        assert_eq!(graphics.hitid(), owner);
    }
    let occupied = graphics.sprites[&1].pixels.clone();
    graphics
        .draw_sprite(sprite, 10.0, 10.0, Some(6), Some(1), true)
        .unwrap();
    assert_eq!(graphics.sprites[&1].pixels, occupied);

    graphics.sprite_move(1, 20.0, 10.0, None).unwrap();
    assert_eq!(graphics.test(21.0, 10.0), 0x004000);
    graphics
        .draw_sprite("1x1:ffffff", 11.0, 10.0, None, None, true)
        .unwrap();
    assert_eq!(graphics.hitid(), 0);

    graphics
        .sprite_move_with_alpha(1, 30.0, 10.0, None, Some(0.0))
        .unwrap();
    assert_eq!(graphics.test(31.0, 10.0), 0);
    graphics
        .draw_sprite("1x1:ffffff", 31.0, 10.0, None, None, true)
        .unwrap();
    assert_eq!(graphics.hitid(), 1);
}

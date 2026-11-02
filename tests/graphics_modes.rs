use avl_basic::{ErrorCode, Graphics};

fn pixel(graphics: &Graphics, x: usize, y: usize) -> u32 {
    graphics.buffer()[(graphics.height - 1 - y) * graphics.width + x]
}

#[test]
fn all_modes_keep_their_dimensions_and_clip_at_their_own_edges() {
    let mut graphics = Graphics::new(640);
    for (width, height) in [
        (640, 480),
        (640, 360),
        (960, 540),
        (800, 600),
        (1024, 768),
        (1280, 720),
        (960, 540),
    ] {
        graphics.set_mode_dimensions(width, height).unwrap();
        assert_eq!((graphics.width, graphics.height), (width, height));
        assert_eq!(graphics.buffer().len(), width * height);
        graphics.set_ink_rgb(0x12, 0x34, 0x56).unwrap();
        for (x, y) in [
            (0, 0),
            (width - 1, 0),
            (0, height - 1),
            (width - 1, height - 1),
        ] {
            graphics.plot(x as f64, y as f64, None);
            assert_eq!(pixel(&graphics, x, y), 0x123456);
        }
        let before = graphics.buffer().to_vec();
        for (x, y) in [
            (-1.0, 0.0),
            (width as f64, 0.0),
            (0.0, -1.0),
            (0.0, height as f64),
        ] {
            graphics.plot(x, y, Some(2));
        }
        assert_eq!(graphics.buffer(), before);
    }
}

#[test]
fn low_widescreen_resets_at_its_own_size_and_legacy_mode_640_still_selects_480() {
    let mut graphics = Graphics::new(640);
    graphics.set_mode_dimensions(640, 360).unwrap();
    graphics.set_paper_rgb(0x12, 0x34, 0x56).unwrap();
    graphics.clg();
    graphics.reset_state();
    assert_eq!((graphics.width, graphics.height), (640, 360));
    assert_eq!(graphics.buffer().len(), 640 * 360);
    assert!(graphics.buffer().iter().all(|pixel| *pixel == 0));
    graphics.plot(639.0, 359.0, None);
    assert_eq!(pixel(&graphics, 639, 359), 0xFFFFFF);

    graphics.set_mode(640).unwrap();
    assert_eq!((graphics.width, graphics.height), (640, 480));
    assert_eq!(graphics.buffer().len(), 640 * 480);
    graphics.plot(639.0, 479.0, None);
    assert_eq!(pixel(&graphics, 639, 479), 0xFFFFFF);
    assert_eq!(pixel(&graphics, 639, 359), 0);
}

#[test]
fn explicit_mode_and_low_widescreen_scale_validation_do_not_mutate_on_error() {
    let mut graphics = Graphics::new(640);
    graphics.plot(639.0, 479.0, Some(2));
    let before = graphics.buffer().to_vec();
    for (width, height) in [(0, 0), (640, 0), (800, 360), (960, 600), (1920, 1080)] {
        assert_eq!(
            graphics
                .set_mode_dimensions(width, height)
                .unwrap_err()
                .code,
            ErrorCode::InvalidArgument
        );
        assert_eq!((graphics.width, graphics.height), (640, 480));
        assert_eq!(graphics.buffer(), before);
    }

    graphics
        .set_scale(Some((-1.0, 1.0, -1.0, 1.0, 180)))
        .unwrap();
    assert_eq!(
        graphics.set_mode_dimensions(640, 360).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
    assert_eq!((graphics.width, graphics.height), (640, 480));
    assert_eq!(graphics.scale_border(), 180);
    assert_eq!(graphics.buffer(), before);

    graphics
        .set_scale(Some((-1.0, 1.0, -1.0, 1.0, 10)))
        .unwrap();
    graphics.set_mode_dimensions(640, 360).unwrap();
    assert_eq!(graphics.scale_bounds(), (-1.0, 1.0, -1.0, 1.0));
    assert_eq!(graphics.scale_border(), 10);
    graphics.set_ink_rgb(0xAB, 0xCD, 0xEF).unwrap();
    graphics.plot(-1.0, -1.0, None);
    graphics.plot(1.0, 1.0, None);
    assert_eq!(pixel(&graphics, 10, 10), 0xABCDEF);
    assert_eq!(pixel(&graphics, 629, 349), 0xABCDEF);
}

#[test]
fn mode_960_remaps_a_preserved_user_scale_to_the_widescreen_canvas() {
    let mut graphics = Graphics::new(640);
    graphics
        .set_scale(Some((-1.0, 1.0, -1.0, 1.0, 10)))
        .unwrap();
    graphics.set_mode(960).unwrap();
    assert_eq!(graphics.scale_bounds(), (-1.0, 1.0, -1.0, 1.0));
    assert_eq!(graphics.scale_border(), 10);
    graphics.set_ink_rgb(0xAB, 0xCD, 0xEF).unwrap();
    graphics.plot(-1.0, -1.0, None);
    graphics.plot(1.0, 1.0, None);
    assert_eq!(pixel(&graphics, 10, 10), 0xABCDEF);
    assert_eq!(pixel(&graphics, 949, 529), 0xABCDEF);

    // A BORDER valid in a taller legacy mode must be checked against 540, not 600.
    graphics.set_scale(None).unwrap();
    graphics.set_mode(1024).unwrap();
    graphics
        .set_scale(Some((-1.0, 1.0, -1.0, 1.0, 270)))
        .unwrap();
    let before = graphics.buffer().to_vec();
    assert_eq!(
        graphics.set_mode(960).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
    assert_eq!((graphics.width, graphics.height), (1024, 768));
    assert_eq!(graphics.scale_border(), 270);
    assert_eq!(graphics.buffer(), before);
}

#[test]
fn widescreen_capture_roundtrips_and_rejects_another_mode_without_mutation() {
    let mut graphics = Graphics::new(960);
    graphics.set_ink_rgb(0x10, 0x20, 0x30).unwrap();
    graphics.rectangle(0.0, 0.0, 959.0, 539.0, None, false);
    graphics.plot(480.0, 270.0, Some(2));
    let expected = graphics.buffer().to_vec();
    let capture = graphics.capture_screen();
    assert!(capture.starts_with("960x540:"));
    graphics.clg();
    graphics.restore_screen(&capture).unwrap();
    assert_eq!(graphics.buffer(), expected);

    let legacy_capture = Graphics::new(800).capture_screen();
    assert_eq!(
        graphics.restore_screen(&legacy_capture).unwrap_err().code,
        ErrorCode::InvalidValue
    );
    assert_eq!(graphics.buffer(), expected);
}

#[test]
fn low_widescreen_capture_rejects_same_width_legacy_height_without_mutation() {
    let mut graphics = Graphics::new(640);
    graphics.set_mode_dimensions(640, 360).unwrap();
    graphics.set_ink_rgb(0x10, 0x20, 0x30).unwrap();
    graphics.rectangle(0.0, 0.0, 639.0, 359.0, None, false);
    graphics.plot(320.0, 180.0, Some(2));
    let expected = graphics.buffer().to_vec();
    let capture = graphics.capture_screen();
    assert!(capture.starts_with("640x360:"));
    graphics.clg();
    graphics.restore_screen(&capture).unwrap();
    assert_eq!(graphics.buffer(), expected);

    let legacy_capture = Graphics::new(640).capture_screen();
    assert_eq!(
        graphics.restore_screen(&legacy_capture).unwrap_err().code,
        ErrorCode::InvalidValue
    );
    assert_eq!(graphics.buffer(), expected);
}

#[test]
fn widescreen_depth_uses_the_new_height_and_validates_before_drawing() {
    let mut graphics = Graphics::new(960);
    let triangle = [
        [950.0, 530.0, 1.0, 255.0, 255.0, 255.0],
        [959.0, 530.0, 1.0, 255.0, 255.0, 255.0],
        [950.0, 539.0, 1.0, 255.0, 255.0, 255.0],
    ];
    let mut depth = vec![0.0; 960 * 540];
    graphics
        .gouraud_triangle(triangle, &mut depth, 960, 540)
        .unwrap();
    assert_eq!(pixel(&graphics, 952, 532), 0xFFFFFF);
    assert!((depth[952 * 540 + 532] - 1.0).abs() < 1e-12);
    assert_eq!(depth[0], 0.0);

    let before = graphics.buffer().to_vec();
    let mut wrong_depth = vec![0.0; 960 * 600];
    assert_eq!(
        graphics
            .gouraud_triangle(triangle, &mut wrong_depth, 960, 600)
            .unwrap_err()
            .code,
        ErrorCode::InvalidValue
    );
    assert_eq!(graphics.buffer(), before);
    assert!(wrong_depth.iter().all(|value| *value == 0.0));
}

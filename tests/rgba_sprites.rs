use avl_basic::{ErrorCode, Interpreter};
use std::fs::File;

#[test]
fn bload_rgba_and_crop_use_the_image_coordinates_and_keep_all_channels() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("atlas AS RGBA.png");
    let mut encoder = png::Encoder::new(File::create(path).unwrap(), 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    let rgba = [
        255, 0, 0, 0, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 64,
    ];
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();

    let mut basic = Interpreter::new();
    basic.root_dir = temp.path().to_path_buf();
    basic.current_dir = temp.path().to_path_buf();
    for line in [
        "10 BLOAD \"atlas AS RGBA.png\",Atlas$ as rgba",
        "20 BLOAD \"atlas AS RGBA.png\",Legacy$",
        "30 DIM Tile$(1) : Tile$(0)=CROP$(Atlas$,1,0,1,2)",
        "40 PRINT Tile$(0) : PRINT CROP$(Legacy$,0,1,2,1)",
        "50 BSAVE \"crop.png\",Tile$(0)",
    ] {
        basic.process_immediate(line).unwrap();
    }
    basic.run_loaded().unwrap();
    assert_eq!(
        basic.take_output(),
        "1x2@RGBA:00ff0080ffffff40\n2x1:0000ffffffff\n"
    );

    let decoder = png::Decoder::new(File::open(temp.path().join("crop.png")).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(
        (info.width, info.height, info.color_type),
        (1, 2, png::ColorType::Rgba)
    );
    assert_eq!(
        &bytes[..info.buffer_size()],
        &[0, 255, 0, 128, 255, 255, 255, 64]
    );
}

#[test]
fn crop_and_bload_validate_geometry_format_and_destination() {
    let mut basic = Interpreter::new();
    basic.process_immediate("S$=\"2x1:112233445566\"").unwrap();
    for (source, expected) in [
        ("T$=CROP$(S$,-1,0,1,1)", ErrorCode::InvalidArgument),
        ("T$=CROP$(S$,0,0,1.5,1)", ErrorCode::InvalidArgument),
        ("T$=CROP$(S$,0,0,0,1)", ErrorCode::InvalidArgument),
        ("T$=CROP$(S$,1,0,2,1)", ErrorCode::InvalidArgument),
        ("T$=CROP$(S$,0,0,1)", ErrorCode::ArgumentMismatch),
        ("T$=CROP$(\"bad\",0,0,1,1)", ErrorCode::InvalidValue),
        ("BLOAD \"missing.png\" AS RGBA", ErrorCode::ArgumentMismatch),
        ("BLOAD \"missing.png\",S$ AS RGBX", ErrorCode::Syntax),
    ] {
        assert_eq!(
            basic.process_immediate(source).unwrap_err().code,
            expected,
            "{source}"
        );
    }
}

#[test]
fn rgba_sprite_without_a_color_key_keeps_its_id_and_visual_opacity() {
    let mut basic = Interpreter::new();
    for source in [
        "MODE 640 : PAPER 0 : CLG OFFSCREEN : COLMODE 2",
        "S$=\"2x1@RGBA:ff000000ff000080\" : P$=\"1x1:ffffff\"",
        "SPRITE S$,5,5,,17 ALPHA .5",
        "SPRITE HITTEST P$,5,5,,22",
    ] {
        basic.process_immediate(source).unwrap();
    }
    assert_eq!(basic.graphics.hitsprite(), 0);
    basic
        .process_immediate("SPRITE HITTEST P$,6,5,,22")
        .unwrap();
    assert_eq!(basic.graphics.hitid(), 17);
    basic.process_immediate("SPRITE MOVE 17,8,5").unwrap();
    let row = (basic.graphics.height - 1 - 5) * basic.graphics.width;
    assert_eq!(basic.graphics.buffer()[row + 8], 0);
    assert_eq!(basic.graphics.buffer()[row + 9], 0x400000);

    basic
        .process_immediate("SPRITE MOVE 17,11,5 ALPHA 0 : SPRITE HITTEST P$,12,5,,22")
        .unwrap();
    assert_eq!(basic.graphics.buffer()[row + 12], 0);
    assert_eq!(basic.graphics.hitid(), 17);
}

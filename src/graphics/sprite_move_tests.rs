use super::*;

const BAR: &str = "4x1:112233112233112233112233";
const DOT: &str = "1x1:abcdef";

fn owners_at_y(graphics: &Graphics, y: usize) -> &[i32] {
    let start = (graphics.height - 1 - y) * graphics.width;
    &graphics.owner[start..start + graphics.width]
}

#[test]
fn moving_and_replacing_a_sprite_only_clear_pixels_it_still_owns() {
    let mut graphics = Graphics::new_canvas(8, 4);
    graphics
        .draw_sprite(BAR, -1.0, 2.0, None, Some(1), false)
        .unwrap();
    graphics
        .draw_sprite(DOT, 1.0, 2.0, None, Some(2), false)
        .unwrap();
    graphics.colmode(2).unwrap();
    graphics
        .draw_sprite(BAR, 0.0, 2.0, None, Some(1), true)
        .unwrap();
    assert_eq!(graphics.hitid(), 2);
    assert_eq!(owners_at_y(&graphics, 2), [1, 2, 1, 0, 0, 0, 0, 0]);

    // Releasing the first footprint must preserve the newer sprite at x=1;
    // collision mode also prevents the moved sprite from reclaiming that pixel.
    graphics.sprite_move(1, 1.0, 2.0, None).unwrap();
    assert_eq!(graphics.hitid(), 2);
    assert_eq!(owners_at_y(&graphics, 2), [0, 2, 1, 1, 1, 0, 0, 0]);
    assert_eq!(graphics.test(0.0, 2.0), 0x112233);
    assert_eq!(graphics.test(1.0, 2.0), 0xabcdef);

    graphics
        .draw_sprite(DOT, 7.0, 2.0, None, Some(1), false)
        .unwrap();
    assert_eq!(owners_at_y(&graphics, 2), [0, 2, 0, 0, 0, 0, 0, 1]);
    graphics.sprite_move(1, 20.0, 2.0, None).unwrap();
    assert_eq!(owners_at_y(&graphics, 2), [0, 2, 0, 0, 0, 0, 0, 0]);
    graphics.sprite_move(1, 2.0, 2.0, None).unwrap();
    assert_eq!(graphics.test(2.0, 2.0), 0xabcdef);
    graphics.sprite_delete(1);
    assert_eq!(owners_at_y(&graphics, 2), [0, 2, 0, 0, 0, 0, 0, 0]);
    assert!(graphics.sprite_move(1, 2.0, 2.0, None).is_err());
}

#[test]
fn partial_clear_and_delete_preserve_reassigned_pixels() {
    let mut graphics = Graphics::new_canvas(8, 4);
    graphics
        .draw_sprite(BAR, 1.0, 1.0, None, Some(1), false)
        .unwrap();
    graphics.set_origin(0, 0, Some((3, 7, 3, 0))).unwrap();
    graphics.clg();
    assert_eq!(owners_at_y(&graphics, 1), [0, 1, 1, 0, 0, 0, 0, 0]);
    graphics
        .draw_sprite(BAR, 2.0, 1.0, None, Some(2), false)
        .unwrap();
    graphics.sprite_delete(1);
    assert_eq!(owners_at_y(&graphics, 1), [0, 0, 0, 2, 2, 2, 0, 0]);
}

#[test]
fn moving_after_a_collision_reset_keeps_the_new_owners_and_the_old_mask() {
    for restore in [false, true] {
        let mut graphics = Graphics::new_canvas(8, 4);
        // Invisible uniform alpha still owns the first pixel. Source alpha
        // zero and the magenta color key leave holes in the other two.
        let masked = "3x1@RGBA:112233ff44556600ff00ffff";
        graphics
            .draw_sprite_with_alpha(masked, 1.0, 1.0, Some(6), Some(1), false, 0.0)
            .unwrap();
        assert_eq!(owners_at_y(&graphics, 1), [0, 1, 0, 0, 0, 0, 0, 0]);
        if restore {
            graphics.restore_screen(&graphics.capture_screen()).unwrap();
        } else {
            graphics.colreset();
        }
        graphics
            .draw_sprite(DOT, 1.0, 1.0, None, Some(2), false)
            .unwrap();
        graphics.sprite_move(1, 4.0, 1.0, None).unwrap();
        assert_eq!(owners_at_y(&graphics, 1), [0, 2, 0, 0, 1, 0, 0, 0]);
        assert_eq!(graphics.test(4.0, 1.0), 0);
    }
}

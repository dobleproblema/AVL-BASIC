use super::*;

fn atlas() -> Texture {
    let width = 127;
    let height = 83;
    Texture {
        width,
        height,
        pixels: (0..width * height)
            .map(|i| if i % 37 == 0 { 0xff00ff } else { i as u32 })
            .collect(),
    }
}

fn compare_with_quad(
    initial: &Graphics,
    texture: &Texture,
    [x0, y0, x1, y1]: [f64; 4],
    uv: (f64, f64, f64, f64),
    key: Option<i32>,
    alpha: f64,
) {
    let mut actual = initial.clone();
    let mut expected = initial.clone();
    actual
        .textured_rect_with_alpha(texture, x0, y0, x1, y1, Some(uv), key, alpha)
        .unwrap();
    let (u0, v0, u1, v1) = uv;
    expected
        .textured_quad_with_alpha(
            texture, x0, y0, u0, v0, x1, y0, u1, v0, x1, y1, u1, v1, x0, y1, u0, v1, key, alpha,
        )
        .unwrap();
    let mismatch = actual
        .buffer
        .iter()
        .zip(&expected.buffer)
        .position(|(a, b)| a != b)
        .map(|i| (i % actual.width, i / actual.width));
    assert_eq!(
        mismatch, None,
        "rect=({x0},{y0},{x1},{y1}), UV={uv:?}, key={key:?}, alpha={alpha}"
    );
    assert_eq!((actual.xpos(), actual.ypos()), (x0, y1));
    assert_eq!(actual.owner, initial.owner);
    assert_eq!(actual.buffer_dirty(), expected.buffer_dirty());
}

#[test]
fn unit_texture_translations_preserve_clipping_and_the_last_canvas_row() {
    let texture = atlas();
    let mut initial = Graphics::new_canvas(97, 61);
    initial.buffer.fill(0x123456);
    initial.owner.fill(7);
    let uv = (11.125, 8.875, 108.125, 69.875);
    let dest = [0.0, 0.0, 97.0, 61.0];
    compare_with_quad(&initial, &texture, dest, uv, None, 1.0);
    let mut actual = initial.clone();
    actual
        .textured_rect(&texture, 0.0, 0.0, 97.0, 61.0, Some(uv))
        .unwrap();
    assert!(actual.buffer[60 * 97..].iter().all(|&p| p == 0x123456));
    assert_eq!(actual.buffer[0], texture.sample_wrapped(11.625, 68.375));

    initial.set_origin(7, -3, Some((3, 75, 47, 4))).unwrap();
    compare_with_quad(&initial, &texture, dest, uv, None, 1.0);
    // SCALE doubles both axes inside this viewport. The UV span matches
    // the physical rectangle even though it differs from the user coordinates.
    initial.set_scale(Some((0.0, 36.0, 0.0, 21.5, 0))).unwrap();
    compare_with_quad(
        &initial,
        &texture,
        [-4.0, -2.0, 33.0, 20.0],
        (7.25, 9.75, 81.25, 53.75),
        None,
        1.0,
    );
}

#[test]
fn half_texel_ties_and_near_unit_uv_slopes_match_triangle_rounding() {
    let texture = atlas();
    let initial = Graphics::new_canvas(97, 61);
    let half = 8.5_f64;
    let dest = [2.0, 3.0, 91.0, 56.0];
    for start in [
        half.next_down().next_down(),
        half.next_down(),
        half,
        half.next_up(),
        half.next_up().next_up(),
    ] {
        for end_offset in [0.0, -0.0000001, 0.0000001] {
            compare_with_quad(
                &initial,
                &texture,
                dest,
                (
                    start,
                    start,
                    start + 89.0 + end_offset,
                    start + 53.0 - end_offset,
                ),
                None,
                1.0,
            );
        }
    }
}

#[test]
fn atlas_edges_wrapping_flips_and_color_keys_preserve_texels() {
    let texture = atlas();
    let mut initial = Graphics::new_canvas(97, 61);
    initial.buffer.fill(0x123456);
    let dest = [1.0, 1.0, 90.0, 54.0];
    for (x, y) in [
        (0.0, 0.0),
        (38.0, 30.0),
        (-1.0, 0.0),
        (38.75, 30.75),
        (127.0, 83.0),
    ] {
        compare_with_quad(
            &initial,
            &texture,
            dest,
            (x, y, x + 89.0, y + 53.0),
            None,
            1.0,
        );
    }
    let uv = (1.25, 5.75, 90.25, 58.75);
    compare_with_quad(&initial, &texture, [90.0, 1.0, 1.0, 54.0], uv, None, 1.0);
    compare_with_quad(
        &initial,
        &texture,
        dest,
        (uv.0, uv.3, uv.2, uv.1),
        None,
        1.0,
    );
    compare_with_quad(&initial, &texture, dest, uv, Some(0xff00ff), 1.0);
    compare_with_quad(&initial, &texture, dest, uv, None, 0.4);

    // The clipped source lies inside the atlas even though the UV endpoints
    // are far away. Cancellation near texel boundaries must still match.
    let large_dest = [-1_000_000.0, -900_000.0, 90.0, 54.0];
    compare_with_quad(
        &initial,
        &texture,
        large_dest,
        (-999_991.5, -899_995.5, 98.5, 58.5),
        None,
        1.0,
    );
}

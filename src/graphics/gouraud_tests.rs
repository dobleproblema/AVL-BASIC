use super::Graphics;

#[test]
fn gouraud_bands_and_simd_match_scalar_color_and_depth() {
    for (width, height) in [(81, 63), (517, 389)] {
        let mut actual = Graphics::new_canvas(width, height);
        actual.buffer.fill(0x175391);
        actual.owner.fill(12);
        actual.clear_buffer_dirty();
        actual.w_left = 3;
        actual.w_right = width as i32 - 2;
        actual.w_top = 2;
        actual.w_bottom = height as i32 - 2;
        let mut expected = actual.clone();
        let mut actual_depth: Vec<f64> = (0..width * height)
            .map(|index| match index % 19 {
                0 => f64::from_bits(0x7ff8000000000023),
                1 => f64::INFINITY,
                2 => f64::NEG_INFINITY,
                3 => -0.0,
                4 => 0.65,
                _ => 0.0,
            })
            .collect();
        let mut expected_depth = actual_depth.clone();
        let width_f = width as f64;
        let height_f = height as f64;
        let triangles = [
            [
                [-9.125, 0.75, 0.2, -140.5, 50.3, 95.0],
                [width_f - 7.5, -5.5, 0.9, 570.0, 27.5, 90.4],
                [width_f + 0.25, height_f + 13.25, 1.1, 90.5, 490.0, 2.3],
            ],
            [
                [-12.5, -9.5, 0.21, 12.0, 145.6, 22.0],
                [width_f - 0.45, height_f + 2.6, 1.65, 120.5, 27.5, 190.4],
                [4.25, height_f - 3.65, 0.81, 190.5, 10.5, 260.0],
            ],
            [
                [9.0, 3.0, f64::from_bits(1), 255.0, 2.0, 40.0],
                [width_f - 7.0, 5.5, 0.7, 249.5, 27.5, 190.4],
                [12.0, height_f, 0.7, 0.0, 255.0, 250.0],
            ],
        ];
        for vertices in triangles {
            actual
                .gouraud_triangle_raster::<true>(vertices, &mut actual_depth, height)
                .unwrap();
            expected
                .gouraud_triangle_raster::<false>(vertices, &mut expected_depth, height)
                .unwrap();
            assert!(expected.buffer_dirty);
            assert_eq!(actual.buffer_dirty, expected.buffer_dirty);
            assert_eq!(actual.buffer, expected.buffer);
            assert_eq!(actual.owner, expected.owner);
            for (a, b) in actual_depth.iter().zip(&expected_depth) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
            actual.clear_buffer_dirty();
            expected.clear_buffer_dirty();
        }
    }
}

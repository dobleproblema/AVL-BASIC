use super::*;

#[derive(Clone, Copy)]
enum TestedPath {
    Empty = 0,
    Checked = 1,
    Bounded = 2,
}

fn unit(seed: &mut u64) -> f64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 11) as f64) * (1.0 / 9007199254740992.0)
}

fn compare_scanline<const MODULATE: bool, const TRANSPARENT: bool>(
    width: usize,
    height: usize,
    vertices: [[f64; 8]; 3],
    scissor: bool,
    label: &str,
) -> TestedPath {
    let texture = Texture::from_gscr(
        "5x3:000000ffffff0102feff80007f01ff102030abcdef00ff00ff00ff804020010101fefefe1234566543210100ff",
    )
    .unwrap();
    let mut actual = Graphics::new_canvas(width, height);
    actual.buffer.fill(0x17293b);
    actual.owner.fill(23);
    actual.current_color = 0xa173c5;
    actual.hit = true;
    actual.hit_color = true;
    actual.hit_sprite = true;
    actual.hit_id = 23;
    actual.hit_color_rgb = 0x234567;
    actual.clear_buffer_dirty();
    if scissor {
        actual.w_left = 7;
        actual.w_right = width as i32 - 9;
        actual.w_top = 6;
        actual.w_bottom = height as i32 - 6;
    }
    let mut expected = actual.clone();
    let mut actual_depth: Vec<f64> = (0..width * height)
        .map(|index| match index % 29 {
            0 => 0.75,
            1 => 3.0,
            2 => f64::from_bits(0x7ff8000000000023),
            3 => f64::INFINITY,
            4 => f64::NEG_INFINITY,
            5 => -0.0,
            _ => 0.0,
        })
        .collect();
    let mut expected_depth = actual_depth.clone();
    let mut tested_path = TestedPath::Empty;
    if let Some(raster) = actual
        .prepare_perspective_texture::<MODULATE>(vertices)
        .unwrap()
    {
        let bounded_values = Graphics::perspective_values_are_bounded::<MODULATE>(&raster);
        actual.raster_perspective_texture::<MODULATE, TRANSPARENT>(
            &texture,
            raster,
            &mut actual_depth,
            height,
            0x000000,
        );
        expected.raster_perspective_texture_original_oracle::<MODULATE, TRANSPARENT>(
            &texture,
            raster,
            &mut expected_depth,
            height,
            0x000000,
        );
        // Count a path only if at least one pixel was really written. A
        // prepared bounding box alone can still have no covered pixel.
        if actual.buffer_dirty {
            tested_path = if bounded_values {
                TestedPath::Bounded
            } else {
                TestedPath::Checked
            };
        }
    }
    assert_eq!(actual.buffer, expected.buffer, "{label}: framebuffer");
    if let Some(index) = actual_depth
        .iter()
        .zip(&expected_depth)
        .position(|(a, b)| a.to_bits() != b.to_bits())
    {
        panic!(
            "{label}: depth bits differ at ({}, {})",
            index / height,
            index % height
        );
    }
    assert_eq!(actual.buffer_dirty, expected.buffer_dirty, "{label}: dirty");
    assert_eq!(actual.owner, expected.owner, "{label}: ownership");
    assert_eq!(actual.current_color, expected.current_color, "{label}: INK");
    assert_eq!(
        (
            actual.hit,
            actual.hit_color,
            actual.hit_sprite,
            actual.hit_id,
            actual.hit_color_rgb
        ),
        (
            expected.hit,
            expected.hit_color,
            expected.hit_sprite,
            expected.hit_id,
            expected.hit_color_rgb
        ),
        "{label}: collision state",
    );
    tested_path
}

#[test]
fn bounded_finite_matches_original_in_color_and_depth_bits() {
    let mut seed = 0xc53a1e2b77d41905_u64;
    // Index 0/1 selects full/scissored viewport; path is Empty/Checked/Bounded.
    let mut coverage = [[0_usize; 3]; 2];
    for (width, height) in [(640, 360), (960, 540), (1280, 720)] {
        for case in 0..12 {
            let mut vertices = std::array::from_fn(|_| {
                [
                    unit(&mut seed) * (width as f64 + 120.0) - 60.0 + 0.13,
                    unit(&mut seed) * (height as f64 + 100.0) - 50.0 + 0.37,
                    0.08 + unit(&mut seed) * 4.0,
                    unit(&mut seed) * 384.0 - 192.0,
                    unit(&mut seed) * 384.0 - 192.0,
                    unit(&mut seed) * 400.0 - 60.0,
                    unit(&mut seed) * 400.0 - 60.0,
                    unit(&mut seed) * 400.0 - 60.0,
                ]
            });
            if case == 0 {
                // Cross several tile boundaries with exactly horizontal and
                // vertical edges; reverse winding in the next explicit case.
                vertices[0][0] = 15.5;
                vertices[0][1] = 15.5;
                vertices[1][0] = 95.5;
                vertices[1][1] = 15.5;
                vertices[2][0] = 15.5;
                vertices[2][1] = 95.5;
            } else if case == 1 || case == 9 {
                // Also cover a narrow triangle and an incomplete row range.
                vertices[0][0] = 33.1;
                vertices[0][1] = 10.2;
                vertices[1][0] = 35.7;
                vertices[1][1] = 91.9;
                vertices[2][0] = 37.2;
                vertices[2][1] = 11.3;
            }
            if case % 2 != 0 {
                vertices.swap(0, 2);
            }
            let label = format!("{width}x{height} case {case}");
            let scissor = case % 3 == 0;
            let paths = [
                compare_scanline::<true, false>(width, height, vertices, scissor, &label),
                compare_scanline::<true, true>(width, height, vertices, scissor, &label),
                // Unmodulated instantiations preserve the same six-value
                // initialization, while advancing only the first three fields.
                compare_scanline::<false, false>(width, height, vertices, scissor, &label),
                compare_scanline::<false, true>(width, height, vertices, scissor, &label),
            ];
            for path in paths {
                coverage[usize::from(scissor)][path as usize] += 1;
            }
        }
    }
    assert_eq!(coverage.iter().flatten().sum::<usize>(), 144);
    for (scissor, counts) in coverage.iter().enumerate() {
        assert!(
            counts[TestedPath::Bounded as usize] >= 4,
            "viewport scissor={scissor} did not exercise nonempty bounded rendering"
        );
        assert_eq!(
            counts[TestedPath::Checked as usize],
            0,
            "ordinary randomized inputs should have bounded values"
        );
    }
    eprintln!(
        "finite oracle nonempty coverage [empty, checked, bounded]: full={:?}, scissor={:?}",
        coverage[0], coverage[1]
    );
}

fn example_raster() -> PerspectiveTextureRaster {
    PerspectiveTextureRaster {
        left: 2,
        right: 29,
        bottom: 2,
        top: 21,
        weight_dx: [0.0; 3],
        weight_dy: [0.0; 3],
        row_weights: [1.0; 3],
        attribute_dx: [0.0; 6],
        attribute_dy: [0.0; 6],
        row_attributes: [1.25, 0.25, 0.75, 255.0, 128.0, 64.0],
    }
}

#[test]
fn finite_guard_is_conservative_at_numeric_and_extent_limits() {
    let ordinary = example_raster();
    assert!(Graphics::perspective_values_are_bounded::<true>(&ordinary));
    assert!(Graphics::perspective_values_are_bounded::<false>(&ordinary));
    for which in 0..3 {
        for value in [1e100, -1e100, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut raster = ordinary;
            match which {
                0 => raster.row_attributes[1] = value,
                1 => raster.attribute_dx[1] = value,
                _ => raster.attribute_dy[1] = value,
            }
            assert!(!Graphics::perspective_values_are_bounded::<true>(&raster));
            assert!(!Graphics::perspective_values_are_bounded::<false>(&raster));
        }
    }
    let mut raster = ordinary;
    raster.attribute_dy[4] = f64::INFINITY;
    assert!(!Graphics::perspective_values_are_bounded::<true>(&raster));
    assert!(Graphics::perspective_values_are_bounded::<false>(&raster));
    let mut raster = ordinary;
    raster.right = raster.left + (1 << 20) - 1;
    assert!(Graphics::perspective_values_are_bounded::<true>(&raster));
    raster.right += 1;
    assert!(!Graphics::perspective_values_are_bounded::<true>(&raster));
    let mut raster = ordinary;
    raster.top = raster.bottom + (1 << 20) - 1;
    assert!(Graphics::perspective_values_are_bounded::<true>(&raster));
    raster.top += 1;
    assert!(!Graphics::perspective_values_are_bounded::<true>(&raster));
}

fn compare_extreme<const MODULATE: bool, const TRANSPARENT: bool>(
    raster: PerspectiveTextureRaster,
    label: &str,
) -> bool {
    let texture = Texture::from_gscr(
        "5x3:000000ffffff0102feff80007f01ff102030abcdef00ff00ff00ff804020010101fefefe1234566543210100ff",
    ).unwrap();
    let mut actual = Graphics::new_canvas(48, 32);
    actual.buffer.fill(0x17293b);
    actual.clear_buffer_dirty();
    let mut expected = actual.clone();
    // Negative old depth deliberately admits very small positive q, so the
    // extreme reciprocal checks really execute instead of failing depth first.
    let mut actual_depth = vec![-1.0; 48 * 32];
    let mut expected_depth = actual_depth.clone();
    actual.raster_perspective_texture::<MODULATE, TRANSPARENT>(
        &texture,
        raster,
        &mut actual_depth,
        32,
        0,
    );
    expected.raster_perspective_texture_original_oracle::<MODULATE, TRANSPARENT>(
        &texture,
        raster,
        &mut expected_depth,
        32,
        0,
    );
    assert_eq!(actual.buffer, expected.buffer, "{label}: color");
    assert!(
        actual_depth
            .iter()
            .zip(&expected_depth)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "{label}: depth bits"
    );
    assert_eq!(actual.buffer_dirty, expected.buffer_dirty, "{label}: dirty");
    actual.buffer_dirty
}

#[test]
fn finite_specializations_match_original_for_extreme_inputs() {
    let normal = example_raster();
    let mut cases = vec![("normal", normal)];
    for (label, field, value) in [
        ("large-row", 0, 1e300),
        ("large-dx", 1, 1e308),
        ("large-dy", 2, 1e308),
        ("infinite-row", 0, f64::INFINITY),
        ("nan-dx", 1, f64::NAN),
    ] {
        let mut raster = normal;
        match field {
            0 => raster.row_attributes[1] = value,
            1 => raster.attribute_dx[1] = value,
            _ => raster.attribute_dy[2] = value,
        }
        cases.push((label, raster));
    }
    let mut tiny_finite = normal;
    tiny_finite.row_attributes = [1e-200, 1.5e-200, 2.5e-200, 255e-200, 255e-200, 255e-200];
    cases.push(("large-finite-reciprocal", tiny_finite));
    let mut overflow_uv = tiny_finite;
    overflow_uv.row_attributes[0] = 1e-220;
    overflow_uv.row_attributes[1] = 1e99;
    cases.push(("uv-product-overflow", overflow_uv));
    let mut infinite_inverse = tiny_finite;
    infinite_inverse.row_attributes[0] = 1e-310;
    cases.push(("infinite-reciprocal", infinite_inverse));
    let mut overflow_rgb = tiny_finite;
    overflow_rgb.row_attributes[3] = 1e300;
    cases.push(("rgb-product-overflow", overflow_rgb));
    let mut extreme_q = normal;
    extreme_q.row_attributes[0] = f64::MAX;
    cases.push(("large-q", extreme_q));
    for (label, raster) in cases {
        let writes = [
            compare_extreme::<true, false>(raster, label),
            compare_extreme::<true, true>(raster, label),
            compare_extreme::<false, false>(raster, label),
            compare_extreme::<false, true>(raster, label),
        ];
        if label == "large-finite-reciprocal" {
            assert!(
                writes.iter().all(|written| *written),
                "expected finite UVs were not rendered"
            );
        } else if label == "uv-product-overflow" || label == "infinite-reciprocal" {
            assert!(
                writes.iter().all(|written| !*written),
                "nonfinite UVs must be rejected"
            );
        }
    }
}

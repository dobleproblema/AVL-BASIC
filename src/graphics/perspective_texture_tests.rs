use super::*;

fn varied_texture() -> Texture {
    // Non-power-of-two dimensions, fractional UVs and all channel ranges make
    // wrapping and a one-unit change in colour visible in the comparisons.
    Texture::from_gscr(
        "5x3:000000ffffff0102feff80007f01ff102030abcdef00ff00ff00ff804020010101fefefe1234566543210100ff",
    )
    .unwrap()
}

fn clean_graphics() -> Graphics {
    let mut graphics = Graphics::new(640);
    graphics.buffer.fill(0x17293b);
    graphics.owner.fill(23);
    graphics.current_color = 0xa173c5;
    graphics.hit = true;
    graphics.hit_color = true;
    graphics.hit_sprite = true;
    graphics.hit_id = 23;
    graphics.hit_color_rgb = 0x234567;
    graphics.clear_buffer_dirty();
    graphics
}

fn assert_same_render(
    actual: &Graphics,
    expected: &Graphics,
    actual_depth: &[f64],
    expected_depth: &[f64],
    label: &str,
) {
    if let Some(index) = actual
        .buffer()
        .iter()
        .zip(expected.buffer())
        .position(|(actual, expected)| actual != expected)
    {
        panic!(
            "{label}: framebuffer differs at ({}, {}): {:06x} versus {:06x}",
            index % actual.width,
            actual.height - 1 - index / actual.width,
            actual.buffer()[index],
            expected.buffer()[index]
        );
    }
    if let Some(index) = actual_depth
        .iter()
        .zip(expected_depth)
        .position(|(actual, expected)| actual.to_bits() != expected.to_bits())
    {
        panic!(
            "{label}: depth differs at ({}, {}): {:?} versus {:?}",
            index / actual.height,
            index % actual.height,
            actual_depth[index],
            expected_depth[index]
        );
    }
    assert_eq!(actual.buffer_dirty(), expected.buffer_dirty(), "{label}");
    assert_eq!(actual.owner, expected.owner, "{label}: sprite ownership");
    assert_eq!(actual.current_color, expected.current_color, "{label}: INK");
    assert_eq!(
        (
            actual.hit,
            actual.hit_color,
            actual.hit_sprite,
            actual.hit_id,
            actual.hit_color_rgb,
        ),
        (
            expected.hit,
            expected.hit_color,
            expected.hit_sprite,
            expected.hit_id,
            expected.hit_color_rgb,
        ),
        "{label}: collision state"
    );
}

// Compare the public entry point with the retained RGB raster forced on every
// triangle. Supplying canvas vertices separately also exercises public ORIGIN
// and SCALE handling without using the optimized dispatcher as the oracle.
fn compare_polygon<const V: usize, const T: usize>(
    initial: &Graphics,
    texture: &Texture,
    initial_depth: &[f64],
    vertices: [[f64; 8]; V],
    canvas_vertices: [[f64; 8]; V],
    triangles: [[usize; 3]; T],
    label: &str,
) -> (Graphics, Vec<f64>) {
    let mut actual = initial.clone();
    let mut expected = initial.clone();
    let mut actual_depth = initial_depth.to_vec();
    let mut expected_depth = initial_depth.to_vec();
    let rasters: Vec<_> = triangles
        .into_iter()
        .map(|indices| {
            expected
                .prepare_perspective_texture::<true>(indices.map(|index| canvas_vertices[index]))
                .unwrap()
        })
        .collect();
    if canvas_vertices.iter().all(|vertex| vertex[2] > 0.0) {
        for raster in rasters.into_iter().flatten() {
            let height = expected.height;
            expected.raster_perspective_texture::<true, false>(
                texture,
                raster,
                &mut expected_depth,
                height,
                0,
            );
        }
    }
    let (width, height) = (actual.width, actual.height);
    if V == 3 {
        actual
            .perspective_textured_triangle(
                texture,
                [vertices[0], vertices[1], vertices[2]],
                &mut actual_depth,
                width,
                height,
            )
            .unwrap();
    } else {
        assert_eq!(V, 4);
        actual
            .perspective_textured_quad(
                texture,
                [vertices[0], vertices[1], vertices[2], vertices[3]],
                &mut actual_depth,
                width,
                height,
            )
            .unwrap();
    }
    assert_same_render(&actual, &expected, &actual_depth, &expected_depth, label);
    assert_eq!(actual.xpos(), vertices[V - 1][0], "{label}: cursor x");
    assert_eq!(actual.ypos(), vertices[V - 1][1], "{label}: cursor y");
    (actual, actual_depth)
}

fn draw_unmodulated_polygon<const V: usize>(
    initial: &Graphics,
    texture: &Texture,
    vertices: [[f64; 8]; V],
) -> (Graphics, Vec<f64>) {
    let mut rendered = initial.clone();
    let (width, height) = (rendered.width, rendered.height);
    let mut depth = vec![0.0; width * height];
    if V == 3 {
        rendered
            .perspective_textured_triangle(
                texture,
                [vertices[0], vertices[1], vertices[2]],
                &mut depth,
                width,
                height,
            )
            .unwrap();
    } else {
        assert_eq!(V, 4);
        rendered
            .perspective_textured_quad(
                texture,
                [vertices[0], vertices[1], vertices[2], vertices[3]],
                &mut depth,
                width,
                height,
            )
            .unwrap();
    }
    (rendered, depth)
}

fn assert_solid_texels(
    initial: &Graphics,
    rendered: &Graphics,
    depth: &[f64],
    texel: u32,
    label: &str,
) -> usize {
    let mut written = 0;
    for x in 0..rendered.width {
        for y in 0..rendered.height {
            let q = depth[x * rendered.height + y];
            assert!(q.is_finite() && q >= 0.0, "{label}: depth at ({x}, {y})");
            let index = (rendered.height - 1 - y) * rendered.width + x;
            let expected = if q > 0.0 {
                written += 1;
                texel
            } else {
                initial.buffer[index]
            };
            assert_eq!(
                rendered.buffer[index], expected,
                "{label}: texture color at ({x}, {y})"
            );
        }
    }
    written
}

#[test]
fn white_texture_matches_rgb_reference_with_wrapping_windings_and_occlusion() {
    let texture = varied_texture();
    let initial = clean_graphics();
    let mut depth = vec![0.0; initial.width * initial.height];
    for x in 0..initial.width {
        for y in 0..initial.height {
            // Alternate untouched, occluded and equal-depth pixels, including
            // the public depth tolerance around a repeated draw.
            depth[x * initial.height + y] = match (x + y) % 4 {
                0 => 0.5,
                1 => 1.5,
                2 => 0.5 + 1e-10,
                _ => 0.0,
            };
        }
    }
    let triangle = [
        [12.25, 16.125, 0.125, -17.875, 11.375, 255.0, 255.0, 255.0],
        [271.75, 28.625, 1.25, 30.125, -21.625, 255.0, 255.0, 255.0],
        [38.125, 203.75, 0.5, -0.375, 42.875, 255.0, 255.0, 255.0],
    ];
    for indices in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let vertices = indices.map(|index| triangle[index]);
        let label = format!("winding {indices:?}");
        let (rendered, rendered_depth) = compare_polygon(
            &initial,
            &texture,
            &depth,
            vertices,
            vertices,
            [[0, 1, 2]],
            &label,
        );
        assert!(rendered.buffer_dirty());
        assert_ne!(rendered.buffer(), initial.buffer());
        compare_polygon(
            &rendered,
            &texture,
            &rendered_depth,
            vertices,
            vertices,
            [[0, 1, 2]],
            &format!("{label}, repeated draw"),
        );
    }
}

#[test]
fn white_texture_matches_rgb_reference_under_origin_scale_and_viewport_clipping() {
    let texture = varied_texture();
    let mut initial = clean_graphics();
    initial
        .set_origin(31, -17, Some((100, 340, 310, 70)))
        .unwrap();
    let depth = vec![0.0; initial.width * initial.height];
    let vertices = [
        [10.25, 40.5, 0.2, -3.125, 4.875, 255.0, 255.0, 255.0],
        [390.75, 50.25, 1.7, 51.375, -9.125, 255.0, 255.0, 255.0],
        [130.5, 390.75, 0.6, -11.875, 22.375, 255.0, 255.0, 255.0],
    ];
    let canvas_vertices = vertices.map(|mut vertex| {
        vertex[0] += 31.0;
        vertex[1] -= 17.0;
        vertex
    });
    compare_polygon(
        &initial,
        &texture,
        &depth,
        vertices,
        canvas_vertices,
        [[0, 1, 2]],
        "ORIGIN and clipping",
    );

    initial.set_scale(Some((-2.0, 2.0, -2.0, 2.0, 0))).unwrap();
    let vertices = [
        [-3.25, -1.125, 0.8, -9.75, 3.125, 255.0, 255.0, 255.0],
        [3.125, -0.75, 0.35, 24.375, -18.875, 255.0, 255.0, 255.0],
        [-0.25, 3.5, 1.4, -0.125, 21.375, 255.0, 255.0, 255.0],
    ];
    // The 240-by-240 viewport maps [-2,2] to [100,340] x [70,310].
    let canvas_vertices = vertices.map(|mut vertex| {
        vertex[0] = 100.0 + (vertex[0] + 2.0) * 60.0;
        vertex[1] = 70.0 + (vertex[1] + 2.0) * 60.0;
        vertex
    });
    let (rendered, rendered_depth) = compare_polygon(
        &initial,
        &texture,
        &depth,
        vertices,
        canvas_vertices,
        [[0, 1, 2]],
        "SCALE and clipping",
    );
    assert!(rendered.buffer_dirty());
    for x in 0..rendered.width {
        for y in 0..rendered.height {
            if !(100..=340).contains(&x) || !(70..=310).contains(&y) {
                assert_eq!(rendered_depth[x * rendered.height + y], 0.0);
            }
        }
    }
}

#[test]
fn quad_can_mix_an_unmodulated_triangle_and_a_lit_triangle_without_a_seam() {
    let texture = varied_texture();
    let initial = clean_graphics();
    let depth = vec![0.0; initial.width * initial.height];
    let mut vertices = [
        [15.25, 17.125, 0.4, -4.875, 3.375, 255.0, 255.0, 255.0],
        [235.75, 27.625, 0.8, 31.125, -9.625, 255.0, 255.0, 255.0],
        [217.125, 217.75, 1.3, -0.375, 16.875, 255.0, 255.0, 255.0],
        [28.875, 197.25, 0.7, -9.625, 22.125, 48.0, 128.0, 0.0],
    ];
    for label in ["one white half", "nearly white vertex", "out-of-range RGB"] {
        compare_polygon(
            &initial,
            &texture,
            &depth,
            vertices,
            vertices,
            [[0, 1, 2], [0, 2, 3]],
            label,
        );
        if label == "one white half" {
            vertices[1][5] = 254.0;
        } else {
            vertices[1][5..].copy_from_slice(&[256.0, -12.0, 127.5]);
        }
    }
}

#[test]
fn unmodulated_texture_preserves_its_texels_for_extreme_depths_and_geometry() {
    // A single nonwhite texel makes the expected color independent of UV
    // rounding and verifies that every color channel is copied unchanged.
    let texture = Texture::from_gscr("1x1:91be73").unwrap();
    let initial = clean_graphics();
    let ordinary = [
        [11.125, 13.375, 1.0, -3.875, 4.125, 255.0, 255.0, 255.0],
        [291.625, 21.875, 1.0, 31.375, -17.125, 255.0, 255.0, 255.0],
        [47.875, 193.125, 1.0, -11.625, 27.875, 255.0, 255.0, 255.0],
    ];
    for inverse_depths in [
        [f64::from_bits(1); 3],
        [1e-320, 2e-320, 7e-320],
        [1e-308, 2e-308, 7e-308],
        [1e-200, 2e-200, 7e-200],
        [1e-12, 1e6, 0.125],
        [1e-100, 1e100, 0.5],
        [1e305, 2e305, 0.25e305],
    ] {
        let mut vertices = ordinary;
        for (vertex, q) in vertices.iter_mut().zip(inverse_depths) {
            vertex[2] = q;
        }
        let (rendered, depth) = draw_unmodulated_polygon(&initial, &texture, vertices);
        let written = assert_solid_texels(
            &initial,
            &rendered,
            &depth,
            0x91be73,
            &format!("q={inverse_depths:?}"),
        );
        // The existing depth tolerance rejects extremely tiny q values; the
        // remaining ranges must still produce visible texture pixels.
        if inverse_depths.iter().copied().fold(0.0, f64::max) >= 1e-12 {
            assert!(written > 0, "q={inverse_depths:?}: no visible texels");
        }
    }
    for positions in [
        [[0.0, 100.5], [630.0, 100.5], [315.0, 100.5 + 1e-10]],
        [[-1e8, -1e8], [1e8, -1e8], [0.0, 1e8]],
        [[-1e100, -1e100], [1e100, -1e100], [0.0, 1e100]],
        [[-51.0, -37.0], [-1.0, -19.0], [-18.0, -0.25]],
        [[10.25, 20.75], [10.25, 20.75], [10.25, 20.75]],
    ] {
        let mut vertices = ordinary;
        for (vertex, position) in vertices.iter_mut().zip(positions) {
            vertex[0..2].copy_from_slice(&position);
        }
        let (rendered, depth) = draw_unmodulated_polygon(&initial, &texture, vertices);
        assert_solid_texels(
            &initial,
            &rendered,
            &depth,
            0x91be73,
            &format!("positions={positions:?}"),
        );
    }
}

#[test]
fn extreme_depth_cancellation_does_not_darken_an_unmodulated_texture() {
    let texture = Texture::from_gscr("1x1:ffffff").unwrap();
    let initial = clean_graphics();
    let vertices = [
        [-0.5, 0.5, 1e-11, 0.0, 0.0, 255.0, 255.0, 255.0],
        [31.5, 0.5, 1e17, 0.0, 0.0, 255.0, 255.0, 255.0],
        [0.5, 31.5, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0],
    ];
    let (rendered, depth) = draw_unmodulated_polygon(&initial, &texture, vertices);
    assert_solid_texels(
        &initial,
        &rendered,
        &depth,
        0xffffff,
        "catastrophic cancellation of RGB*q versus q",
    );
    // The full RGB raster used to darken this pixel to 0x191919 because RGB*q
    // and q lost different low bits. No illumination means no such modulation.
    assert!(depth[31] > 0.0);
    assert_eq!(
        rendered.buffer[(rendered.height - 1 - 31) * rendered.width],
        0xffffff
    );
}

#[test]
fn both_quad_halves_preserve_unmodulated_texels_at_extreme_depths() {
    let texture = Texture::from_gscr("1x1:91be73").unwrap();
    let initial = clean_graphics();
    let vertices = [
        [63.5, 0.5, 1e-11, 0.0, 0.0, 255.0, 255.0, 255.0],
        [95.5, 0.5, 1e17, 0.0, 0.0, 255.0, 255.0, 255.0],
        [64.5, 31.5, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0],
        [32.5, 1.5, 1e17, 0.0, 0.0, 255.0, 255.0, 255.0],
    ];
    let (rendered, depth) = draw_unmodulated_polygon(&initial, &texture, vertices);
    assert!(assert_solid_texels(&initial, &rendered, &depth, 0x91be73, "quad") > 0);
    // Samples lie on opposite sides of the shared 0-2 diagonal, so neither
    // triangle can accidentally be omitted while preserving these results.
    for (x, y) in [(80, 5), (48, 5), (64, 31)] {
        assert!(depth[x * rendered.height + y] > 0.0, "quad at ({x}, {y})");
        assert_eq!(
            rendered.buffer[(rendered.height - 1 - y) * rendered.width + x],
            0x91be73
        );
    }
}

#[test]
fn unmodulated_texture_does_not_compute_unused_rgb_products() {
    let texture = Texture::from_gscr("1x1:91be73").unwrap();
    let initial = clean_graphics();
    // q and q*UV are finite, while multiplying this q by 255 would overflow.
    // Omitting illumination must not reject otherwise valid texture inputs.
    let vertices = [
        [0.5, 0.5, 1e307, 0.0, 0.0, 255.0, 255.0, 255.0],
        [32.5, 0.5, 1e307, 0.0, 0.0, 255.0, 255.0, 255.0],
        [32.5, 32.5, 1e307, 0.0, 0.0, 255.0, 255.0, 255.0],
        [0.5, 32.5, 1e307, 0.0, 0.0, 255.0, 255.0, 255.0],
    ];
    let (triangle, triangle_depth) =
        draw_unmodulated_polygon(&initial, &texture, [vertices[0], vertices[1], vertices[2]]);
    assert!(
        assert_solid_texels(
            &initial,
            &triangle,
            &triangle_depth,
            0x91be73,
            "huge q triangle"
        ) > 0
    );
    let (quad, quad_depth) = draw_unmodulated_polygon(&initial, &texture, vertices);
    assert!(assert_solid_texels(&initial, &quad, &quad_depth, 0x91be73, "huge q quad") > 0);
}

#[test]
fn nearly_white_and_colored_lighting_still_modulate_the_texture() {
    let texture = Texture::from_gscr("1x1:ffffff").unwrap();
    let initial = clean_graphics();
    let depth = vec![0.0; initial.width * initial.height];
    let ordinary = [
        [0.5, 0.5, 0.25, 0.0, 0.0, 255.0, 255.0, 255.0],
        [32.5, 0.5, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0],
        [0.5, 32.5, 0.5, 0.0, 0.0, 255.0, 255.0, 255.0],
    ];
    for (light, expected) in [
        ([254.0, 255.0, 255.0], 0xfeffff),
        ([255.0, 254.0, 255.0], 0xfffeff),
        ([255.0, 255.0, 254.0], 0xfffffe),
        ([128.0, 64.0, 0.0], 0x804000),
    ] {
        let vertices = ordinary.map(|mut vertex| {
            vertex[5..].copy_from_slice(&light);
            vertex
        });
        let label = format!("light={light:?}");
        let (rendered, rendered_depth) = compare_polygon(
            &initial,
            &texture,
            &depth,
            vertices,
            vertices,
            [[0, 1, 2]],
            &label,
        );
        assert!(assert_solid_texels(&initial, &rendered, &rendered_depth, expected, &label) > 0);
    }
}

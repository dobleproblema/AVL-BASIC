use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::process::Command;

#[derive(Debug)]
struct FontSource {
    width: usize,
    height: usize,
    glyphs: BTreeMap<u32, Vec<u32>>,
}

fn main() {
    println!("cargo:rerun-if-changed=windows.rc");
    println!("cargo:rerun-if-changed=assets/avl-basic.ico");
    println!("cargo:rerun-if-changed=assets/fonts/avl-basic-fonts.txt");
    println!("cargo:rerun-if-changed=samples");

    generate_font_tables();
    link_samples_into_target_profile();

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("windows.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}

fn link_samples_into_target_profile() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| ".".into()));
    let samples = root.join("samples");
    let Some(profile_dir) = target_profile_dir() else {
        return;
    };
    let link = profile_dir.join("samples");

    if !samples.is_dir() || fs::symlink_metadata(&link).is_ok() {
        return;
    }
    if let Err(err) = create_dir_link(&samples, &link) {
        println!(
            "cargo:warning=failed to link {} to {}: {}",
            link.display(),
            samples.display(),
            err
        );
    }
}

fn target_profile_dir() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env::var_os("OUT_DIR")?);
    loop {
        if dir.file_name().and_then(|name| name.to_str()) == Some("build") {
            return dir.parent().map(Path::to_path_buf);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(windows)]
fn create_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("mklink /J exited with {status}"),
        ))
    }
}

#[cfg(unix)]
fn create_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(any(windows, unix)))]
fn create_dir_link(_target: &Path, _link: &Path) -> io::Result<()> {
    Ok(())
}

fn generate_font_tables() {
    let source_path = Path::new("assets/fonts/avl-basic-fonts.txt");
    let source = fs::read_to_string(source_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
    let fonts = parse_fonts(&source);
    validate_fonts(&fonts);

    let generated = render_font_tables(&fonts);
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
    fs::write(out_dir.join("font_tables.rs"), generated).expect("failed to write font_tables.rs");
}

fn parse_fonts(source: &str) -> BTreeMap<String, FontSource> {
    let mut fonts: BTreeMap<String, FontSource> = BTreeMap::new();
    let mut current_font: Option<String> = None;
    let mut pending: Option<PendingGlyph> = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            if let Some(glyph) = pending.as_ref() {
                panic!(
                    "incomplete glyph U+{:04X} before line {}: expected {} rows, found {}",
                    glyph.codepoint,
                    line_number,
                    font(&fonts, &glyph.font).height,
                    glyph.rows.len()
                );
            }
            continue;
        }

        if trimmed.starts_with("font ") {
            if let Some(glyph) = pending.take() {
                panic!(
                    "incomplete glyph U+{:04X} before line {}: expected {} rows, found {}",
                    glyph.codepoint,
                    line_number,
                    font(&fonts, &glyph.font).height,
                    glyph.rows.len()
                );
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 4 {
                panic!(
                    "line {}: expected `font <name> <width> <height>`",
                    line_number
                );
            }
            let name = parts[1].to_string();
            let width = parse_usize(parts[2], line_number, "width");
            let height = parse_usize(parts[3], line_number, "height");
            if fonts
                .insert(
                    name.clone(),
                    FontSource {
                        width,
                        height,
                        glyphs: BTreeMap::new(),
                    },
                )
                .is_some()
            {
                panic!("line {}: duplicate font `{}`", line_number, name);
            }
            current_font = Some(name);
            continue;
        }

        if trimmed.starts_with("glyph ") {
            if let Some(glyph) = pending.take() {
                panic!(
                    "incomplete glyph U+{:04X} before line {}: expected {} rows, found {}",
                    glyph.codepoint,
                    line_number,
                    font(&fonts, &glyph.font).height,
                    glyph.rows.len()
                );
            }
            let font_name = current_font
                .clone()
                .unwrap_or_else(|| panic!("line {}: glyph before font section", line_number));
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 2 {
                panic!("line {}: expected `glyph U+XXXX`", line_number);
            }
            let codepoint = parse_codepoint(parts[1], line_number);
            pending = Some(PendingGlyph {
                font: font_name,
                codepoint,
                rows: Vec::new(),
            });
            continue;
        }

        let glyph = pending
            .as_mut()
            .unwrap_or_else(|| panic!("line {}: row outside glyph", line_number));
        let font_info = font(&fonts, &glyph.font);
        if trimmed.len() != font_info.width {
            panic!(
                "line {}: glyph U+{:04X} row width is {}, expected {}",
                line_number,
                glyph.codepoint,
                trimmed.len(),
                font_info.width
            );
        }
        let row_value = row_to_bits(trimmed, line_number);
        glyph.rows.push(row_value);
        if glyph.rows.len() == font_info.height {
            let glyph = pending.take().unwrap();
            let target = fonts.get_mut(&glyph.font).unwrap();
            if target.glyphs.insert(glyph.codepoint, glyph.rows).is_some() {
                panic!(
                    "line {}: duplicate glyph U+{:04X} in font `{}`",
                    line_number, glyph.codepoint, glyph.font
                );
            }
        }
    }

    if let Some(glyph) = pending {
        panic!(
            "incomplete glyph U+{:04X} at end of file: expected {} rows, found {}",
            glyph.codepoint,
            font(&fonts, &glyph.font).height,
            glyph.rows.len()
        );
    }

    fonts
}

#[derive(Debug)]
struct PendingGlyph {
    font: String,
    codepoint: u32,
    rows: Vec<u32>,
}

fn font<'a>(fonts: &'a BTreeMap<String, FontSource>, name: &str) -> &'a FontSource {
    fonts
        .get(name)
        .unwrap_or_else(|| panic!("unknown font `{}`", name))
}

fn parse_usize(text: &str, line_number: usize, field: &str) -> usize {
    text.parse()
        .unwrap_or_else(|_| panic!("line {}: invalid {} `{}`", line_number, field, text))
}

fn parse_codepoint(text: &str, line_number: usize) -> u32 {
    let Some(hex) = text.strip_prefix("U+") else {
        panic!("line {}: glyph codepoint must use U+XXXX", line_number);
    };
    let codepoint = u32::from_str_radix(hex, 16)
        .unwrap_or_else(|_| panic!("line {}: invalid codepoint `{}`", line_number, text));
    if char::from_u32(codepoint).is_none() {
        panic!("line {}: invalid Unicode scalar `{}`", line_number, text);
    }
    codepoint
}

fn row_to_bits(row: &str, line_number: usize) -> u32 {
    let mut value = 0u32;
    for ch in row.chars() {
        value <<= 1;
        match ch {
            '.' => {}
            '1' => value |= 1,
            _ => panic!(
                "line {}: rows may contain only `.` and `1`, found `{}`",
                line_number, ch
            ),
        }
    }
    value
}

fn validate_fonts(fonts: &BTreeMap<String, FontSource>) {
    let small = fonts.get("small").expect("missing `small` font");
    let big = fonts.get("big").expect("missing `big` font");
    assert_dimensions("small", small, 8, 16);
    assert_dimensions("big", big, 16, 16);

    let small_glyphs: BTreeSet<u32> = small.glyphs.keys().copied().collect();
    let big_glyphs: BTreeSet<u32> = big.glyphs.keys().copied().collect();
    if small_glyphs != big_glyphs {
        panic!("small and big fonts must define the same glyph set");
    }
    if !small_glyphs.contains(&0x25A1) {
        panic!("replacement glyph U+25A1 is required");
    }
}

fn assert_dimensions(name: &str, font: &FontSource, width: usize, height: usize) {
    if font.width != width || font.height != height {
        panic!(
            "`{}` font must be {}x{}, found {}x{}",
            name, width, height, font.width, font.height
        );
    }
}

fn render_font_tables(fonts: &BTreeMap<String, FontSource>) -> String {
    let mut out = String::new();
    out.push_str("// @generated by build.rs from assets/fonts/avl-basic-fonts.txt.\n");
    out.push_str("// Do not edit by hand; edit the source font file instead.\n\n");
    render_font_constants(&mut out, "SMALL", font(fonts, "small"));
    render_font_constants(&mut out, "BIG", font(fonts, "big"));
    render_glyph_count(&mut out, fonts);
    render_glyph_rows(&mut out, fonts);
    out
}

fn render_font_constants(out: &mut String, prefix: &str, font: &FontSource) {
    for (codepoint, rows) in &font.glyphs {
        out.push_str(&format!(
            "const {}_{:04X}: [u32; {}] = [{}];\n",
            prefix,
            codepoint,
            font.height,
            rows.iter()
                .map(u32::to_string)
                .collect::<Vec<String>>()
                .join(", ")
        ));
    }
    out.push('\n');
}

fn render_glyph_rows(out: &mut String, fonts: &BTreeMap<String, FontSource>) {
    out.push_str("pub fn glyph_rows(font: FontKind, ch: char) -> Option<&'static [u32]> {\n");
    out.push_str("    match font {\n");
    render_font_match(out, "Small", "SMALL", font(fonts, "small"));
    render_font_match(out, "Big", "BIG", font(fonts, "big"));
    out.push_str("    }\n");
    out.push_str("}\n");
}

fn render_glyph_count(out: &mut String, fonts: &BTreeMap<String, FontSource>) {
    out.push_str("pub fn glyph_count(font: FontKind) -> usize {\n");
    out.push_str("    match font {\n");
    out.push_str(&format!(
        "        FontKind::Small => {},\n",
        font(fonts, "small").glyphs.len()
    ));
    out.push_str(&format!(
        "        FontKind::Big => {},\n",
        font(fonts, "big").glyphs.len()
    ));
    out.push_str("    }\n");
    out.push_str("}\n\n");
}

fn render_font_match(out: &mut String, variant: &str, prefix: &str, font: &FontSource) {
    out.push_str(&format!("        FontKind::{} => match ch {{\n", variant));
    for codepoint in font.glyphs.keys() {
        out.push_str(&format!(
            "            '\\u{{{:X}}}' => Some(&{}_{:04X}),\n",
            codepoint, prefix, codepoint
        ));
    }
    out.push_str("            _ => None,\n");
    out.push_str("        },\n");
}

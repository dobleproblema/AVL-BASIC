use crate::error::{BasicError, BasicResult, ErrorCode};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

const RGBA_TAG: &str = "@RGBA";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Encoding {
    Rgb,
    Rgba,
}

impl Encoding {
    fn channels(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    fn header(self, width: usize, height: usize) -> String {
        let tag = if self == Self::Rgba { RGBA_TAG } else { "" };
        format!("{width}x{height}{tag}:")
    }
}

pub(super) struct Image<'a> {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) encoding: Encoding,
    data: &'a str,
}

fn invalid_image() -> BasicError {
    BasicError::new(ErrorCode::InvalidValue)
}

impl<'a> Image<'a> {
    pub(super) fn parse(source: &'a str) -> BasicResult<Self> {
        let (header, data) = source.split_once(':').ok_or_else(invalid_image)?;
        let (dimensions, encoding) = header
            .strip_suffix(RGBA_TAG)
            .map_or((header, Encoding::Rgb), |header| (header, Encoding::Rgba));
        let (width, height) = dimensions.split_once('x').ok_or_else(invalid_image)?;
        let width = width.parse::<usize>().map_err(|_| invalid_image())?;
        let height = height.parse::<usize>().map_err(|_| invalid_image())?;
        let sprite_bounds = 1..=i32::MAX as usize;
        if encoding == Encoding::Rgba
            && !(sprite_bounds.contains(&width) && sprite_bounds.contains(&height))
        {
            return Err(invalid_image());
        }
        let hex_len = width
            .checked_mul(height)
            .and_then(|count| count.checked_mul(encoding.channels() * 2));
        if hex_len != Some(data.len()) {
            return Err(invalid_image());
        }
        Ok(Self {
            width,
            height,
            encoding,
            data,
        })
    }

    /// Decode RGB as 0xRRGGBB and RGBA as 0xRRGGBBAA.
    pub(super) fn pixels(&self) -> BasicResult<Vec<u32>> {
        let step = self.encoding.channels() * 2;
        let valid = match self.encoding {
            Encoding::Rgb => self.data.is_ascii(),
            Encoding::Rgba => self.data.bytes().all(|byte| byte.is_ascii_hexdigit()),
        };
        if !valid {
            return Err(invalid_image());
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.data.len() / step)
            .map_err(|_| invalid_image())?;
        for start in (0..self.data.len()).step_by(step) {
            let pixel = u32::from_str_radix(&self.data[start..start + step], 16)
                .map_err(|_| invalid_image())?;
            pixels.push(pixel);
        }
        Ok(pixels)
    }

    pub(super) fn crop(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> BasicResult<String> {
        let invalid_rect = || BasicError::new(ErrorCode::InvalidArgument);
        if width == 0 || height == 0 {
            return Err(invalid_rect());
        }
        x.checked_add(width)
            .filter(|right| *right <= self.width)
            .ok_or_else(invalid_rect)?;
        let bottom = y
            .checked_add(height)
            .filter(|bottom| *bottom <= self.height)
            .ok_or_else(invalid_rect)?;
        if !self.data.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_image());
        }

        // The source length and crop bounds prove these byte offsets fit.
        // Copy each selected row directly from the encoded source.
        let step = self.encoding.channels() * 2;
        let row_len = width * step;
        let mut result = self.encoding.header(width, height);
        result
            .try_reserve_exact(row_len * height)
            .map_err(|_| invalid_image())?;
        for row in y..bottom {
            let start = (row * self.width + x) * step;
            result.push_str(&self.data[start..start + row_len]);
        }
        Ok(result)
    }
}

pub(super) fn parse_rgb(source: &str) -> BasicResult<(usize, usize, Vec<u32>)> {
    let image = Image::parse(source)?;
    if image.encoding != Encoding::Rgb {
        return Err(invalid_image());
    }
    Ok((image.width, image.height, image.pixels()?))
}

fn png_error(error: impl std::fmt::Display) -> BasicError {
    invalid_image().with_detail(error.to_string())
}

pub(super) fn load_png(path: &Path, encoding: Encoding) -> BasicResult<String> {
    let file = File::open(path).map_err(png_error)?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    if encoding == Encoding::Rgba {
        decoder.set_transformations(png::Transformations::normalize_to_color8());
    }
    let mut reader = decoder.read_info().map_err(png_error)?;
    let mut buffer = Vec::new();
    let output_size = reader.output_buffer_size();
    buffer.try_reserve_exact(output_size).map_err(png_error)?;
    buffer.resize(output_size, 0);
    let info = reader.next_frame(&mut buffer).map_err(png_error)?;
    let (grayscale, has_alpha) = match info.color_type {
        png::ColorType::Rgb => (false, false),
        png::ColorType::Rgba => (false, true),
        png::ColorType::Grayscale if encoding == Encoding::Rgba => (true, false),
        png::ColorType::GrayscaleAlpha if encoding == Encoding::Rgba => (true, true),
        _ => return Err(invalid_image()),
    };
    let pixels = buffer[..info.buffer_size()].chunks_exact(info.color_type.samples());
    let channels = encoding.channels();
    let encoded_size = pixels
        .len()
        .checked_mul(channels * 2)
        .ok_or_else(invalid_image)?;
    let mut output = encoding.header(info.width as usize, info.height as usize);
    output.try_reserve_exact(encoded_size).map_err(png_error)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for pixel in pixels {
        let [r, g, b] = if grayscale {
            [pixel[0]; 3]
        } else {
            [pixel[0], pixel[1], pixel[2]]
        };
        let alpha = if has_alpha {
            pixel[pixel.len() - 1]
        } else {
            255
        };
        for value in [r, g, b, alpha].into_iter().take(channels) {
            output.push(HEX[usize::from(value >> 4)] as char);
            output.push(HEX[usize::from(value & 15)] as char);
        }
    }
    Ok(output)
}

pub(super) fn save_png(
    path: &Path,
    width: usize,
    height: usize,
    pixels: &[u32],
    encoding: Encoding,
) -> BasicResult<()> {
    if width == 0
        || height == 0
        || width.checked_mul(height) != Some(pixels.len())
        || width > u32::MAX as usize
        || height > u32::MAX as usize
    {
        return Err(invalid_image());
    }
    let channels = encoding.channels();
    let len = pixels
        .len()
        .checked_mul(channels)
        .ok_or_else(invalid_image)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(png_error)?;
    for pixel in pixels {
        bytes.extend_from_slice(&pixel.to_be_bytes()[4 - channels..]);
    }
    let file = File::create(path).map_err(png_error)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(match encoding {
        Encoding::Rgb => png::ColorType::Rgb,
        Encoding::Rgba => png::ColorType::Rgba,
    });
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(png_error)?;
    writer.write_image_data(&bytes).map_err(png_error)
}

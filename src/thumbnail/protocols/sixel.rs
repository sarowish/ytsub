use crate::emulator::mux::{END, ESCAPE, START};
use anyhow::Result;
use image::DynamicImage;
use quantette::{
    Image, PaletteSize, Pipeline, QuantizeMethod,
    deps::palette::{encoding::Srgb, rgb::Rgb},
    dither::FloydSteinberg,
};
use std::fmt::Write;

struct QuantizedImage {
    indices: Vec<u8>,
    palette: Vec<Rgb<Srgb, u8>>,
}

pub fn display_image(image: DynamicImage) -> Result<String> {
    let width = image.width();
    let height = image.height();
    let qu = quantize(image)?;

    let mut buf = String::new();

    write!(buf, "{}P9;1;0q", *START)?;

    for (i, c) in qu.palette.iter().enumerate() {
        write!(
            buf,
            "#{};2;{};{};{}",
            i,
            u16::from(c.red) * 100 / 255,
            u16::from(c.green) * 100 / 255,
            u16::from(c.blue) * 100 / 255,
        )?;
    }

    let band_count = height.div_ceil(6);

    for band in 0..band_count {
        let y = band * 6;
        let y_max = height.min(y + 6);

        let mut used_colors = [false; 256];

        for y in y..y_max {
            for x in 0..width {
                let pixel_index = (y * width + x) as usize;
                let color_index = qu.indices[pixel_index];
                used_colors[color_index as usize] = true;
            }
        }

        for (color_index, _) in used_colors
            .iter()
            .enumerate()
            .take(qu.palette.len())
            .filter(|(_, used)| **used)
        {
            write!(buf, "#{color_index}")?;

            let mut x = 0;

            while x < width {
                let sixel = to_sixel(&qu.indices, color_index as u8, width, x, y, y_max);
                let mut repeat_count = 1;

                while x + repeat_count < width {
                    let next_sixel = to_sixel(
                        &qu.indices,
                        color_index as u8,
                        width,
                        x + repeat_count,
                        y,
                        y_max,
                    );

                    if next_sixel != sixel {
                        break;
                    }

                    repeat_count += 1;
                }

                let sixel = (sixel + 0x3f) as char;

                if repeat_count > 1 {
                    write!(buf, "!{repeat_count}{sixel}")?;
                } else {
                    write!(buf, "{sixel}")?;
                }

                x += repeat_count;
            }

            buf.push('$');
        }

        buf.push('-');
    }

    write!(buf, "{}\\{}", *ESCAPE, *END)?;

    Ok(buf)
}

fn quantize(image: DynamicImage) -> Result<QuantizedImage> {
    let image = Image::try_from(image.into_rgb8())?;

    let pipeline = Pipeline::new()
        .palette_size(PaletteSize::from_u16_clamped(256))
        .quantize_method(QuantizeMethod::Wu)
        .ditherer(FloydSteinberg::with_error_diffusion(0.85));

    let color_map = pipeline
        .input_image(image.as_ref())
        .output_srgb8_indexed_image();

    let indices = color_map.indices().to_vec();
    let palette = color_map.palette().to_vec();

    Ok(QuantizedImage { indices, palette })
}

fn to_sixel(indices: &[u8], color_index: u8, width: u32, x: u32, mut y: u32, y_max: u32) -> u8 {
    let mut bits = 0;

    for bit in 0..6 {
        if y >= y_max {
            break;
        }

        let pixel_index = (y * width + x) as usize;

        if color_index == indices[pixel_index] {
            bits |= 1 << bit;
        }

        y += 1;
    }

    bits
}

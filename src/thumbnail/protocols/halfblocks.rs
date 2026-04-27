use anyhow::Result;
use image::{Rgb, imageops::FilterType};
use ratatui::{buffer::Cell, layout::Rect, style::Color, symbols::half_block};
use std::path::Path;

pub fn display_image(path: &Path, area: Rect) -> Result<Vec<HalfBlock>> {
    let image = image::open(path)?.resize_exact(
        area.width.into(),
        u32::from(area.height) * 2,
        FilterType::Triangle,
    );

    let mut data = Vec::new();

    for (y, row) in image.to_rgb8().enumerate_rows() {
        if y.is_multiple_of(2) {
            for (_, _, pixel) in row {
                data.push(HalfBlock::new(*pixel));
            }
        } else {
            let position = (y as usize / 2) * area.width as usize;
            for (x, _, pixel) in row {
                data[position + x as usize].update_lower(*pixel);
            }
        }
    }

    Ok(data)
}

pub struct HalfBlock {
    upper: Color,
    lower: Color,
}

impl HalfBlock {
    fn new(pixel: Rgb<u8>) -> Self {
        Self {
            upper: Color::Rgb(pixel[0], pixel[1], pixel[2]),
            lower: Color::Reset,
        }
    }

    fn update_lower(&mut self, pixel: Rgb<u8>) {
        self.lower = Color::Rgb(pixel[0], pixel[1], pixel[2]);
    }

    pub fn set_cell(&self, cell: &mut Cell) {
        cell.set_char(half_block::UPPER)
            .set_fg(self.upper)
            .set_bg(self.lower);
    }
}

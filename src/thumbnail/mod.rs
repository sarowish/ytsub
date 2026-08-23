pub mod protocols;

use crate::{
    emulator::ClearNeeded,
    thumbnail::protocols::{GraphicsProtocol, chafa, halfblocks, kitty::place, ueberzug},
};
use anyhow::Result;
use crossterm::{execute, style::Print};
use protocols::ImageData;
use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::{Rect, Size},
};
use std::{fmt::Write, num::NonZeroU16};

const UNIT_WIDTH: CellDiffOption = CellDiffOption::ForcedWidth(NonZeroU16::new(1).unwrap());

pub struct Thumbnail {
    pub data: ImageData,
    pub width: u16,
    pub height: u16,
    pub area: Option<Rect>,
    pub covered_area: Option<Rect>,
}

impl Thumbnail {
    pub const fn new(data: ImageData, width: u16, height: u16) -> Self {
        Self {
            data,
            width,
            height,
            area: None,
            covered_area: None,
        }
    }

    pub fn render(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        image_size: Size,
        clear: ClearNeeded,
    ) -> Result<()> {
        let previous_area = self.area.replace(area);

        let mut erase = match clear {
            ClearNeeded::Full => area_clear_sequence(area)?,
            ClearNeeded::LastLine => last_line_clear_sequence(area)?,
            ClearNeeded::ImageAnchor => {
                clear_previous_image_anchor(previous_area, area)?;
                image_anchor_clear_sequence(area)?
            }
            ClearNeeded::None => String::new(),
        };

        match &self.data {
            ImageData::Kgp => {
                let place = place(area)?;
                render_linewise_by_first_cells(buf, area, place);
            }
            ImageData::Iip(data) | ImageData::Sixel(data) => {
                erase.push_str(data);
                render_by_first_cell(buf, area, image_size, &erase);
            }
            ImageData::Ueberzug(path) => ueberzug::display_image(path, area)?,
            ImageData::Chafa(path) => {
                let output = chafa::show_image(path, area)?;
                erase.push_str(&String::from_utf8_lossy(&output));

                render_linewise_by_first_cells(buf, area, erase.split('\n'));
            }
            ImageData::HalfBlocks(path) => {
                let data = halfblocks::display_image(path, area)?;
                let mut blocks = data.iter();

                for y in area.top()..(area.bottom()) {
                    for x in area.left()..area.right() {
                        if let Some(block) = blocks.next()
                            && let Some(cell) = buf.cell_mut((x, y))
                        {
                            block.set_cell(cell);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn needs_rerender(
        &self,
        prev_covered_area: Option<Rect>,
        graphics_protocol: GraphicsProtocol,
    ) -> bool {
        graphics_protocol.uses_skipped_cells()
            && prev_covered_area.is_some_and(|prev_area| {
                self.covered_area
                    .is_none_or(|cur_area| cur_area.intersection(prev_area) != prev_area)
            })
    }
}

pub fn area_clear_sequence(area: Rect) -> Result<String> {
    let mut erase = String::new();

    for _ in 0..area.height {
        write!(erase, "\x1b[{}X\x1b[1B", area.width)?;
    }
    write!(erase, "\x1b[{}A", area.height)?;

    Ok(erase)
}

fn last_line_clear_sequence(area: Rect) -> Result<String> {
    let mut erase = String::new();

    write!(erase, "\x1b[{}B", area.height.saturating_sub(1))?;
    write!(erase, "\x1b[{}X", area.width)?;

    write!(erase, "\x1b[{}C", area.width.saturating_sub(1))?;
    for _ in 0..area.height {
        write!(erase, "\x1b[1X\x1b[1A")?;
    }
    write!(erase, "\x1b[1B")?;
    write!(erase, "\x1b[{}D", area.width.saturating_sub(1))?;

    Ok(erase)
}

fn image_anchor_clear_sequence(area: Rect) -> Result<String> {
    let mut erase = String::from(" \x1b[1D");
    erase.push_str(&last_line_clear_sequence(area)?);

    Ok(erase)
}

fn clear_previous_image_anchor(previous_area: Option<Rect>, area: Rect) -> Result<()> {
    if let Some(previous_area) = previous_area
        && (previous_area.x != area.x || previous_area.y != area.y)
    {
        let erase = format!(
            "\x1b7\x1b[{};{}H \x1b8",
            u32::from(previous_area.y) + 1,
            u32::from(previous_area.x) + 1
        );

        execute!(std::io::stdout(), Print(erase))?;
    }

    Ok(())
}

fn render_by_first_cell(buf: &mut Buffer, area: Rect, image_size: Size, data: &str) {
    buf.cell_mut(area)
        .map(|cell| cell.set_symbol(data).set_diff_option(UNIT_WIDTH));
    let mut skip_first = false;

    for y in area.top()..(area.bottom()) {
        for x in area.left()..area.right() {
            if !skip_first {
                skip_first = true;
                continue;
            }
            buf.cell_mut((x, y))
                .map(|cell| cell.set_diff_option(CellDiffOption::Skip));
        }
    }

    let image_area =
        Rect::new(area.x, area.y, image_size.width, image_size.height).intersection(buf.area);

    for y in image_area.top()..image_area.bottom() {
        let x_start = if y < area.bottom() {
            area.right()
        } else {
            image_area.left()
        };

        for x in x_start..image_area.right() {
            buf.cell_mut((x, y))
                .filter(|cell| cell.diff_option == CellDiffOption::None)
                .map(|cell| cell.set_diff_option(CellDiffOption::AlwaysUpdate));
        }
    }
}

fn render_linewise_by_first_cells<T>(buf: &mut Buffer, area: Rect, data: T)
where
    T: IntoIterator,
    T::Item: AsRef<str>,
{
    for (y, line) in data.into_iter().enumerate() {
        let row = area.top() + y as u16;
        let line = line.as_ref();

        buf.cell_mut((area.left(), row))
            .map(|cell| cell.set_symbol(line).set_diff_option(UNIT_WIDTH));

        for x in (area.left() + 1)..area.right() {
            buf.cell_mut((x, row))
                .map(|cell| cell.set_diff_option(CellDiffOption::Skip));
        }
    }
}

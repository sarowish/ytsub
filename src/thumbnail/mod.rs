pub mod protocols;

use crate::{
    emulator::ClearNeeded,
    thumbnail::protocols::{GraphicsProtocol, chafa, halfblocks, kitty::place, ueberzug},
};
use anyhow::Result;
use protocols::ImageData;
use ratatui::{buffer::Buffer, layout::Rect};
use std::fmt::Write;

pub struct Thumbnail {
    pub data: ImageData,
    pub width: u16,
    pub height: u16,
    pub area: Option<Rect>,
    pub covered_area: Option<Rect>,
}

impl Thumbnail {
    pub fn new(data: ImageData, width: u16, height: u16) -> Self {
        Self {
            data,
            width,
            height,
            area: None,
            covered_area: None,
        }
    }

    pub fn render(&mut self, buf: &mut Buffer, area: Rect, clear: ClearNeeded) -> Result<()> {
        self.area = Some(area);

        let mut erase = match clear {
            ClearNeeded::Full => clear_area(area)?,
            ClearNeeded::LastLine => clear_last_line(area)?,
            ClearNeeded::None => String::new(),
        };

        match &self.data {
            ImageData::Kgp => {
                let place = place(area)?;
                render_linewise_by_first_cells(buf, area, place);
            }
            ImageData::Iip(data) | ImageData::Sixel(data) => {
                erase.push_str(data);
                render_by_first_cell(buf, area, &erase);
            }
            ImageData::Ueberzug(path) => ueberzug::display_image(path, &area)?,
            ImageData::Chafa(path) => {
                let output = chafa::show_image(path, &area)?;
                erase.push_str(&String::from_utf8_lossy(&output));

                render_linewise_by_first_cells(buf, area, erase.split('\n'));
            }
            ImageData::HalfBlocks(path) => {
                let data = halfblocks::display_image(path, &area)?;
                let mut blocks = data.iter();

                for y in area.top()..(area.bottom()) {
                    for x in area.left()..area.right() {
                        if let Some(block) = blocks.next()
                            && let Some(cell) = buf.cell_mut((x, y))
                        {
                            block.set_cell(cell)
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

pub fn clear_area(area: Rect) -> Result<String> {
    let mut erase = String::new();

    for _ in 0..area.height {
        write!(erase, "\x1b[0K\x1b[1B")?;
    }
    write!(erase, "\x1b[{}A", area.height)?;

    Ok(erase)
}

fn clear_last_line(area: Rect) -> Result<String> {
    let mut erase = String::new();

    write!(erase, "\x1b[{}B", area.height.saturating_sub(1))?;
    write!(erase, "\x1b[0K")?;

    write!(erase, "\x1b[{}C", area.width.saturating_sub(1))?;
    for _ in 0..area.height {
        write!(erase, "\x1b[1X\x1b[1A")?;
    }
    write!(erase, "\x1b[1B")?;
    write!(erase, "\x1b[{}D", area.width.saturating_sub(1))?;

    Ok(erase)
}

fn render_by_first_cell(buf: &mut Buffer, area: Rect, data: &str) {
    buf.cell_mut(area).map(|cell| cell.set_symbol(data));
    let mut skip_first = false;

    for y in area.top()..(area.bottom()) {
        for x in area.left()..area.right() {
            if !skip_first {
                skip_first = true;
                continue;
            }
            buf.cell_mut((x, y)).map(|cell| cell.set_skip(true));
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
            .map(|cell| cell.set_symbol(line));

        for x in (area.left() + 1)..area.right() {
            buf.cell_mut((x, row)).map(|cell| cell.set_skip(true));
        }
    }
}

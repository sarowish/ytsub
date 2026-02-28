pub mod emulator;
pub mod mux;
pub mod protocols;

use crate::thumbnail::{emulator::ClearNeeded, protocols::kitty::place};
use anyhow::Result;
use crossterm::{
    cursor::{RestorePosition, SavePosition},
    execute,
    style::Print,
};
use protocols::ImageData;
use ratatui::{buffer::Buffer, layout::Rect};
use std::fmt::Write;

pub struct Thumbnail {
    pub data: ImageData,
    pub width: u16,
    pub height: u16,
    pub area: Option<Rect>,
}

impl Thumbnail {
    pub fn render(&mut self, buf: &mut Buffer, area: Rect, clear: ClearNeeded) -> Result<()> {
        let area_changed = self.area.is_none_or(|cur| cur != area);

        self.area = Some(area);

        match &self.data {
            ImageData::Kgp => {
                send_buffer(&place(area)?)?;
                draw_thumbnail(buf, area, "");
            }
            ImageData::Iip(data) | ImageData::Sixel(data) => {
                let mut erase = match clear {
                    ClearNeeded::Full if area_changed => clear_area(area)?,
                    ClearNeeded::LastLine if area_changed => clear_last_line(area)?,
                    _ => String::new(),
                };
                erase.push_str(data);
                draw_thumbnail(buf, area, &erase);
            }
        }

        Ok(())
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

    write!(erase, "\x1b[{}B", area.height - 1)?;
    write!(erase, "\x1b[0K")?;

    write!(erase, "\x1b[{}C", area.width - 1)?;
    for _ in 0..area.height {
        write!(erase, "\x1b[1X\x1b[1A")?;
    }
    write!(erase, "\x1b[1B")?;
    write!(erase, "\x1b[{}D", area.width - 1)?;

    Ok(erase)
}

fn draw_thumbnail(buf: &mut Buffer, area: Rect, data: &str) {
    let mut skip_first = if data.is_empty() {
        true
    } else {
        buf.cell_mut(area).map(|cell| cell.set_symbol(data));
        false
    };

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

fn send_buffer(buf: &str) -> Result<()> {
    execute!(std::io::stdout(), SavePosition, Print(buf), RestorePosition)?;

    Ok(())
}

use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend as RatatuiCrosstermBackend, WindowSize},
    buffer::{Cell, CellWidth},
    layout::{Position, Size},
};
use std::io::{self, Write};

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub struct CrosstermBackend<W: Write> {
    inner: RatatuiCrosstermBackend<W>,
}

impl<W: Write> CrosstermBackend<W> {
    pub const fn new(writer: W) -> Self {
        Self {
            inner: RatatuiCrosstermBackend::new(writer),
        }
    }
}

impl<W: Write> Backend for CrosstermBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let cells = content.collect::<Vec<_>>();
        let mut chunk_start = 0;

        for index in 1..cells.len() {
            let (previous_x, previous_y, previous_cell) = cells[index - 1];
            let (x, y, _) = cells[index];
            let follows_previous_cell = previous_x.checked_add(1) == Some(x) && previous_y == y;

            if follows_previous_cell && previous_cell.cell_width() != 1 {
                self.inner.draw(cells[chunk_start..index].iter().copied())?;
                chunk_start = index;
            }
        }

        self.inner.draw(cells[chunk_start..].iter().copied())
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        self.inner.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

use super::Thumbnail;
use super::mux::{self, detect_tmux};
use super::protocols::GraphicsProtocol;
use crate::thumbnail::mux::IS_TMUX;
use anyhow::{Result, bail};
use crossterm::{
    cursor::{RestorePosition, SavePosition},
    execute, queue,
    style::Print,
};
use std::{env, io::Read};

#[derive(Clone, Copy)]
pub enum ClearNeeded {
    Full,
    LastLine,
    None,
}

pub struct Emulator {
    pub graphics_protocol: GraphicsProtocol,
    pub clear_needed: ClearNeeded,
    pub cell_height: u16,
    pub cell_width: u16,
    pub thumbnail: Option<Thumbnail>,
}

impl Emulator {
    pub fn new() -> Result<Self> {
        detect_tmux();

        let mut w = std::io::stdout();
        let mut r = std::io::stdin();

        queue!(w, SavePosition,)?;

        let term_program = get_term_program();
        let is_konsole = env::var("KONSOLE_VERSION").is_ok_and(|s| !s.is_empty());

        if !(term_program.as_ref().is_some_and(|t| t == "WezTerm") || is_konsole) {
            queue!(
                w,
                Print(mux::csi("\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"))
            )?;
        }

        execute!(
            w,
            Print(mux::csi("\x1b[c")),
            Print("\x1b[16t"),
            Print(mux::csi("\x1b[5n")),
            RestorePosition,
        )?;

        let mut parser = ResponseParser::default();
        let mut responses = Vec::new();

        'outer: loop {
            let mut buf: [u8; 128] = [0; 128];
            let n = r.read(&mut buf)?;

            if n == 0 {
                break;
            }

            for ch in buf.iter().take(n) {
                if let Some(resp) = parser.process(char::from(*ch)) {
                    parser.reset();
                    responses.push(resp);

                    if let ParserResponse::Status = resp {
                        break 'outer;
                    }
                }
            }
        }

        let mut graphics_protocol = None;
        let mut cell_size = None;

        for resp in responses {
            match resp {
                ParserResponse::SupportsKgp => {
                    graphics_protocol = Some(GraphicsProtocol::Kgp);
                }
                ParserResponse::SupportsSixel => {
                    if graphics_protocol.is_none() {
                        graphics_protocol = Some(GraphicsProtocol::Sixel);
                    }
                }
                a @ ParserResponse::CellSize(_, _) => cell_size = Some(a),
                _ => {}
            }
        }

        if let Some(term) = &term_program
            && supports_iip(term)
        {
            graphics_protocol = Some(GraphicsProtocol::Iip);
        }

        if cell_size.is_none()
            && let Ok(mut window) = crossterm::terminal::window_size()
        {
            if (window.columns == 0 || window.rows == 0)
                && let Ok((columns, rows)) = crossterm::terminal::size()
            {
                window.columns = columns;
                window.rows = rows;
            }

            cell_size = Some(ParserResponse::CellSize(
                window.height / window.rows,
                window.width / window.columns,
            ));
        }

        if let Some(gp) = graphics_protocol
            && let Some(ParserResponse::CellSize(height, width)) = cell_size
        {
            Ok(Self {
                graphics_protocol: gp,
                clear_needed: clear_needed(gp, term_program),
                cell_height: height,
                cell_width: width,
                thumbnail: None,
            })
        } else {
            bail!("Won't be able to show thumbnails");
        }
    }
}

fn clear_needed(graphics_protocol: GraphicsProtocol, term_program: Option<String>) -> ClearNeeded {
    if term_program.is_some_and(|t| t == "WarpTerminal") {
        return ClearNeeded::Full;
    }

    match graphics_protocol {
        GraphicsProtocol::Kgp => ClearNeeded::None,
        GraphicsProtocol::Iip | GraphicsProtocol::Sixel => ClearNeeded::LastLine,
    }
}

fn get_term_program() -> Option<String> {
    if *IS_TMUX {
        mux::read_term()
    } else {
        env::var("TERM_PROGRAM").ok()
    }
}

fn supports_iip(term_program: &str) -> bool {
    term_program.contains("iTerm")
        || term_program.contains("WezTerm")
        || term_program.contains("mintty")
        || term_program.contains("vscode")
        || term_program.contains("Tabby")
        || term_program.contains("Hyper")
        || term_program.contains("rio")
        || term_program.contains("Bobcat")
        || term_program.contains("WarpTerminal")
}

#[derive(Default)]
enum ParserState {
    #[default]
    Unknown,
    Kitty,
    Csi,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum ParserResponse {
    SupportsKgp,
    SupportsSixel,
    CellSize(u16, u16),
    Status,
}

#[derive(Default)]
struct ResponseParser {
    data: String,
    state: ParserState,
}

impl ResponseParser {
    fn process(&mut self, next: char) -> Option<ParserResponse> {
        match self.state {
            ParserState::Unknown => {
                match (self.data.as_str(), next) {
                    (_, '\x1b') => {
                        self.reset();
                        return None;
                    }
                    ("_Gi=31", ';') => {
                        self.state = ParserState::Kitty;
                    }
                    ("[", _) => {
                        self.state = ParserState::Csi;
                    }
                    _ => {}
                }

                self.data.push(next);
            }
            ParserState::Kitty => {
                if next == '\\' {
                    if self.data == "_Gi=31;OK\x1b" {
                        return Some(ParserResponse::SupportsKgp);
                    }

                    self.reset();
                } else {
                    self.data.push(next);
                }
            }
            ParserState::Csi => match next {
                'n' => {
                    if self.data.starts_with("[0") {
                        return Some(ParserResponse::Status);
                    }
                }
                'c' => {
                    if self.data.starts_with("[?") && self.data[2..].split(';').any(|p| p == "4") {
                        return Some(ParserResponse::SupportsSixel);
                    }

                    self.reset();
                }
                't' => {
                    if self.data.starts_with("[6;")
                        && let Some((height, width)) = self.data[3..].split_once(';')
                        && let Ok(height) = height.parse()
                        && let Ok(width) = width.parse()
                    {
                        return Some(ParserResponse::CellSize(height, width));
                    }
                }
                _ => self.data.push(next),
            },
        }

        None
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::ParserResponse;
    use super::ResponseParser;

    fn parse(data: &str) -> Vec<ParserResponse> {
        let mut parser = ResponseParser::default();
        let mut responses = Vec::new();

        for next in data.chars() {
            if let Some(resp) = parser.process(next) {
                parser.reset();
                responses.push(resp);
            }
        }

        responses
    }

    #[test]
    fn kitty() {
        let data = "\x1b_Gi=31;OK\x1b\\";

        assert_eq!(parse(data), vec![ParserResponse::SupportsKgp]);
    }

    #[test]
    fn sixel() {
        let data = "\x1b[?62;4;22;28;52c";

        assert_eq!(parse(data), vec![ParserResponse::SupportsSixel]);
    }

    #[test]
    fn cell_size() {
        let data = "\x1b[6;17;8t";

        assert_eq!(parse(data), vec![ParserResponse::CellSize(17, 8)]);
    }

    #[test]
    fn no_cell_size() {
        let data = "\x1b[?6c\x1b[0n";

        assert_eq!(parse(data), vec![ParserResponse::Status]);
    }

    #[test]
    fn bulk() {
        let data = "\x1b_Gi=31;OK\x1b\\\x1b[?62;52;c\x1b[6;18;9t\x1b[0n";

        assert_eq!(
            parse(data),
            vec![
                ParserResponse::SupportsKgp,
                ParserResponse::CellSize(18, 9),
                ParserResponse::Status
            ]
        );
    }

    #[test]
    fn garbage() {
        let data = "\x1buuh\x1b";

        assert_eq!(parse(data), vec![]);
    }
}

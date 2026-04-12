use super::Thumbnail;
use super::mux::{self, detect_tmux};
use super::protocols::GraphicsProtocol;
use crate::clipboard::OSC52_SUPPORTED;
use crate::thumbnail::mux::IS_TMUX;
use crate::thumbnail::protocols::ueberzug;
use crate::utils::{binary_exists, env_var_is_set};
use anyhow::{Result, bail};
use crossterm::{
    cursor::{RestorePosition, SavePosition},
    execute, queue,
    style::Print,
};
use std::fmt::Write as _;
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

        queue!(w, SavePosition,)?;

        let term_program = get_term_program();
        let is_konsole = env_var_is_set("KONSOLE_VERSION");

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

        let responses = read_responses()?;

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
                ParserResponse::SupportsOsc52 => {
                    OSC52_SUPPORTED.init(true);
                }
                ParserResponse::CellSize(h, w) => cell_size = Some((h, w)),
                _ => {}
            }
        }

        if !*OSC52_SUPPORTED {
            execute!(
                w,
                Print(xtgettcap_query(&["Ms"])),
                Print(mux::csi("\x1b[5n"))
            )?;

            if read_responses()?.contains(&ParserResponse::SupportsOsc52) {
                OSC52_SUPPORTED.init(true);
            }
        }

        if let Some(term) = &term_program
            && supports_iip(term)
        {
            graphics_protocol = Some(GraphicsProtocol::Iip);
        } else if graphics_protocol.is_none() {
            if binary_exists("ueberzugpp")
                && let Some(method) = ueberzug::compositor_support()
            {
                ueberzug::METHOD
                    .set(method)
                    .expect("Emulator capabilites should only be detected once");
                ueberzug::start();
                graphics_protocol = Some(GraphicsProtocol::Ueberzug);
            } else if binary_exists("chafa") {
                graphics_protocol = Some(GraphicsProtocol::Chafa);
            } else {
                graphics_protocol = Some(GraphicsProtocol::HalfBlocks);
            }
        }

        cell_size = cell_size.or_else(cell_size_fallback).or_else(|| {
            // use a default size for symbol related protocols
            graphics_protocol
                .is_some_and(|p| {
                    matches!(p, GraphicsProtocol::Chafa | GraphicsProtocol::HalfBlocks)
                })
                .then_some((18, 9))
        });

        if let Some(gp) = graphics_protocol
            && let Some((height, width)) = cell_size
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

fn cell_size_fallback() -> Option<(u16, u16)> {
    let mut window = crossterm::terminal::window_size().ok()?;

    if (window.columns == 0 || window.rows == 0)
        && let Ok((columns, rows)) = crossterm::terminal::size()
    {
        window.columns = columns;
        window.rows = rows;
    }

    if window.columns == 0 || window.rows == 0 || window.height == 0 || window.width == 0 {
        None
    } else {
        Some((window.height / window.rows, window.width / window.columns))
    }
}

fn read_responses() -> Result<Vec<ParserResponse>> {
    let mut r = std::io::stdin();
    let mut buf: [u8; 128] = [0; 128];
    let mut responses = Vec::new();
    let mut parser = ResponseParser::default();

    'outer: loop {
        let n = r.read(&mut buf)?;

        if n == 0 {
            break;
        }

        for ch in buf.iter().take(n) {
            let mut v = parser.process(char::from(*ch));

            if !v.is_empty() {
                parser.reset();
            }

            if v == [ParserResponse::Status] {
                break 'outer;
            }

            responses.append(&mut v);
        }
    }

    Ok(responses)
}

fn clear_needed(graphics_protocol: GraphicsProtocol, term_program: Option<String>) -> ClearNeeded {
    if term_program.is_some_and(|t| t == "WarpTerminal") {
        return ClearNeeded::Full;
    }

    match graphics_protocol {
        GraphicsProtocol::Kgp | GraphicsProtocol::Ueberzug | GraphicsProtocol::HalfBlocks => {
            ClearNeeded::None
        }
        GraphicsProtocol::Iip | GraphicsProtocol::Sixel => ClearNeeded::LastLine,
        GraphicsProtocol::Chafa => ClearNeeded::Full,
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
        || env_var_is_set("MLTERM")
}

#[derive(Default)]
enum ParserState {
    #[default]
    Unknown,
    Kitty,
    Csi,
    Terminfo,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum ParserResponse {
    SupportsKgp,
    SupportsSixel,
    CellSize(u16, u16),
    SupportsOsc52,
    Status,
}

#[derive(Default)]
struct ResponseParser {
    data: String,
    state: ParserState,
}

impl ResponseParser {
    fn process(&mut self, next: char) -> Vec<ParserResponse> {
        match self.state {
            ParserState::Unknown => {
                match (self.data.as_str(), next) {
                    (_, '\x1b') => {
                        self.reset();
                        return Vec::new();
                    }
                    ("_Gi=31", ';') => {
                        self.state = ParserState::Kitty;
                    }
                    ("[", _) => {
                        self.state = ParserState::Csi;
                    }
                    ("P1+r", _) => {
                        self.data.clear();
                        self.state = ParserState::Terminfo;
                    }
                    _ => {}
                }

                self.data.push(next);
            }
            ParserState::Kitty => {
                if next == '\\' {
                    if self.data == "_Gi=31;OK\x1b" {
                        return vec![ParserResponse::SupportsKgp];
                    }

                    self.reset();
                } else {
                    self.data.push(next);
                }
            }
            ParserState::Csi => match next {
                'n' => {
                    if self.data.starts_with("[0") {
                        return vec![ParserResponse::Status];
                    }
                }
                'c' => {
                    let mut attributes = Vec::new();

                    if let Some(s) = self.data.strip_prefix("[?") {
                        for attr in s.split(';') {
                            match attr {
                                "4" => attributes.push(ParserResponse::SupportsSixel),
                                "52" => attributes.push(ParserResponse::SupportsOsc52),
                                _ => {}
                            }
                        }
                    }

                    self.reset();
                    return attributes;
                }
                't' => {
                    if let Some(s) = self.data.strip_prefix("[6;")
                        && let Some((height, width)) = s.split_once(';')
                        && let Ok(height) = height.parse::<f32>()
                        && let Ok(width) = width.parse::<f32>()
                    {
                        return vec![ParserResponse::CellSize(height as u16, width as u16)];
                    }

                    self.reset();
                }
                _ => self.data.push(next),
            },
            ParserState::Terminfo => match next {
                '\x1b' => {
                    if let Some((key, value)) = self.data.split_once('=')
                        && decode_xtgettcap_hex(key).is_some_and(|k| k == "Ms")
                        && decode_xtgettcap_hex(value).is_some()
                    {
                        return vec![ParserResponse::SupportsOsc52];
                    }
                }
                _ => self.data.push(next),
            },
        }

        Vec::new()
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

fn xtgettcap_query(names: &[&str]) -> String {
    let mut s = String::from("\x1bP+q");

    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            s.push(';');
        }

        for b in name.as_bytes() {
            write!(&mut s, "{:02x}", b).unwrap();
        }
    }

    s.push_str("\x1b\\");

    s
}

fn decode_xtgettcap_hex(s: &str) -> Option<String> {
    if !s.len().is_multiple_of(2) {
        return None;
    }

    let mut decoded = Vec::with_capacity(s.len() / 2);
    let chunks = s.as_bytes().chunks_exact(2);

    for chunk in chunks {
        let hex = str::from_utf8(chunk).ok()?;
        let b = u8::from_str_radix(hex, 16).ok()?;
        decoded.push(b);
    }

    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::ParserResponse;
    use super::ResponseParser;

    fn parse(data: &str) -> Vec<ParserResponse> {
        let mut parser = ResponseParser::default();
        let mut responses = Vec::new();

        for next in data.chars() {
            let mut v = parser.process(next);

            if !v.is_empty() {
                parser.reset();
            }

            responses.append(&mut v);
        }

        responses
    }

    #[test]
    fn kitty() {
        let data = "\x1b_Gi=31;OK\x1b\\";

        assert_eq!(parse(data), vec![ParserResponse::SupportsKgp]);
    }

    #[test]
    fn primary_device_attributes() {
        let data = "\x1b[?62;4;22;28;52c";

        assert_eq!(
            parse(data),
            vec![ParserResponse::SupportsSixel, ParserResponse::SupportsOsc52]
        );
    }

    #[test]
    fn sixel() {
        let data = "\x1b[?4c";

        assert_eq!(parse(data), vec![ParserResponse::SupportsSixel]);
    }

    #[test]
    fn cell_size() {
        let data = "\x1b[6;17;8t";

        assert_eq!(parse(data), vec![ParserResponse::CellSize(17, 8)]);
    }

    #[test]
    fn cell_size_floating_point() {
        let data = "\x1b[6;17;8.203125t";

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
                ParserResponse::SupportsOsc52,
                ParserResponse::CellSize(18, 9),
                ParserResponse::Status
            ]
        );
    }

    #[test]
    fn xtgettcap() {
        let data = "\x1bP1+r4d73=1B5D35323B25703125733B25703225731B5C\x1b\\";

        assert_eq!(parse(data), vec![ParserResponse::SupportsOsc52]);
    }

    #[test]
    fn garbage() {
        let data = "\x1buuh\x1b";

        assert_eq!(parse(data), vec![]);
    }
}

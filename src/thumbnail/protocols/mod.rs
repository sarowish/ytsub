pub mod halfblocks;
pub mod iip;
pub mod kitty;
pub mod sixel;
pub mod ueberzug;

use std::path::PathBuf;

use anyhow::Result;
use image::DynamicImage;

#[derive(Debug, Copy, Clone)]
pub enum GraphicsProtocol {
    Kgp,
    Iip,
    Sixel,
    Ueberzug,
    HalfBlocks,
}

pub enum ImageData {
    Kgp,
    Iip(String),
    Sixel(String),
    Ueberzug(PathBuf),
    HalfBlocks(PathBuf),
}

impl GraphicsProtocol {
    pub fn display_image(self, image: DynamicImage, path: PathBuf) -> Result<ImageData> {
        let s = match self {
            GraphicsProtocol::Kgp => {
                kitty::display_image(image)?;
                ImageData::Kgp
            }
            GraphicsProtocol::Iip => ImageData::Iip(iip::display_image(&image)?),
            GraphicsProtocol::Sixel => ImageData::Sixel(sixel::display_image(image)?),
            GraphicsProtocol::Ueberzug => ImageData::Ueberzug(path),
            GraphicsProtocol::HalfBlocks => ImageData::HalfBlocks(path),
        };

        Ok(s)
    }
}

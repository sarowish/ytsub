pub mod chafa;
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
    Chafa,
    HalfBlocks,
}

pub enum ImageData {
    Kgp,
    Iip(String),
    Sixel(String),
    Ueberzug(PathBuf),
    Chafa(PathBuf),
    HalfBlocks(PathBuf),
}

impl GraphicsProtocol {
    pub fn display_image(self, image: DynamicImage, path: PathBuf) -> Result<ImageData> {
        let s = match self {
            Self::Kgp => {
                kitty::display_image(image)?;
                ImageData::Kgp
            }
            Self::Iip => ImageData::Iip(iip::display_image(&image)?),
            Self::Sixel => ImageData::Sixel(sixel::display_image(image)?),
            Self::Ueberzug => ImageData::Ueberzug(path),
            Self::Chafa => ImageData::Chafa(path),
            Self::HalfBlocks => ImageData::HalfBlocks(path),
        };

        Ok(s)
    }

    pub const fn uses_skipped_cells(self) -> bool {
        matches!(self, Self::Kgp | Self::Iip | Self::Sixel | Self::Chafa)
    }
}

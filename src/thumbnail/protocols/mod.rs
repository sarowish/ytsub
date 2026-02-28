pub mod iip;
pub mod kitty;
pub mod sixel;

use anyhow::Result;
use image::DynamicImage;

#[derive(Debug, Copy, Clone)]
pub enum GraphicsProtocol {
    Kgp,
    Iip,
    Sixel,
}

pub enum ImageData {
    Kgp,
    Iip(String),
    Sixel(String),
}

impl GraphicsProtocol {
    pub fn display_image(self, image: DynamicImage) -> Result<ImageData> {
        let s = match self {
            GraphicsProtocol::Kgp => {
                kitty::display_image(image)?;
                ImageData::Kgp
            }
            GraphicsProtocol::Iip => ImageData::Iip(iip::display_image(&image)?),
            GraphicsProtocol::Sixel => ImageData::Sixel(sixel::display_image(image)?),
        };

        Ok(s)
    }
}

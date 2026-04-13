use crate::emulator::mux::{END, START};
use anyhow::Result;
use base64::{Engine, engine::Config, prelude::BASE64_STANDARD};
use image::{DynamicImage, codecs::jpeg::JpegEncoder};
use std::fmt::Write;

pub fn display_image(image: &DynamicImage) -> Result<String> {
    let width = image.width();
    let height = image.height();
    let mut b = vec![];

    JpegEncoder::new(&mut b).encode_image(image)?;

    let mut buf = String::with_capacity(
        200 + base64::encoded_len(b.len(), BASE64_STANDARD.config().encode_padding()).unwrap_or(0),
    );

    write!(
        buf,
        "{}]1337;File=size={};width={width}px;height={height}px;inline=1;doNotMoveCursor=1:",
        *START,
        b.len(),
    )?;

    BASE64_STANDARD.encode_string(b, &mut buf);

    write!(buf, "\x07{}", *END)?;

    Ok(buf)
}

//! PNG and JPEG screenshot encoding.
use image::ImageEncoder;
use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub(crate) fn write_png(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let encoder = image::codecs::png::PngEncoder::new(BufWriter::new(file));
    encoder.write_image(rgba, size[0], size[1], image::ExtendedColorType::Rgba8)?;
    Ok(())
}

pub(crate) fn write_jpeg(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(BufWriter::new(file), 95);
    encoder.write_image(rgba, size[0], size[1], image::ExtendedColorType::Rgba8)?;
    Ok(())
}

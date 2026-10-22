//! The output pk3: generated PNGs at the paths rend2's automatic lookup reads,
//! plus [`MANIFEST_PATH`], a JSON manifest of sources, outputs and settings.
//!
//! The archive is deterministic: entries sorted by path, a fixed 1980-01-01
//! timestamp, PNGs stored (they are already compressed), the manifest deflated.

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbImage, RgbaImage};
use serde::Serialize;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Where the manifest sits inside the pk3.
pub const MANIFEST_PATH: &str = "jkr-materialgen/manifest.json";

/// The notice repeated in the manifest, the help text and the docs.
pub const NOTICE: &str = "Generated from the textures of your own Jedi Academy installation. \
These images are derived from retail game data: keep them on this machine, and do not \
share, upload or commit them.";

/// One file of the archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The manifest's top level.
#[derive(Clone, Debug, Serialize)]
pub struct Manifest {
    pub tool: &'static str,
    pub version: &'static str,
    pub notice: &'static str,
    pub settings: ManifestSettings,
    pub sources: Vec<SourceEntry>,
    pub skipped: Vec<SkippedEntry>,
}

/// The settings a run used.
#[derive(Clone, Debug, Serialize)]
pub struct ManifestSettings {
    pub maps: Vec<String>,
    pub strength: f32,
    pub max_size: Option<u32>,
    pub limit: Option<usize>,
    pub normal_convention: &'static str,
    pub packed_layout: &'static str,
    pub gradient_radius: f32,
    pub gradient_passes: usize,
    pub height_bands: Vec<[f32; 2]>,
    pub coarse_weight: f32,
    pub min_height_range: f32,
}

/// One source texture and what was written for it.
#[derive(Clone, Debug, Serialize)]
pub struct SourceEntry {
    pub image: String,
    /// File name of the pk3 (or directory) the image came from.
    pub archive: String,
    pub width: u32,
    pub height: u32,
    pub class: &'static str,
    pub class_source: String,
    pub alpha_tested: bool,
    pub shaders: Vec<String>,
    pub maps: Vec<String>,
    pub triangles: u64,
    pub outputs: Vec<String>,
    /// rend2 maps that already existed and were left alone.
    pub existing: Vec<String>,
}

/// One shader or texture without maps, and why.
#[derive(Clone, Debug, Serialize)]
pub struct SkippedEntry {
    pub name: String,
    pub reason: String,
}

/// The PNG encoder: maximum deflate with adaptive row filters. Normal maps are
/// noisy, and the encoder's default fast mode barely compresses them.
fn encoder(bytes: &mut Vec<u8>) -> PngEncoder<&mut Vec<u8>> {
    PngEncoder::new_with_quality(bytes, CompressionType::Best, FilterType::Adaptive)
}

/// PNG of an RGBA image.
pub fn png_rgba(image: &RgbaImage) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    encoder(&mut bytes).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        ExtendedColorType::Rgba8,
    )?;
    Ok(bytes)
}

/// PNG of an RGB image.
pub fn png_rgb(image: &RgbImage) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    encoder(&mut bytes).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        ExtendedColorType::Rgb8,
    )?;
    Ok(bytes)
}

/// Write `entries` and `manifest` to `path` (through a temporary file next to
/// it, renamed at the end). Returns the archive size in bytes.
pub fn write_pk3(
    path: &Path,
    entries: &mut [Entry],
    manifest: &Manifest,
) -> Result<u64, Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("pk3.partial");
    {
        let file = BufWriter::new(File::create(&partial)?);
        let mut zip = ZipWriter::new(file);
        let stored = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::default());
        let deflated = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default());
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        for entry in entries.iter() {
            zip.start_file(entry.path.as_str(), stored)?;
            zip.write_all(&entry.bytes)?;
        }
        zip.start_file(MANIFEST_PATH, deflated)?;
        let mut json = serde_json::to_vec_pretty(manifest)?;
        json.push(b'\n');
        zip.write_all(&json)?;
        zip.finish()?.flush()?;
    }
    fs::rename(&partial, path)?;
    Ok(fs::metadata(path)?.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use jkr_shader::ShaderCatalog;
    use jkr_vfs::VirtualFileSystem;
    use std::io::Read;

    fn manifest() -> Manifest {
        Manifest {
            tool: "jkr-materialgen",
            version: "test",
            notice: NOTICE,
            settings: ManifestSettings {
                maps: vec!["mp/test".into()],
                strength: 1.0,
                max_size: None,
                limit: None,
                normal_convention: "test",
                packed_layout: "test",
                gradient_radius: 32.0,
                gradient_passes: 2,
                height_bands: vec![[1.0, 0.5]],
                coarse_weight: 0.5,
                min_height_range: 0.1,
            },
            sources: vec![SourceEntry {
                image: "textures/a/wall.jpg".into(),
                archive: "assets1.pk3".into(),
                width: 4,
                height: 4,
                class: "stone",
                class_source: "default".into(),
                alpha_tested: false,
                shaders: vec!["textures/a/wall".into()],
                maps: vec!["mp/test".into()],
                triangles: 2,
                outputs: vec![
                    "textures/a/wall_nh.png".into(),
                    "textures/a/wall_rmo.png".into(),
                ],
                existing: Vec::new(),
            }],
            skipped: vec![SkippedEntry {
                name: "textures/skies/x".into(),
                reason: "sky".into(),
            }],
        }
    }

    #[test]
    fn pk3_layout_and_manifest() {
        let directory =
            std::env::temp_dir().join(format!("jkr-materialgen-test-{}", std::process::id()));
        let path = directory.join("zzz_jkr_materials.pk3");
        let normal = RgbaImage::from_pixel(4, 4, Rgba([128, 128, 255, 77]));
        let packed = RgbImage::from_pixel(4, 4, image::Rgb([200, 0, 255]));
        let mut entries = vec![
            Entry {
                path: "textures/a/wall_rmo.png".into(),
                bytes: png_rgb(&packed).expect("png"),
            },
            Entry {
                path: "textures/a/wall_nh.png".into(),
                bytes: png_rgba(&normal).expect("png"),
            },
        ];
        let size = write_pk3(&path, &mut entries, &manifest()).expect("writes");
        assert_eq!(size, fs::metadata(&path).expect("exists").len());
        assert!(!path.with_extension("pk3.partial").exists());

        let mut archive = zip::ZipArchive::new(File::open(&path).expect("opens")).expect("zip");
        let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        assert_eq!(
            names,
            [
                "textures/a/wall_nh.png",
                "textures/a/wall_rmo.png",
                MANIFEST_PATH
            ]
        );
        let mut json = String::new();
        archive
            .by_name(MANIFEST_PATH)
            .expect("manifest")
            .read_to_string(&mut json)
            .expect("utf-8");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");
        assert_eq!(value["sources"][0]["image"], "textures/a/wall.jpg");
        assert_eq!(value["sources"][0]["outputs"][0], "textures/a/wall_nh.png");
        assert_eq!(value["settings"]["maps"][0], "mp/test");
        assert_eq!(value["skipped"][0]["reason"], "sky");

        // Mounted like a game pk3, rend2's lookup next to the diffuse finds the maps.
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_pk3(&path).expect("mounts");
        let catalog = ShaderCatalog::default();
        let found = catalog
            .resolve_stage_image(&vfs, "textures/a/wall_nh")
            .expect("lookup")
            .expect("found");
        assert_eq!(found.as_str(), "textures/a/wall_nh.png");
        let bytes = vfs
            .read(found.as_str())
            .expect("reads")
            .expect("present")
            .bytes;
        let decoded = image::load_from_memory(&bytes).expect("decodes").to_rgba8();
        assert_eq!(decoded.get_pixel(1, 1).0, [128, 128, 255, 77]);
        drop(vfs);
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[test]
    fn archives_are_deterministic() {
        let directory =
            std::env::temp_dir().join(format!("jkr-materialgen-det-{}", std::process::id()));
        let entry = |path: &str| Entry {
            path: path.into(),
            bytes: vec![1, 2, 3],
        };
        let first = directory.join("a.pk3");
        let second = directory.join("b.pk3");
        write_pk3(
            &first,
            &mut [entry("x_n.png"), entry("a_n.png")],
            &manifest(),
        )
        .expect("a");
        write_pk3(
            &second,
            &mut [entry("a_n.png"), entry("x_n.png")],
            &manifest(),
        )
        .expect("b");
        assert_eq!(fs::read(&first).expect("a"), fs::read(&second).expect("b"));
        fs::remove_dir_all(&directory).expect("cleanup");
    }
}

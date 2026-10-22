//! One generator run: mount, select, generate in parallel, write the pk3.

use crate::classes::MaterialClass;
use crate::generate::{
    BANDS, COARSE_WEIGHT, GRADIENT_PASSES, GRADIENT_RADIUS, MIN_HEIGHT_RANGE, PACKED_SUFFIX,
    Settings, generate,
};
use crate::mount::mount_game_data;
use crate::package::{
    Entry, Manifest, ManifestSettings, NOTICE, SkippedEntry, SourceEntry, png_rgb, png_rgba,
    write_pk3,
};
use crate::select::{Candidate, Selection, installed_maps, select, shader_uses};
use jkr_shader::ShaderCatalog;
use jkr_vfs::VirtualFileSystem;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Everything a run needs, as parsed from the command line.
#[derive(Clone, Debug)]
pub struct Options {
    pub game_data: PathBuf,
    pub fs_game: Option<String>,
    /// Map names (`mp/ffa3`); `None` for every installed map.
    pub maps: Option<Vec<String>>,
    /// Generate only the this many most-used textures.
    pub limit: Option<usize>,
    pub settings: Settings,
    pub dry_run: bool,
    pub out: PathBuf,
}

/// What a run did.
#[derive(Debug)]
pub struct Summary {
    pub selection: Selection,
    /// Textures that got maps.
    pub generated: usize,
    /// Images written.
    pub images: usize,
    /// Textures whose image failed to decode.
    pub failed: Vec<(String, String)>,
    /// Textures without relief, which got no maps.
    pub flat: Vec<String>,
    /// Archive size in bytes (0 for a dry run).
    pub bytes: u64,
    pub elapsed: Duration,
    /// Archives left out because they are this tool's output.
    pub excluded: Vec<PathBuf>,
}

/// Run the generator.
pub fn run(options: &Options) -> Result<Summary, Box<dyn Error>> {
    let started = Instant::now();
    let exclude = options
        .out
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let (vfs, excluded) =
        mount_game_data(&options.game_data, options.fs_game.as_deref(), &exclude)?;
    let mut shader_warnings = 0usize;
    let catalog = ShaderCatalog::load_with_warnings(&vfs, |_, _| shader_warnings += 1);
    if shader_warnings > 0 {
        eprintln!(
            "note: {shader_warnings} malformed shader definitions were skipped, as the client does"
        );
    }
    let maps = match &options.maps {
        Some(maps) => maps.iter().map(|map| normalize_map(map)).collect(),
        None => installed_maps(&vfs),
    };
    let map_uses = shader_uses(&vfs, &maps)?;
    for (map, error) in &map_uses.unreadable {
        eprintln!("warning: skipping map {map}: {error}");
    }
    let mut selection = select(&vfs, &catalog, map_uses.read, &map_uses.uses)?;
    if let Some(limit) = options.limit {
        selection.candidates.truncate(limit);
    }
    if options.dry_run {
        return Ok(Summary {
            selection,
            generated: 0,
            images: 0,
            failed: Vec::new(),
            flat: Vec::new(),
            bytes: 0,
            elapsed: started.elapsed(),
            excluded,
        });
    }

    let results = generate_all(&vfs, &selection.candidates, &options.settings);
    let mut entries = Vec::new();
    let mut sources = Vec::new();
    let mut failed = Vec::new();
    let mut flat = Vec::new();
    for (candidate, result) in selection.candidates.iter().zip(results) {
        match result {
            Ok(output) if output.entries.is_empty() => flat.push(candidate.image.clone()),
            Ok(output) => {
                sources.push(SourceEntry {
                    image: candidate.image.clone(),
                    archive: output.archive,
                    width: output.width,
                    height: output.height,
                    class: candidate.class.name,
                    class_source: candidate.class_source.describe(),
                    alpha_tested: candidate.alpha_tested,
                    shaders: candidate.shaders.iter().cloned().collect(),
                    maps: candidate.maps.iter().cloned().collect(),
                    triangles: candidate.triangles,
                    outputs: output.entries.iter().map(|e| e.path.clone()).collect(),
                    existing: candidate.existing.clone(),
                });
                entries.extend(output.entries);
            }
            Err(error) => failed.push((candidate.image.clone(), error)),
        }
    }
    let mut skipped: Vec<SkippedEntry> = selection
        .skipped
        .iter()
        .map(|skipped| SkippedEntry {
            name: skipped.name.clone(),
            reason: skipped.reason.describe().to_owned(),
        })
        .collect();
    skipped.extend(flat.iter().map(|image| SkippedEntry {
        name: image.clone(),
        reason: "no surface detail (flat colour)".to_owned(),
    }));
    skipped.extend(failed.iter().map(|(image, error)| SkippedEntry {
        name: image.clone(),
        reason: format!("image could not be decoded: {error}"),
    }));
    let manifest = Manifest {
        tool: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        notice: NOTICE,
        settings: ManifestSettings {
            maps: selection.maps.clone(),
            strength: options.settings.strength,
            max_size: options.settings.max_size,
            limit: options.limit,
            normal_convention: "tangent space, red +s (right), green +t (down the image), \
                                (128,128,255) flat; _nh alpha is height (255 high)",
            packed_layout: "_rmo: red roughness, green metalness, blue occlusion",
            gradient_radius: GRADIENT_RADIUS,
            gradient_passes: GRADIENT_PASSES,
            height_bands: BANDS.iter().map(|(r, w)| [*r, *w]).collect(),
            coarse_weight: COARSE_WEIGHT,
            min_height_range: MIN_HEIGHT_RANGE,
        },
        sources,
        skipped,
    };
    let images = entries.len();
    let bytes = write_pk3(&options.out, &mut entries, &manifest)?;
    Ok(Summary {
        generated: manifest.sources.len(),
        selection,
        images,
        failed,
        flat,
        bytes,
        elapsed: started.elapsed(),
        excluded,
    })
}

/// `maps/mp/ffa3.bsp`, `mp/ffa3.bsp` and `mp/ffa3` all name `mp/ffa3`.
pub fn normalize_map(name: &str) -> String {
    let name = name.replace('\\', "/").to_ascii_lowercase();
    let name = name.strip_prefix("maps/").unwrap_or(&name);
    name.strip_suffix(".bsp").unwrap_or(name).to_owned()
}

/// The generated files of one texture.
struct Output {
    archive: String,
    width: u32,
    height: u32,
    entries: Vec<Entry>,
}

/// Generate every candidate on all cores; results keep the candidates' order.
fn generate_all(
    vfs: &VirtualFileSystem,
    candidates: &[Candidate],
    settings: &Settings,
) -> Vec<Result<Output, String>> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Result<Output, String>>>> =
        Mutex::new((0..candidates.len()).map(|_| None).collect());
    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(candidates.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(candidate) = candidates.get(index) else {
                        break;
                    };
                    let result = generate_one(vfs, candidate, settings);
                    results.lock().expect("results lock")[index] = Some(result);
                }
            });
        }
    });
    results
        .into_inner()
        .expect("results lock")
        .into_iter()
        .map(|result| result.expect("every candidate ran"))
        .collect()
}

fn generate_one(
    vfs: &VirtualFileSystem,
    candidate: &Candidate,
    settings: &Settings,
) -> Result<Output, String> {
    let asset = vfs
        .read(&candidate.image)
        .map_err(|error| error.to_string())?
        .ok_or("image disappeared")?;
    let source = decode(&candidate.image, &asset.bytes)?;
    let maps = generate(&source, candidate.class, candidate.alpha_tested, settings);
    let mut entries = Vec::new();
    if maps.flat {
        return Ok(Output {
            archive: archive_name(&asset.source.mount_name),
            width: maps.packed.width(),
            height: maps.packed.height(),
            entries,
        });
    }
    if candidate.normal {
        let bytes = if maps.normal_alpha {
            png_rgba(&maps.normal)
        } else {
            png_rgb(&image::DynamicImage::ImageRgba8(maps.normal.clone()).to_rgb8())
        }
        .map_err(|error| error.to_string())?;
        entries.push(Entry {
            path: format!("{}{}.png", candidate.base, maps.normal_kind.suffix()),
            bytes,
        });
    }
    if candidate.packed {
        entries.push(Entry {
            path: format!("{}{PACKED_SUFFIX}.png", candidate.base),
            bytes: png_rgb(&maps.packed).map_err(|error| error.to_string())?,
        });
    }
    Ok(Output {
        archive: archive_name(&asset.source.mount_name),
        width: maps.packed.width(),
        height: maps.packed.height(),
        entries,
    })
}

/// Decode by extension, as the client's loader does.
fn decode(path: &str, bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let format = match path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) {
        Some(extension) if extension == "tga" => Some(image::ImageFormat::Tga),
        Some(extension) if extension == "jpg" || extension == "jpeg" => {
            Some(image::ImageFormat::Jpeg)
        }
        Some(extension) if extension == "png" => Some(image::ImageFormat::Png),
        _ => None,
    };
    let image = match format {
        Some(format) => image::load_from_memory_with_format(bytes, format),
        None => image::load_from_memory(bytes),
    }
    .map_err(|error| error.to_string())?;
    Ok(image.to_rgba8())
}

fn archive_name(mount: &str) -> String {
    Path::new(mount).file_name().map_or_else(
        || mount.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// One line per candidate for `--dry-run` and verbose listings.
pub fn describe_candidate(candidate: &Candidate) -> String {
    let class: &MaterialClass = candidate.class;
    let mut outputs = Vec::new();
    if candidate.normal {
        outputs.push(if class.parallax && !candidate.alpha_tested {
            "_nh"
        } else {
            "_n"
        });
    }
    if candidate.packed {
        outputs.push(PACKED_SUFFIX);
    }
    format!(
        "{:>7} tris  {:<11} {:<24} {:<10} {}",
        candidate.triangles,
        class.name,
        candidate.class_source.describe(),
        outputs.join(","),
        candidate.image
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_names_normalize() {
        assert_eq!(normalize_map("mp/ffa3"), "mp/ffa3");
        assert_eq!(normalize_map("maps/MP/FFA3.bsp"), "mp/ffa3");
        assert_eq!(normalize_map("mp\\duel1.bsp"), "mp/duel1");
    }

    #[test]
    fn decoding_follows_the_extension() {
        let png =
            crate::package::png_rgb(&image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3])))
                .expect("png");
        assert_eq!(
            decode("a/b.png", &png).expect("decodes").get_pixel(0, 0).0,
            [1, 2, 3, 255]
        );
        assert!(decode("a/b.tga", &png).is_err());
    }
}

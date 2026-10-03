//! Effect texture atlas construction and expansion when new EFX shaders arrive.

use super::*;

/// The shared bind-group layout used by effect rendering and atlas expansion.
pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("JKR effect texture atlas layout"),
        entries: &[
            texture_layout_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Build all requested shader tiles into the effect texture atlas.
pub(crate) fn create(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    required_effect_shaders: &BTreeSet<String>,
) -> Result<ParticleAtlas, Box<dyn Error>> {
    const TILE: u32 = 128;
    const EFFECT_SHADERS: &[&str] = &[
        player_shadows::SHADER,
        "gfx/misc/spark",
        "gfx/misc/spark2",
        "gfx/misc/steam",
        "gfx/misc/steam2",
        "gfx/effects/whiteflare",
        "gfx/effects/blaster_blob",
        "gfx/effects/blasterfrontflash",
        "gfx/effects/blastersideflash",
        "gfx/exp/explosion1",
        "gfx/exp/slower_rocket_explosion",
        "gfx/exp/rocket_explosion",
        "gfx/misc/dotfill_a",
        "gfx/effects/whiteflash",
        "gfx/effects/light_cone",
        "gfx/misc/smoke",
        "gfx/misc/black_smoke",
    ];
    struct PendingAnimation {
        shader: String,
        paths: Vec<jkr_vfs::VirtualPath>,
        frequency: f32,
        one_shot: bool,
        blend: ParticleBlend,
        rgb_wave: Option<WaveForm>,
        alpha_wave: Option<WaveForm>,
        tc_scale: [f32; 2],
        tc_scroll: [f32; 2],
    }
    let requested_shaders = EFFECT_SHADERS
        .iter()
        .map(|shader| (*shader).to_owned())
        .chain(required_effect_shaders.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut pending = Vec::new();
    for shader in &requested_shaders {
        let before = pending.len();
        if let Some(definition) = shaders.get(shader) {
            for stage in definition
                .stages
                .iter()
                .filter(|stage| !stage.images.is_empty())
            {
                let mut paths = Vec::new();
                for name in &stage.images {
                    if name.starts_with('$') || name == "-" {
                        continue;
                    }
                    if let Some(path) = shaders.resolve_image(vfs, name)?
                        && !paths.contains(&path)
                    {
                        paths.push(path);
                    }
                }
                if !paths.is_empty() {
                    let (tc_scale, tc_scroll) =
                        effect_texcoords::compile(&stage.texture_modifications);
                    let blend = effect_runtime::particle_blend_for_stage(Some(&stage.blend));
                    if blend == ParticleBlend::Unsupported {
                        eprintln!(
                            "unsupported effect blend falls back to alpha: {shader} {:?}",
                            stage.blend
                        );
                    }
                    pending.push(PendingAnimation {
                        shader: shader.to_ascii_lowercase(),
                        paths,
                        frequency: stage.animation_frequency.unwrap_or(0.0),
                        one_shot: stage.one_shot,
                        blend,
                        rgb_wave: stage.rgb_wave.clone(),
                        alpha_wave: stage.alpha_wave.clone(),
                        tc_scale,
                        tc_scroll,
                    });
                }
            }
        }
        if pending.len() == before
            && let Some(path) = shaders.resolve_image(vfs, shader)?
        {
            pending.push(PendingAnimation {
                shader: shader.to_ascii_lowercase(),
                paths: vec![path],
                frequency: 0.0,
                one_shot: false,
                blend: ParticleBlend::Alpha,
                rgb_wave: None,
                alpha_wave: None,
                tc_scale: [1.0; 2],
                tc_scroll: [0.0; 2],
            });
        }
    }
    let tile_count = pending
        .iter()
        .map(|animation| animation.paths.len())
        .sum::<usize>()
        .max(1);
    let columns = (tile_count as f32).sqrt().ceil() as u32;
    let rows = u32::try_from(tile_count)?.div_ceil(columns);
    let width = TILE * columns;
    let height = TILE * rows;
    let mut atlas = image::RgbaImage::new(width, height);
    let mut animations = HashMap::new();
    let mut tile_index = 0_u32;
    for animation in pending {
        let mut frames = Vec::new();
        for path in animation.paths {
            let Some(asset) = vfs.read(path.as_str())? else {
                continue;
            };
            let Ok(decoded) = decode_image(&asset.bytes, path.as_str()) else {
                continue;
            };
            let resized = image::imageops::resize(
                &decoded.into_rgba8(),
                TILE,
                TILE,
                image::imageops::FilterType::Triangle,
            );
            let tile_x = tile_index % columns;
            let tile_y = tile_index / columns;
            image::imageops::overlay(
                &mut atlas,
                &resized,
                i64::from(tile_x * TILE),
                i64::from(tile_y * TILE),
            );
            let inset = 0.5;
            frames.push([
                (tile_x as f32 * TILE as f32 + inset) / width as f32,
                (tile_y as f32 * TILE as f32 + inset) / height as f32,
                ((tile_x + 1) as f32 * TILE as f32 - inset) / width as f32,
                ((tile_y + 1) as f32 * TILE as f32 - inset) / height as f32,
            ]);
            tile_index += 1;
        }
        if !frames.is_empty() {
            animations
                .entry(animation.shader)
                .or_insert_with(Vec::new)
                .push(ParticleAtlasAnimation {
                    frames,
                    frequency: animation.frequency,
                    one_shot: animation.one_shot,
                    blend: animation.blend,
                    rgb_wave: animation.rgb_wave,
                    alpha_wave: animation.alpha_wave,
                    tc_scale: animation.tc_scale,
                    tc_scroll: animation.tc_scroll,
                });
        }
    }
    let fallback = animations
        .get("gfx/misc/spark")
        .and_then(|animations| animations.first())
        .and_then(|animation| animation.frames.first())
        .copied()
        .unwrap_or([0.0, 0.0, 1.0 / columns as f32, 1.0 / rows as f32]);
    let texture = create_rgba8_texture(
        device,
        queue,
        "JKR retail effect texture atlas",
        width,
        height,
        atlas.as_raw(),
        false,
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("JKR effect texture sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("JKR effect texture atlas bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    Ok(ParticleAtlas {
        bind_group,
        animations,
        fallback,
    })
}

impl GpuState {
    /// Retain existing shader names when new graphs require additional tiles.
    pub(crate) fn refresh_effect_atlas(&mut self) -> Result<(), Box<dyn Error>> {
        let mut required = BTreeSet::new();
        self.effects.append_shader_paths(&mut required);
        if required
            .iter()
            .all(|name| self.particle_atlas.animations.contains_key(name))
        {
            return Ok(());
        }
        required.extend(self.particle_atlas.animations.keys().cloned());
        let vfs = self.vfs.as_ref().ok_or("no VFS")?;
        self.particle_atlas = create(
            &self.device,
            &self.queue,
            &layout(&self.device),
            vfs,
            &self.shaders,
            &required,
        )?;
        Ok(())
    }
}

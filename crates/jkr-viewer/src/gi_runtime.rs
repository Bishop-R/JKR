//! Map-lifetime GPU residency of the voxelised world and its surface table. Installed
//! only for the real-time lighting mode; the 2004 baked mode allocates nothing here.
use wgpu::util::DeviceExt;

/// Header shared by every GI shader: grid origin/dimensions and cell sizes.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Header {
    pub(crate) origin: [f32; 4],
    pub(crate) fine: [u32; 4],
    pub(crate) coarse: [u32; 4],
    /// Fine cell size, coarse cell size, surface count, unused.
    pub(crate) sizes: [f32; 4],
}

/// Immutable voxel buffers (kept alive by the bind group) plus the surface table.
pub(crate) struct Runtime {
    pub(crate) fixtures: Option<super::lamp_geometry::Runtime>,

    /// CPU copy, kept for dead-probe tests at probe installation.
    pub(crate) world: crate::gi_voxels::VoxelWorld,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl Runtime {
    /// Upload a built voxel world and surface table.
    pub(crate) fn new(
        device: &wgpu::Device,
        world: crate::gi_voxels::VoxelWorld,
        surfaces: &[crate::gi_voxels::Surface],
    ) -> Self {
        let header = Header {
            origin: world.origin.extend(0.).to_array(),
            fine: [world.fine[0], world.fine[1], world.fine[2], 0],
            coarse: [world.coarse[0], world.coarse[1], world.coarse[2], 0],
            sizes: [
                world.fine_size,
                world.fine_size * crate::gi_voxels::COARSE_FACTOR as f32,
                surfaces.len() as f32,
                0.,
            ],
        };
        let storage = |label, contents: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let header_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR GI header"),
            contents: bytemuck::bytes_of(&header),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let occupancy = storage(
            "JKR GI occupancy bits",
            bytemuck::cast_slice(&world.occupancy),
        );
        // u16 material ids packed in pairs; the shader unpacks by parity.
        let packed: Vec<u32> = world
            .materials
            .chunks(2)
            .map(|pair| u32::from(pair[0]) | (u32::from(*pair.get(1).unwrap_or(&0)) << 16))
            .collect();
        let materials = storage("JKR GI material grid", bytemuck::cast_slice(&packed));
        let table: Vec<[f32; 8]> = surfaces
            .iter()
            .map(|s| {
                [
                    s.albedo[0],
                    s.albedo[1],
                    s.albedo[2],
                    f32::from(u8::from(s.sky)),
                    s.emission[0],
                    s.emission[1],
                    s.emission[2],
                    0.,
                ]
            })
            .collect();
        let table = if table.is_empty() {
            vec![[0.; 8]]
        } else {
            table
        };
        let surfaces_buffer = storage("JKR GI surface table", bytemuck::cast_slice(&table));
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let read_only = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR GI voxel world"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, read_only),
                entry(2, read_only),
                entry(3, read_only),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR GI voxel world"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: header_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: occupancy.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: materials.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: surfaces_buffer.as_entire_binding(),
                },
            ],
        });
        Self {
            world,
            layout,
            bind_group,
            fixtures: None,
        }
    }
}

impl super::Runtime {
    /// Voxelise the world draws once and keep them resident; a no-op unless enabled.
    pub(crate) fn install_gi(
        &mut self,
        device: &wgpu::Device,
        flat: &crate::scene_flatten::FlattenedScene,
        enabled: bool,
    ) {
        self.gi = None;
        if !enabled {
            return;
        }
        let started = std::time::Instant::now();
        let world = self.voxelise(flat);
        let elapsed = started.elapsed();
        crate::log::progress(format_args!(
            "GI voxels: {}x{}x{} fine ({} units) / {}x{}x{} coarse, {} triangles, {:.1} MB, \
             {:.1}% occupied, {} emissive materials, built in {:.0} ms",
            world.fine[0],
            world.fine[1],
            world.fine[2],
            world.fine_size,
            world.coarse[0],
            world.coarse[1],
            world.coarse[2],
            world.triangles,
            world.bytes() as f64 / 1e6,
            world.occupied_fraction() * 100.,
            self.surfaces_by_source
                .iter()
                .filter(|s| s.emission.iter().any(|&e| e > 0.))
                .count(),
            elapsed.as_secs_f64() * 1e3
        ));
        let mut runtime = Runtime::new(device, world, &self.surfaces_by_source);
        let triangles: Vec<_> = self
            .gi_draws(flat)
            .flat_map(|(range, material, _, sky)| {
                flat.indices[range.start as usize..range.end as usize]
                    .chunks_exact(3)
                    .map(move |t| super::lamp_geometry::geometry::Triangle {
                        points: [t[0], t[1], t[2]]
                            .map(|i| glam::Vec3::from_array(flat.vertices[i as usize].position)),
                        material,
                        sky,
                    })
            })
            .collect();
        runtime.fixtures = Some(super::lamp_geometry::Runtime::new(
            device,
            &triangles,
            &self.surfaces_by_source,
        ));
        self.gi = Some(runtime);
    }

    /// Build the voxel world from opaque static world draws with their source materials.
    pub(crate) fn voxelise(
        &self,
        flat: &crate::scene_flatten::FlattenedScene,
    ) -> crate::gi_voxels::VoxelWorld {
        let draws = flat
            .draws
            .iter()
            .filter(|draw| draw.world_surface)
            .filter_map(|draw| {
                let runtime = *self.source_to_runtime.get(draw.material)?;
                let material = self.materials.get(runtime)?;
                // Only true opaque casters: alpha-tested railings and grates would otherwise
                // become solid voxel walls that stop probe rays and column traces.
                if material.blended
                    || material.flare
                    || !material
                        .stages
                        .first()
                        .is_some_and(|stage| stage.shadow_caster)
                {
                    return None;
                }
                let emissive = self
                    .surfaces_by_source
                    .get(draw.material)
                    .is_some_and(|s| s.emission.iter().any(|&e| e > 0.));
                Some((&draw.indices, u16::try_from(draw.material).ok()?, emissive))
            });
        crate::gi_voxels::VoxelWorld::build(
            &flat.vertices,
            &flat.indices,
            draws,
            self.shadow_bounds,
        )
    }
    fn gi_draws<'a>(
        &'a self,
        flat: &'a crate::scene_flatten::FlattenedScene,
    ) -> impl Iterator<Item = (&'a std::ops::Range<u32>, u32, bool, bool)> + 'a {
        flat.draws
            .iter()
            .filter(|draw| draw.world_surface)
            .filter_map(move |draw| {
                let runtime = *self.source_to_runtime.get(draw.material)?;
                let material = self.materials.get(runtime)?;
                // Coverage masks are not yet in the static tracer. Keep their holes open
                // instead of turning an alpha-tested railing into an opaque barrier.
                let sky = self
                    .surfaces_by_source
                    .get(draw.material)
                    .is_some_and(|s| s.sky);
                if !sky
                    && (material.blended
                        || material.flare
                        || !material
                            .stages
                            .first()
                            .is_some_and(|stage| stage.shadow_caster))
                {
                    return None;
                }
                let emissive = self
                    .surfaces_by_source
                    .get(draw.material)
                    .is_some_and(|s| s.emission.iter().any(|&e| e > 0.));
                Some((
                    &draw.indices,
                    u32::try_from(draw.material).ok()?,
                    emissive,
                    sky,
                ))
            })
    }
}

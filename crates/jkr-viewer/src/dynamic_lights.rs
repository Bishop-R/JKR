//! Fixed-capacity generic point lights for the world material pass.
//!
//! The capacity matches rd-vanilla `MAX_DLIGHTS == 32`
//! (`codemp/rd-common/tr_types.h:29`), whose surface masks cannot represent
//! more lights. This module remains renderer-generic; legacy weapon selection
//! stays in `jkr-client`.

use bytemuck::{Pod, Zeroable};

#[path = "dynamic_light_settings.rs"]
mod settings;
pub(crate) use settings::Settings;

#[path = "point_light_grid.rs"]
mod grid;

#[path = "entity_lights.rs"]
/// Legacy entity-source selection feeding the existing generic light list.
pub(crate) mod entities;

/// Maximum point lights submitted in one frame.
pub(crate) const MAX_POINT_LIGHTS: usize = 32;

/// One generic RGB point light with a finite radius.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PointLight {
    pub(crate) origin: [f32; 3],
    pub(crate) radius: f32,
    pub(crate) color: [f32; 3],
}

/// Allocation-free per-frame light list.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointLightList {
    lights: [PointLight; MAX_POINT_LIGHTS],
    len: usize,
    dropped: usize,
    radiant: u32,
}

impl Default for PointLightList {
    fn default() -> Self {
        Self {
            lights: [PointLight::default(); MAX_POINT_LIGHTS],
            len: 0,
            dropped: 0,
            radiant: 0,
        }
    }
}

impl PointLightList {
    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
        self.radiant = 0;
    }

    pub(crate) fn push(&mut self, light: PointLight) -> bool {
        if light.radius <= 0.0 {
            return false;
        }
        let Some(slot) = self.lights.get_mut(self.len) else {
            self.dropped += 1;
            return false;
        };
        *slot = light;
        self.len += 1;
        true
    }

    /// Add emitted illumination independently of the existing surface light in day mode.
    pub(crate) fn push_radiant(&mut self, light: PointLight) -> bool {
        let index = self.len;
        if !self.push(light) {
            return false;
        }
        self.radiant |= 1 << index;
        true
    }

    pub(crate) fn as_slice(&self) -> &[PointLight] {
        &self.lights[..self.len]
    }

    pub(crate) fn gpu_block(&self) -> GpuPointLightBlock {
        let mut block = GpuPointLightBlock::zeroed();
        block.metadata[0] = self.len as u32;
        block.metadata[3] = self.radiant;
        block.grid = grid::build(self.as_slice());
        for (index, (destination, source)) in block
            .lights
            .iter_mut()
            .zip(&self.lights[..self.len])
            .enumerate()
        {
            destination.origin_radius = [
                source.origin[0],
                source.origin[1],
                source.origin[2],
                source.radius,
            ];
            destination.color = [
                source.color[0],
                source.color[1],
                source.color[2],
                f32::from(self.radiant & (1 << index) != 0),
            ];
        }
        block
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuPointLight {
    origin_radius: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuPointLightBlock {
    lights: [GpuPointLight; MAX_POINT_LIGHTS],
    /// Light count, fragment-diffuse enable, lighting-mode bits, then a mask of emitted-light sources.
    pub(crate) metadata: [u32; 4],
    grid: grid::Grid,
}

pub(crate) fn empty_gpu_block() -> GpuPointLightBlock {
    GpuPointLightBlock::zeroed()
}

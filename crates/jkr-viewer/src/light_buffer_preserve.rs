//! Retain the main light buffer around mirror work instead of shading the main view twice.
use super::*;

pub(super) struct Saved {
    textures: [wgpu::Texture; 4],
}
impl LightBuffer {
    pub(in crate::world_materials) fn preservation_enabled(&self) -> bool {
        self.saved.is_some()
    }

    pub(in crate::world_materials) fn configure_preservation(
        &mut self,
        device: &wgpu::Device,
        enabled: bool,
    ) {
        self.saved = enabled.then(|| Saved {
            textures: self.views().map(|view| {
                let source = view.texture();
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("JKR preserved main lighting"),
                    size: source.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: source.format(),
                    usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
            }),
        });
    }

    fn views(&self) -> [&wgpu::TextureView; 4] {
        [&self.color, &self.depth, &self.normal, &self.occlusion]
    }

    fn copy_preserved(&self, encoder: &mut wgpu::CommandEncoder, restore: bool) -> bool {
        let Some(saved) = &self.saved else {
            return false;
        };
        for (view, copy) in self.views().into_iter().zip(&saved.textures) {
            let live = view.texture();
            let (source, target) = if restore { (copy, live) } else { (live, copy) };
            encoder.copy_texture_to_texture(
                source.as_image_copy(),
                target.as_image_copy(),
                live.size(),
            );
        }
        true
    }
}

impl super::super::super::Runtime {
    /// Allocate at map installation only when automatic floor reflections can use it.
    pub(crate) fn configure_light_preservation(&mut self, device: &wgpu::Device, enabled: bool) {
        if let Some(light) = self.shadows.as_mut().and_then(|s| s.light.as_mut()) {
            light.configure_preservation(device, enabled);
        }
    }

    /// Save/restore all four attachments using GPU copies; no frame allocations or readback.
    pub(crate) fn copy_preserved_light(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        restore: bool,
    ) -> bool {
        self.shadows
            .as_ref()
            .and_then(|s| s.light.as_ref())
            .is_some_and(|light| light.copy_preserved(encoder, restore))
    }
}

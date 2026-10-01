//! Acquire the window swapchain image for a frame.

use crate::GpuState;
use crate::gpu_context::FrameStatus;

#[path = "post_aa.rs"]
/// Optional scene-only post-process, independent of depth and simulation.
pub(crate) mod aa;

#[path = "render_scale.rs"]
/// Single-sample scene supersampling, resolved before postprocessing and HUD.
pub(crate) mod scale;

/// Acquire this frame's colour target. `Err` is the status to report
/// instead of rendering (the surface needs reconfiguring, or the frame is
/// skipped).
pub(crate) fn acquire(
    gpu: &GpuState,
) -> Result<(Option<wgpu::SurfaceTexture>, wgpu::TextureView), FrameStatus> {
    let frame = if let Some(surface) = &gpu.context.surface {
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return Err(FrameStatus::Reconfigure);
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return Err(FrameStatus::Skip),
        }
    } else {
        return Err(FrameStatus::Skip);
    };
    let view = frame
        .as_ref()
        .expect("surface acquired")
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    Ok((frame, view))
}

/// How the world pass starts on its colour target: cleared to the night
/// tint, or kept when a portal world was already drawn underneath.
pub(crate) fn world_load(portal: crate::portal::View) -> wgpu::LoadOp<wgpu::Color> {
    if portal != crate::portal::View::Absent {
        wgpu::LoadOp::Load
    } else {
        wgpu::LoadOp::Clear(wgpu::Color {
            r: 0.015,
            g: 0.02,
            b: 0.035,
            a: 1.0,
        })
    }
}

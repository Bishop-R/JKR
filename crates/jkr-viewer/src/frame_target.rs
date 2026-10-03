//! Acquire the window swapchain image for a frame.

use crate::GpuState;
use crate::gpu_context::FrameStatus;

#[path = "post_aa.rs"]
/// Optional scene-only post-process, independent of depth and simulation.
pub(crate) mod aa;

#[path = "render_scale.rs"]
/// Single-sample scene supersampling, resolved before postprocessing and HUD.
pub(crate) mod scale;

type Output = (Option<wgpu::SurfaceTexture>, wgpu::TextureView);

/// Scene target and any swapchain image acquired before recording the world.
pub(crate) struct Prepared {
    pub(crate) scene: wgpu::TextureView,
    early: Option<Output>,
}

/// Defer swapchain acquisition when the scene already has an offscreen target.
pub(crate) fn prepare(gpu: &mut GpuState) -> Result<Prepared, FrameStatus> {
    if let Some(scene) = gpu.offscreen_scene_target().cloned() {
        Ok(Prepared { scene, early: None })
    } else {
        let early = gpu.acquire_output()?;
        Ok(Prepared {
            scene: early.1.clone(),
            early: Some(early),
        })
    }
}

impl Prepared {
    /// Acquire the presentation target after world recording. On surface failure,
    /// submit the recorded work without presenting so encoder workers can finish.
    pub(crate) fn finish(
        self,
        gpu: &mut GpuState,
        encoder: wgpu::CommandEncoder,
        timing: &mut crate::frame_pacing::budget::Timer,
    ) -> Result<
        (
            Option<wgpu::SurfaceTexture>,
            wgpu::TextureView,
            wgpu::CommandEncoder,
        ),
        FrameStatus,
    > {
        use crate::frame_pacing::budget::Phase;
        let output = match self.early {
            Some(output) => output,
            None => {
                timing.mark(Phase::Acquire);
                match gpu.acquire_output() {
                    Ok(output) => output,
                    Err(status) => {
                        timing.mark(Phase::Submit);
                        gpu.frame_pacer
                            .split
                            .submit(&gpu.queue, encoder, None, timing);
                        return Err(status);
                    }
                }
            }
        };
        Ok((output.0, output.1, encoder))
    }
}

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

impl GpuState {
    /// Wait until the previous frame is submitted and presented, map its readbacks, then
    /// acquire this frame's swapchain image.
    fn acquire_output(
        &mut self,
    ) -> Result<(Option<wgpu::SurfaceTexture>, wgpu::TextureView), FrameStatus> {
        self.frame_pacer.split.wait_previous();
        if let Some(phases) = &self.gpu_phases {
            phases.after_submit();
            phases.report();
        }
        self.screenshots.after_submit();
        acquire(self)
    }
}

//! Off-screen evidence for the views the scope pass draws, no window needed: with
//! `JKR_GAME_DATA` set, the binoculars' view over `JKR_EVIDENCE_BACKGROUND` (any picture;
//! a grey ramp without one), written to `JKR_EVIDENCE_OUT` (default
//! `target/parity-reports/binoculars.png`).

use super::*;

const SIZE: [u32; 2] = [1280, 960];

#[test]
#[ignore = "needs JKR_GAME_DATA and a GPU"]
fn binocular_view_off_screen() {
    let Some(game_data) = std::env::var_os("JKR_GAME_DATA") else {
        return;
    };
    let context = pollster::block_on(crate::gpu_context::Context::new(None, None)).expect("GPU");
    let device = &context.device;
    let vfs = crate::assets::mount_game_data(std::path::Path::new(&game_data)).expect("data");
    let shaders = crate::assets::load_shaders(&vfs);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut mask =
        Mask::new(device, &context.queue, format, &vfs, &shaders).expect("scope pictures");
    assert!(mask.binoculars.is_some(), "binocular pictures");
    let [width, height] = SIZE;
    let background = std::env::var_os("JKR_EVIDENCE_BACKGROUND")
        .and_then(|path| image::open(path).ok())
        .map(|picture| {
            picture
                .resize_exact(width, height, image::imageops::FilterType::Triangle)
                .into_rgba8()
        })
        .unwrap_or_else(|| {
            image::RgbaImage::from_fn(width, height, |x, y| {
                let value = (60 + (x + y) * 120 / (width + height)) as u8;
                image::Rgba([value, value, value, 255])
            })
        });
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("binocular evidence"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue.raw().write_texture(
        target.as_image_copy(),
        background.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        extent,
    );
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("binocular evidence depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::DepthTarget::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    mask.prepare_binoculars(
        &context.queue,
        BinocularView {
            time: 12_345,
            yaw: 37.0,
            zoom_fov: 40.0,
        },
    );
    let row = (width * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("binocular evidence readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let view = target.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("binocular evidence"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        mask.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        extent,
    );
    context.queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |mapped| {
            let _ = sender.send(mapped);
        });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    receiver.recv().expect("map").expect("mapped");
    let mapped = readback.slice(..).get_mapped_range().expect("mapped range");
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in mapped.chunks(row as usize).take(height as usize) {
        pixels.extend_from_slice(&line[..(width * 4) as usize]);
    }
    let picture = image::RgbaImage::from_raw(width, height, pixels).expect("picture");
    let out = std::env::var_os("JKR_EVIDENCE_OUT").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/parity-reports/binoculars.png")
        },
        std::path::PathBuf::from,
    );
    picture.save(&out).expect("save");
    // The mask darkens the edges and the strip behind the numbers is black.
    let corner = picture.get_pixel(4, 4).0;
    let strip = picture.get_pixel(width * 300 / 640, height * 405 / 480).0;
    assert!(
        corner[..3].iter().all(|channel| *channel < 40),
        "{corner:?}"
    );
    assert!(strip[..3].iter().all(|channel| *channel < 40), "{strip:?}");
}

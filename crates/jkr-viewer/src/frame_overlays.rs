//! Depth-sampling world accents precede HUD/text in a read-only depth pass.
use crate::GpuState;

impl GpuState {
    /// Translate this frame's 2D layers into the shared shape batch: the HUD's part a zoom
    /// mask covers first, then the HUD, chat, scores, menus and console. `shown` is whether
    /// the HUD, the chat and the scores are.
    pub(crate) fn prepare_ui_shapes(
        &mut self,
        viewport: [f32; 2],
        [information, chat, scores]: [bool; 3],
        console_covers_frame: bool,
    ) {
        let layers = [
            information.then(|| &self.hud.identification.list),
            information.then(|| self.hud.draw_list()),
            chat.then(|| self.chat.draw_list()),
            scores.then(|| self.scoreboard.draw_list()),
            (self.game_menu && !console_covers_frame).then(|| self.in_game_menu.draw_list()),
            self.client_menu
                .as_ref()
                .filter(|_| !console_covers_frame)
                .and_then(|menu| menu.draw_list()),
            self.console.as_ref().map(|console| console.draw_list()),
        ];
        let under = information.then(|| self.hud.under_scope_list());
        self.ui_shapes
            .prepare_layers(&self.queue, under, layers.into_iter().flatten(), viewport);
    }

    /// Filter the completed scene after flares, then draw untouched HUD/text into final color.
    pub(crate) fn draw_frame_overlays(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        output: &wgpu::TextureView,
        cluster: Option<usize>,
        visibility: Option<&jkr_bsp::Visibility>,
        text: u32,
        classic_text: u32,
    ) {
        let hud = self.live_session.is_some()
            || self.demo_session.is_some()
            || self.console.as_ref().is_some_and(|c| c.is_open())
            || self.client_menu.as_ref().is_some_and(|m| m.is_visible());
        let hyperspace = self.local_prediction.hyperspace_shade();
        let flares = hyperspace.is_none()
            && self.world_materials.has_flares()
            && self.geometry.environment.data.control[3] != 0.;
        if !hud && !flares && self.post_aa.is_none() && self.render_scale.is_none() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("JKR flares and overlays"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // cg_view.c:1647-1649 / RB_Hyperspace: replace the world,
                    // but retain the ordinary HUD pass over the grey flash.
                    load: hyperspace.map_or(wgpu::LoadOp::Load, |c| {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: c,
                            g: c,
                            b: c,
                            a: 1.0,
                        })
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth.view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if flares {
            self.world_materials.draw_flares(
                &mut pass,
                &crate::world_materials::FrameDraw {
                    camera: &self.camera_bind_group,
                    vertices: &self.geometry.vertex_buffer,
                    indices: &self.geometry.index_buffer,
                    instances: &self.actor_instance_buffer,
                    mover_ranges: &self.mover_instance_ranges,
                    source_cluster: cluster,
                    visibility,
                    entities: &[],
                },
                &self.depth.sample_bind_group,
            );
        }
        // In-world, so before FXAA and the unfiltered HUD layer.
        self.ground_hud.draw(
            &mut pass,
            &self.depth.sample_bind_group,
            &self.text_bind_group,
        );
        if self.post_aa.is_some() || self.render_scale.is_some() {
            drop(pass);

            if let Some(scale) = &self.render_scale {
                scale.draw(
                    encoder,
                    self.post_aa.as_ref().map_or(output, |aa| &aa.scene),
                );
            }
            if let Some(aa) = &self.post_aa {
                aa.draw_scene(encoder, output);
            }

            if !hud {
                if let Some(aa) = &self.post_aa {
                    aa.draw_display(encoder, output);
                }
                return;
            }
            pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("JKR unfiltered HUD after FXAA"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self
                        .post_aa
                        .as_ref()
                        .map_or(output, |aa| aa.hud_target(output)),
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self
                        .render_scale
                        .as_ref()
                        .map_or(&self.depth.view, |scale| &scale.hud_depth.view),
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        if !hud {
            return;
        }
        pass.set_pipeline(&self.hud_pipeline);
        pass.set_bind_group(0, &self.hud_bind_group, &[]);
        match self.hud_scissors {
            None => pass.draw(0..3, 0..1),
            Some(rectangles) => {
                for [x, y, width, height] in rectangles.into_iter().flatten() {
                    pass.set_scissor_rect(x, y, width, height);
                    pass.draw(0..3, 0..1);
                }
                pass.set_scissor_rect(0, 0, self.configuration.width, self.configuration.height);
            }
        }
        // `CG_Draw2D` draws its screen tints and the rocket lock, then the zoom mask, then
        // the rest of the 2D screen over it.
        self.ui_shapes.draw_under(&mut pass);
        if let Some(mask) = &self.scope_mask {
            mask.draw(&mut pass);
        }
        self.ui_shapes.draw_over(&mut pass);
        if text != 0 {
            pass.set_pipeline(&self.text_pipeline);
            pass.set_bind_group(0, &self.text_bind_group, &[]);
            pass.set_vertex_buffer(0, self.text_vertex_buffer.slice(..));
            pass.draw(0..text, 0..1);
        }
        if classic_text != 0
            && let Some(group) = &self.classic_text_bind_group
        {
            pass.set_pipeline(&self.text_pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, self.classic_text_vertex_buffer.slice(..));
            pass.draw(0..classic_text, 0..1);
        }
        drop(pass);

        if let Some(aa) = &self.post_aa {
            aa.draw_display(encoder, output);
        }
    }
}

//! Shared CPU fallback quad tessellation; GPU expansion leaves this arithmetic unchanged.
use super::*;

impl Mesh {
    /// Append a four-corner cylinder stamp with the existing two-triangle winding.
    pub(crate) fn quad(
        &mut self,
        points: [(Vec3, [f32; 2]); 4],
        uv: [f32; 4],
        uv_transform: [f32; 4],
        color: [f32; 4],
        depth_hack: bool,
    ) {
        if self.expanded_quad(points, uv, uv_transform, color, depth_hack) {
            return;
        }
        if self.vertices.len() + 4 > MAX_VERTICES || self.indices.len() + 6 > MAX_INDICES {
            self.dropped += 1;
            return;
        }
        let base = self.vertices.len() as u32;
        self.vertices
            .extend(points.map(|(position, local_uv)| Vertex {
                position: position.to_array(),
                local_uv,
                uv_rect: uv,
                uv_transform,
                color,
                depth_hack: f32::from(depth_hack),
            }));
        self.indices
            .extend([base, base + 1, base + 2, base + 2, base + 3, base]);
    }

    /// Append a widened lightning strip with its distinct reference triangle winding.
    pub(crate) fn line_quad(
        &mut self,
        points: [(Vec3, [f32; 2]); 4],
        uv: [f32; 4],
        uv_transform: [f32; 4],
        color: [f32; 4],
        depth_hack: bool,
    ) {
        // Strip order is 0,1,2 / 2,1,3. Cyclically rotate those triangles
        // into the polygon expander's order without changing their winding
        // or diagonal. CPU-only vertices are not uploaded in expansion mode.
        if self.expanded_quad(
            [points[2], points[0], points[1], points[3]],
            uv,
            uv_transform,
            color,
            depth_hack,
        ) {
            return;
        }
        if self.vertices.len() + 4 > MAX_VERTICES || self.indices.len() + 6 > MAX_INDICES {
            self.dropped += 1;
            return;
        }
        let base = self.vertices.len() as u32;
        self.vertices
            .extend(points.map(|(position, local_uv)| Vertex {
                position: position.to_array(),
                local_uv,
                uv_rect: uv,
                uv_transform,
                color,
                depth_hack: f32::from(depth_hack),
            }));
        self.indices
            .extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
    }
}

//! Map-lifetime triangle visibility for lighting. No raster resolution or map-size bias.
use glam::Vec3;

/// A static visibility triangle and its renderer material, independent of file format.
#[derive(Clone, Copy)]
pub(crate) struct Triangle {
    pub(crate) points: [Vec3; 3],
    pub(crate) material: u32,
    pub(crate) sky: bool,
}

/// Linear BVH with four-triangle leaves, packed for the portable GPU tracer.
pub(crate) struct Geometry {
    /// Two vec4 words per node: lower/first-child, upper/leaf-count.
    pub(crate) nodes: Vec<[u32; 4]>,
    /// Three vec4 words per triangle: origin/material, edge/sky, edge/padding.
    pub(crate) triangles: Vec<[u32; 4]>,
}

fn word(p: Vec3, tag: u32) -> [u32; 4] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits(), tag]
}

impl Geometry {
    /// Build a balanced median tree; every shader traversal fits its 32-entry stack.
    pub(crate) fn new(triangles: &[Triangle]) -> Self {
        let mut ordered: Vec<_> = triangles
            .iter()
            .copied()
            .filter(|t| {
                t.points.iter().all(|p| p.is_finite())
                    && (t.points[1] - t.points[0])
                        .cross(t.points[2] - t.points[0])
                        .length_squared()
                        > 1e-8
            })
            .collect();
        let mut nodes = vec![[0; 4]; 2];
        if !ordered.is_empty() {
            build(&mut ordered, 0, 0, &mut nodes);
        }
        let triangles = ordered
            .iter()
            .flat_map(|t| {
                [
                    word(t.points[0], t.material),
                    word(t.points[1] - t.points[0], u32::from(t.sky)),
                    word(t.points[2] - t.points[0], 0),
                ]
            })
            .collect();
        Self { nodes, triangles }
    }
}

fn build(triangles: &mut [Triangle], offset: usize, node: usize, nodes: &mut Vec<[u32; 4]>) {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for t in triangles.iter() {
        for &p in &t.points {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if triangles.len() <= 4 {
        nodes[node * 2] = word(lo, offset as u32);
        nodes[node * 2 + 1] = word(hi, triangles.len() as u32);
        return;
    }
    let extent = hi - lo;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    let middle = triangles.len() / 2;
    triangles.select_nth_unstable_by(middle, |a, b| {
        let centroid = |t: &Triangle| t.points.iter().map(|p| p[axis]).sum::<f32>();
        centroid(a).total_cmp(&centroid(b))
    });
    let first = nodes.len() / 2;
    nodes.extend([[0; 4]; 4]);
    nodes[node * 2] = word(lo, first as u32);
    nodes[node * 2 + 1] = word(hi, 0);
    let (left, right) = triangles.split_at_mut(middle);
    build(left, offset, first, nodes);
    build(right, offset + middle, first + 1, nodes);
}

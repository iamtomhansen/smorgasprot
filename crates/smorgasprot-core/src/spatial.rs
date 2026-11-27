//! Uniform 3D voxel spatial hash grid for fast neighborhood and contact queries.

use glam::Vec3;
use std::collections::HashMap;

/// A 3D spatial hash grid indexing points by discrete voxel coordinates.
#[derive(Debug, Clone)]
pub struct SpatialGrid {
    cell_size: f32,
    inv_cell_size: f32,
    cells: HashMap<(i32, i32, i32), Vec<(u32, [f32; 3])>>,
}

impl SpatialGrid {
    /// Creates a new spatial grid with the given voxel `cell_size` (in Angstroms).
    pub fn new(cell_size: f32) -> Self {
        let cell_size = cell_size.max(0.5);
        Self {
            cell_size,
            inv_cell_size: 1.0 / cell_size,
            cells: HashMap::new(),
        }
    }

    #[inline]
    fn cell_coords(&self, pos: [f32; 3]) -> (i32, i32, i32) {
        (
            (pos[0] * self.inv_cell_size).floor() as i32,
            (pos[1] * self.inv_cell_size).floor() as i32,
            (pos[2] * self.inv_cell_size).floor() as i32,
        )
    }

    /// Inserts an item with `id` and 3D position `pos`.
    pub fn insert(&mut self, id: u32, pos: [f32; 3]) {
        let key = self.cell_coords(pos);
        self.cells.entry(key).or_default().push((id, pos));
    }

    /// Calls `f(id, pos, dist_sq)` for every indexed point within spherical `radius` of `query_pos`.
    pub fn query_radius<F>(&self, query_pos: [f32; 3], radius: f32, mut f: F)
    where
        F: FnMut(u32, [f32; 3], f32),
    {
        let r_sq = radius * radius;
        let reach = (radius / self.cell_size).ceil() as i32;
        let (cx, cy, cz) = self.cell_coords(query_pos);

        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    if let Some(bucket) = self.cells.get(&(cx + dx, cy + dy, cz + dz)) {
                        for &(id, pos) in bucket {
                            let ddx = query_pos[0] - pos[0];
                            let ddy = query_pos[1] - pos[1];
                            let ddz = query_pos[2] - pos[2];
                            let d2 = ddx * ddx + ddy * ddy + ddz * ddz;
                            if d2 <= r_sq {
                                f(id, pos, d2);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Returns the bond angle (degrees) at vertex `b` for points `a–b–c`, or `None` if degenerate.
pub fn bond_angle_deg(a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let ba = (a - b).try_normalize()?;
    let bc = (c - b).try_normalize()?;
    Some(ba.angle_between(bc).to_degrees())
}

/// Calculates the signed IUPAC dihedral angle (in degrees, `[-180, 180]`) defined by four 3D
/// points.
pub fn dihedral_deg(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3) -> f32 {
    let b0 = p0 - p1;
    let b1 = p2 - p1;
    let b2 = p3 - p2;
    let b1_norm = b1.normalize_or_zero();
    let v = b0 - b0.dot(b1_norm) * b1_norm;
    let w = b2 - b2.dot(b1_norm) * b1_norm;
    let x = v.dot(w);
    let y = b1_norm.cross(v).dot(w);
    y.atan2(x).to_degrees()
}

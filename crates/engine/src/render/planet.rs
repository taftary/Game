//! Seeded planet mesh: the tier-chosen `HexSphere` as render data.
//!
//! Geometry stays seed-independent (ADR-002): one mesh per `(N, radius)`
//! is shared across bodies. The `seed` is carried for the later world-gen
//! layers (elevation, biomes) that stack per-seed data on these cells —
//! the descriptor hash already binds seed + tier + mesh so saves can
//! reference it (ADR-002 consequences).

use vulkano::buffer::BufferContents;
use vulkano::pipeline::graphics::vertex_input::Vertex;

use super::tier::QualityTier;
use crate::hexsphere::HexSphere;

/// Single planet vertex: position on the sphere, radial normal, and a
/// pentagon flag consumed by the smoke fragment shader (pentagons tinted
/// gold so the 12 Euler-forced sites are visible).
///
/// `#[repr(C)]` + `BufferContents`/`Vertex` make this directly uploadable
/// to a GPU vertex buffer.
#[derive(BufferContents, Vertex, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct PlanetVertex {
    #[format(R32G32B32_SFLOAT)]
    pub position: [f32; 3],
    #[format(R32G32B32_SFLOAT)]
    pub normal: [f32; 3],
    #[format(R32_SFLOAT)]
    pub tint: f32,
}

/// Indexed planet mesh: fan-triangulated dual cells, ready for GPU upload.
#[derive(Clone, Debug)]
pub struct IndexedMesh {
    /// Cell centers first (`cell_count`), then corner vertices.
    pub vertices: Vec<PlanetVertex>,
    /// Triangle list; `len() == 3 * triangle_count`.
    pub indices: Vec<u32>,
}

/// Seeded planet: generation parameters plus the shared base mesh.
#[derive(Clone, Debug)]
pub struct SeededPlanet {
    seed: u64,
    tier: QualityTier,
    radius: f32,
    mesh: HexSphere,
}

impl SeededPlanet {
    /// Generates the tier-chosen mesh. `radius` must be positive and
    /// finite (`HexSphere::generate` panics otherwise — callers validate
    /// first, e.g. CLI argv).
    pub fn generate(seed: u64, tier: QualityTier, radius: f32) -> Self {
        let mesh = HexSphere::generate(tier.subdivisions(), radius);
        Self {
            seed,
            tier,
            radius,
            mesh,
        }
    }

    /// Generation seed (reserved for per-seed layers; mesh is
    /// seed-independent per ADR-002).
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Quality tier this planet was generated for.
    pub fn tier(&self) -> QualityTier {
        self.tier
    }

    /// Sphere radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Shared base mesh.
    pub fn mesh(&self) -> &HexSphere {
        &self.mesh
    }

    /// Triangle count after fan triangulation: 6 per hexagon, 5 per
    /// pentagon (pentagons are first-class cells, never dropped).
    pub fn triangle_count(&self) -> usize {
        let cells = self.mesh.cell_count();
        let pentagons = self.mesh.pentagon_count();
        6 * (cells - pentagons) + 5 * pentagons
    }

    /// Fan-triangulates every dual cell `(center, corner[i],
    /// corner[i+1])`. Rings are CCW seen from outside (hexsphere
    /// guarantee), so front faces are CCW and backface culling suffices
    /// for the convex planet — no depth buffer in M1.
    pub fn to_indexed_mesh(&self) -> IndexedMesh {
        let mesh = &self.mesh;
        let cells = mesh.cell_count() as u32;
        let radius = self.radius;

        // Corner tint: 1.0 on any corner touching a pentagon, so the 12
        // pentagon sites render as gold patches.
        let mut corner_tint = vec![0.0f32; mesh.corner_count()];
        for cell in 0..cells {
            if mesh.is_pentagon(cell) {
                for corner in mesh.cell_corner_ids(cell) {
                    corner_tint[corner as usize] = 1.0;
                }
            }
        }

        let mut vertices = Vec::with_capacity(cells as usize + mesh.corner_count());
        for cell in 0..cells {
            let center = mesh.cell_center(cell);
            vertices.push(PlanetVertex {
                position: center,
                normal: normalize(center, radius),
                tint: if mesh.is_pentagon(cell) { 1.0 } else { 0.0 },
            });
        }
        for corner in 0..mesh.corner_count() as u32 {
            let position = mesh.corner_position(corner);
            vertices.push(PlanetVertex {
                position,
                normal: normalize(position, radius),
                tint: corner_tint[corner as usize],
            });
        }

        let mut indices = Vec::with_capacity(3 * self.triangle_count());
        for cell in 0..cells {
            let ring: Vec<u32> = mesh
                .cell_corner_ids(cell)
                .map(|corner| cells + corner)
                .collect();
            // Closed fan: (center, ring[i], ring[i+1 mod n]).
            for i in 0..ring.len() {
                indices.extend_from_slice(&[cell, ring[i], ring[(i + 1) % ring.len()]]);
            }
        }

        IndexedMesh { vertices, indices }
    }

    /// Deterministic descriptor hash binding seed + tier + radius + mesh.
    /// Same inputs → identical hash on every platform; different seeds →
    /// different descriptors over the same shared mesh.
    pub fn descriptor_hash(&self) -> u64 {
        const FNV_OFFSET: u64 = 14_695_981_039_345_656_037;
        const FNV_PRIME: u64 = 1_099_511_628_211;
        let mut hash = FNV_OFFSET;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        };
        mix(&self.seed.to_le_bytes());
        mix(&(self.tier.subdivisions()).to_le_bytes());
        mix(&self.radius.to_bits().to_le_bytes());
        mix(&self.mesh.mesh_hash().to_le_bytes());
        hash
    }
}

/// Radial normal for a sphere point (mesh is centered on the origin).
fn normalize(point: [f32; 3], radius: f32) -> [f32; 3] {
    [point[0] / radius, point[1] / radius, point[2] / radius]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulation_counts_hold_on_every_tier() {
        for tier in QualityTier::all() {
            let planet = SeededPlanet::generate(42, tier, 1.0);
            let indexed = planet.to_indexed_mesh();
            let cells = planet.mesh().cell_count();
            let corners = planet.mesh().corner_count();
            assert_eq!(indexed.vertices.len(), cells + corners, "{tier:?}");
            assert_eq!(indexed.indices.len(), 3 * planet.triangle_count());
            // 6 per hexagon + 5 per pentagon.
            let hexagons = cells - 12;
            assert_eq!(planet.triangle_count(), 6 * hexagons + 60);
            // Indices reference real vertices only.
            assert!(
                indexed
                    .indices
                    .iter()
                    .all(|&i| (i as usize) < indexed.vertices.len()),
                "{tier:?}"
            );
        }
    }

    #[test]
    fn normals_are_unit_length_and_tint_marks_pentagons() {
        let planet = SeededPlanet::generate(7, QualityTier::Low, 2.0);
        let indexed = planet.to_indexed_mesh();
        for vertex in &indexed.vertices {
            let length =
                (vertex.normal[0].powi(2) + vertex.normal[1].powi(2) + vertex.normal[2].powi(2))
                    .sqrt();
            assert!((length - 1.0).abs() < 1e-5, "{vertex:?}");
            assert!(vertex.tint == 0.0 || vertex.tint == 1.0, "{vertex:?}");
        }
        // Exactly the 12 pentagon centers + their 5 ring corners each.
        let tinted = indexed
            .vertices
            .iter()
            .filter(|vertex| vertex.tint == 1.0)
            .count();
        assert_eq!(tinted, 12 * (1 + 5));
    }

    #[test]
    fn descriptor_is_deterministic_but_seed_sensitive() {
        let first = SeededPlanet::generate(1234, QualityTier::Medium, 1.0);
        let repeat = SeededPlanet::generate(1234, QualityTier::Medium, 1.0);
        let other_seed = SeededPlanet::generate(5678, QualityTier::Medium, 1.0);
        assert_eq!(first.descriptor_hash(), repeat.descriptor_hash());
        assert_ne!(first.descriptor_hash(), other_seed.descriptor_hash());
        // Geometry is seed-independent (ADR-002): same mesh hash.
        assert_eq!(first.mesh().mesh_hash(), other_seed.mesh().mesh_hash());
    }
}

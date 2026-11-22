use bevy::prelude::Vec2;

use crate::terrain::topology::WorldTopology;

#[derive(Clone, Debug)]
pub struct ScalarField {
    pub dim: usize,
    pub size: f32,
    pub topology: WorldTopology,
    pub values: Vec<f32>,
}

impl ScalarField {
    pub fn new(dim: usize, size: f32, topology: WorldTopology) -> Self {
        let dim = dim.max(2);
        Self {
            dim,
            size,
            topology,
            values: vec![0.0; dim * dim],
        }
    }

    pub fn spacing(&self) -> f32 {
        if self.topology.wraps() {
            self.size / self.dim as f32
        } else {
            self.size / (self.dim - 1).max(1) as f32
        }
    }

    pub fn half(&self) -> f32 {
        self.size * 0.5
    }

    pub fn index(&self, x: usize, z: usize) -> usize {
        z % self.dim * self.dim + x % self.dim
    }

    pub fn coords(&self, index: usize) -> (usize, usize) {
        (index % self.dim, index / self.dim)
    }

    pub fn wrap(&self, value: isize) -> usize {
        if self.topology.wraps() {
            value.rem_euclid(self.dim as isize) as usize
        } else {
            value.clamp(0, self.dim as isize - 1) as usize
        }
    }

    pub fn get(&self, x: usize, z: usize) -> f32 {
        self.values[self.index(x, z)]
    }

    pub fn get_at(&self, index: usize) -> f32 {
        self.values[index]
    }

    pub fn set(&mut self, x: usize, z: usize, value: f32) {
        let index = self.index(x, z);
        self.values[index] = value;
    }

    pub fn set_at(&mut self, index: usize, value: f32) {
        self.values[index] = value;
    }

    pub fn add(&mut self, x: usize, z: usize, delta: f32) {
        let index = self.index(x, z);
        self.values[index] += delta;
    }

    fn lattice_axis(&self, value: f32) -> f32 {
        let g = (value + self.half()) / self.spacing();
        let snapped = g.round();
        if (snapped - g).abs() < 1e-6 * (1.0 + g.abs()) {
            snapped
        } else {
            g
        }
    }

    pub fn grid_of(&self, p: Vec2) -> (usize, usize) {
        let wrapped = self.topology.wrap_xz(p, self.size);
        let gx = self.lattice_axis(wrapped.x).floor();
        let gz = self.lattice_axis(wrapped.y).floor();
        (self.wrap(gx as isize), self.wrap(gz as isize))
    }

    pub fn world_of(&self, x: usize, z: usize) -> Vec2 {
        Vec2::new(
            -self.half() + (x % self.dim) as f32 * self.spacing(),
            -self.half() + (z % self.dim) as f32 * self.spacing(),
        )
    }

    pub fn sample_world(&self, p: Vec2) -> f32 {
        let wrapped = self.topology.wrap_xz(p, self.size);
        let gx = self.lattice_axis(wrapped.x);
        let gz = self.lattice_axis(wrapped.y);
        let x0 = gx.floor();
        let z0 = gz.floor();
        let fx = gx - x0;
        let fz = gz - z0;

        let x1 = self.wrap(x0 as isize + 1);
        let z1 = self.wrap(z0 as isize + 1);
        let x0 = self.wrap(x0 as isize);
        let z0 = self.wrap(z0 as isize);

        let v00 = self.get(x0, z0);
        let v10 = self.get(x1, z0);
        let v01 = self.get(x0, z1);
        let v11 = self.get(x1, z1);

        let top = v00 + (v10 - v00) * fx;
        let bottom = v01 + (v11 - v01) * fx;
        top + (bottom - top) * fz
    }

    pub fn gradient(&self, x: usize, z: usize) -> Vec2 {
        let spacing = self.spacing().max(1e-6);
        let last = self.dim - 1;
        let gx = if !self.topology.wraps() && x == 0 {
            (self.get(1.min(last), z) - self.get(0, z)) / spacing
        } else if !self.topology.wraps() && x >= last {
            (self.get(last, z) - self.get(last.saturating_sub(1), z)) / spacing
        } else {
            let left = self.get(self.wrap(x as isize - 1), z);
            let right = self.get(self.wrap(x as isize + 1), z);
            (right - left) / (2.0 * spacing)
        };
        let gz = if !self.topology.wraps() && z == 0 {
            (self.get(x, 1.min(last)) - self.get(x, 0)) / spacing
        } else if !self.topology.wraps() && z >= last {
            (self.get(x, last) - self.get(x, last.saturating_sub(1))) / spacing
        } else {
            let down = self.get(x, self.wrap(z as isize - 1));
            let up = self.get(x, self.wrap(z as isize + 1));
            (up - down) / (2.0 * spacing)
        };
        Vec2::new(gx, gz)
    }

    pub fn gradient_world(&self, p: Vec2) -> Vec2 {
        let wrapped = self.topology.wrap_xz(p, self.size);
        let gx = self.lattice_axis(wrapped.x);
        let gz = self.lattice_axis(wrapped.y);
        let x0 = gx.floor();
        let z0 = gz.floor();
        let tx = gx - x0;
        let tz = gz - z0;

        let x0i = self.wrap(x0 as isize);
        let x1i = self.wrap(x0 as isize + 1);
        let z0i = self.wrap(z0 as isize);
        let z1i = self.wrap(z0 as isize + 1);

        let g00 = self.gradient(x0i, z0i);
        let g10 = self.gradient(x1i, z0i);
        let g01 = self.gradient(x0i, z1i);
        let g11 = self.gradient(x1i, z1i);

        let top = g00 + (g10 - g00) * tx;
        let bottom = g01 + (g11 - g01) * tx;
        top + (bottom - top) * tz
    }

    #[allow(dead_code)]
    fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
        let t2 = t * t;
        let t3 = t2 * t;
        0.5 * ((2.0 * p1)
            + (-p0 + p2) * t
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
    }

    #[allow(dead_code)]
    pub fn sample_catmull_rom(&self, p: Vec2) -> f32 {
        let wrapped = self.topology.wrap_xz(p, self.size);
        let gx = self.lattice_axis(wrapped.x);
        let gz = self.lattice_axis(wrapped.y);
        let x0 = gx.floor();
        let z0 = gz.floor();
        let tx = gx - x0;
        let tz = gz - z0;

        let mut rows = [0.0f32; 4];
        for (row, dz) in (-1..=2).enumerate() {
            let a = self.get(self.wrap(x0 as isize - 1), self.wrap(z0 as isize + dz));
            let b = self.get(self.wrap(x0 as isize), self.wrap(z0 as isize + dz));
            let c = self.get(self.wrap(x0 as isize + 1), self.wrap(z0 as isize + dz));
            let d = self.get(self.wrap(x0 as isize + 2), self.wrap(z0 as isize + dz));
            rows[row] = Self::catmull(a, b, c, d, tx);
        }
        Self::catmull(rows[0], rows[1], rows[2], rows[3], tz)
    }

    pub fn neighbours8(&self, x: usize, z: usize) -> Vec<usize> {
        let mut out = Vec::with_capacity(8);
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if !self.topology.wraps()
                    && (nx < 0 || nz < 0 || nx >= self.dim as isize || nz >= self.dim as isize)
                {
                    continue;
                }
                out.push(self.index(self.wrap(nx), self.wrap(nz)));
            }
        }
        out
    }

    pub fn min(&self) -> f32 {
        self.values
            .iter()
            .cloned()
            .fold(f32::INFINITY, f32::min)
    }

    pub fn max(&self) -> f32 {
        self.values
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max)
    }

    #[cfg(test)]
    pub fn mean(&self) -> f32 {
        if self.values.is_empty() {
            return 0.0;
        }
        self.values.iter().sum::<f32>() / self.values.len() as f32
    }

    pub fn fill(&mut self, value: f32) {
        self.values.iter_mut().for_each(|v| *v = value);
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[cfg(test)]
    pub fn sorted_indices_by_value(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.values.len()).collect();
        indices.sort_by(|&a, &b| self.values[b].total_cmp(&self.values[a]));
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(topology: WorldTopology) -> ScalarField {
        let mut field = ScalarField::new(8, 80.0, topology);
        for z in 0..8 {
            for x in 0..8 {
                field.set(x, z, x as f32 * 10.0);
            }
        }
        field
    }

    #[test]
    fn spacing_divides_the_world_by_the_right_count() {
        let finite = ScalarField::new(8, 80.0, WorldTopology::Finite);
        assert!(
            (finite.spacing() - 80.0 / 7.0).abs() < 1e-5,
            "finite spacing must span [-half, +half] inclusively"
        );
        let torus = ScalarField::new(8, 80.0, WorldTopology::FiniteTorus);
        assert!((torus.spacing() - 10.0).abs() < 1e-5, "torus spacing must span the full period");
        assert!(
            (finite.world_of(7, 0).x - finite.half()).abs() < 1e-4,
            "the last finite column must land on +half, not short of it"
        );
    }

    #[test]
    fn grid_and_world_coordinates_round_trip() {
        let field = ramp(WorldTopology::Finite);
        for z in 0..8 {
            for x in 0..8 {
                let world = field.world_of(x, z);
                let (gx, gz) = field.grid_of(world);
                assert_eq!((gx, gz), (x, z), "round trip failed for ({x},{z})");
            }
        }
    }

    #[test]
    fn bilinear_sampling_matches_the_lattice() {
        let field = ramp(WorldTopology::Finite);
        for z in 0..8 {
            for x in 0..8 {
                let world = field.world_of(x, z);
                assert!((field.sample_world(world) - field.get(x, z)).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn bilinear_sampling_interpolates_between_lattice_points() {
        let field = ramp(WorldTopology::Finite);
        let a = field.world_of(2, 3);
        let b = field.world_of(3, 3);
        let mid = (a + b) * 0.5;
        let expected = (field.get(2, 3) + field.get(3, 3)) * 0.5;
        assert!((field.sample_world(mid) - expected).abs() < 1e-3);
    }

    #[test]
    fn torus_sampling_wraps_across_the_seam() {
        let field = ramp(WorldTopology::FiniteTorus);
        let left = field.sample_world(Vec2::new(-40.0, 0.0));
        let right = field.sample_world(Vec2::new(40.0, 0.0));
        assert!((left - field.get(0, 0)).abs() < 1e-4);
        assert!((right - left).abs() < 1e-4, "torus sampling must be periodic");
    }

    #[test]
    fn finite_sampling_clamps_instead_of_wrapping() {
        let field = ramp(WorldTopology::Finite);
        assert!((field.sample_world(Vec2::new(1000.0, 0.0)) - field.get(7, 0)).abs() < 1e-4);
        assert!((field.sample_world(Vec2::new(-1000.0, 0.0)) - field.get(0, 0)).abs() < 1e-4);
    }

    #[test]
    fn neighbour_lists_respect_topology() {
        let finite = ramp(WorldTopology::Finite);
        let corner = finite.neighbours8(0, 0);
        let self_index = finite.index(0, 0);
        assert_eq!(corner.len(), 3, "a finite corner has only three in-bounds neighbours");
        assert!(
            !corner.contains(&self_index),
            "finite neighbours must never include the query cell itself"
        );
        assert_eq!(corner.iter().collect::<std::collections::HashSet<_>>().len(), 3);
        for expected in [finite.index(1, 0), finite.index(0, 1), finite.index(1, 1)] {
            assert!(corner.contains(&expected), "missing in-bounds neighbour {expected}");
        }
        let edge = finite.neighbours8(3, 0);
        assert_eq!(edge.len(), 5);
        assert!(!edge.contains(&finite.index(3, 0)));
        assert_eq!(finite.neighbours8(3, 3).len(), 8);

        let torus = ramp(WorldTopology::FiniteTorus);
        let wrapped = torus.neighbours8(0, 0);
        assert_eq!(wrapped.len(), 8);
        assert!(!wrapped.contains(&torus.index(0, 0)));
        assert!(
            wrapped.contains(&torus.index(7, 7)),
            "the torus corner must reach the opposite corner"
        );
    }

    #[test]
    fn gradient_of_a_linear_ramp_is_constant() {
        let field = ramp(WorldTopology::Finite);
        for x in 2..6 {
            let g = field.gradient(x, 4);
            assert!((g.x - 10.0 / field.spacing()).abs() < 1e-3, "gradient {g:?}");
            assert!(g.y.abs() < 1e-5);
        }
    }

    #[test]
    fn gradient_at_finite_borders_matches_the_interior() {
        let field = ramp(WorldTopology::Finite);
        let expected = 10.0 / field.spacing();
        for z in 0..8 {
            for x in [0usize, 1, 3, 6, 7] {
                let g = field.gradient(x, z);
                assert!(
                    (g.x - expected).abs() < 1e-3,
                    "border gradient at ({x},{z}) was {g:?}, expected x {expected}"
                );
                assert!(g.y.abs() < 1e-5, "flat axis leaked slope at ({x},{z}): {g:?}");
            }
        }
    }

    #[test]
    fn sampling_straddles_a_lattice_line_without_a_plateau() {
        let field = ramp(WorldTopology::Finite);
        let spacing = field.spacing();
        let line_x = field.world_of(3, 0).x;
        let z = field.world_of(0, 0).y;
        let mut previous = f32::NEG_INFINITY;
        for step in -400..=400 {
            let offset = step as f32 * spacing * 2e-5;
            let value = field.sample_world(Vec2::new(line_x + offset, z));
            assert!(
                value > previous,
                "sample at offset {offset} ({value}) did not exceed the previous sample ({previous})"
            );
            previous = value;
        }
    }

    #[test]
    fn gradient_world_of_a_linear_ramp_is_constant_everywhere() {
        let field = ramp(WorldTopology::Finite);
        let expected = 10.0 / field.spacing();
        for step in 0..=40 {
            let x = -38.0 + step as f32 * 1.9;
            let g = field.gradient_world(Vec2::new(x, -12.5));
            assert!(
                (g.x - expected).abs() < 1e-3,
                "interpolated gradient at x {x} was {g:?}, expected x {expected}"
            );
            assert!(g.y.abs() < 1e-5);
        }
    }

    #[test]
    fn gradient_world_agrees_with_lattice_gradient_at_nodes() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let field = ramp(topology);
            for z in 0..8 {
                for x in 0..8 {
                    let point = field.world_of(x, z);
                    let interpolated = field.gradient_world(point);
                    let lattice = field.gradient(x, z);
                    assert!(
                        (interpolated - lattice).length() < 1e-4,
                        "({x},{z}) interpolated {interpolated:?} vs lattice {lattice:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn gradient_world_varies_smoothly_between_lattice_nodes() {
        let mut field = ScalarField::new(8, 70.0, WorldTopology::Finite);
        for z in 0..8 {
            for x in 0..8 {
                field.set(x, z, (x as f32) * (x as f32));
            }
        }
        let z = field.world_of(0, 3).y;
        let left = field.world_of(3, 3).x;
        let right = field.world_of(4, 3).x;
        let mut previous = f32::NEG_INFINITY;
        let mut distinct = 0usize;
        for step in 0..=8 {
            let t = step as f32 / 8.0;
            let x = left + (right - left) * t;
            let slope = field.gradient_world(Vec2::new(x, z)).x;
            if slope > previous + 1e-6 {
                distinct += 1;
            }
            previous = slope;
        }
        assert!(
            distinct >= 6,
            "a quadratic ramp must produce smoothly increasing slopes between nodes, got {distinct} increases"
        );
    }

    #[test]
    fn catmull_rom_interpolates_lattice_values() {
        let field = ramp(WorldTopology::Finite);
        for z in 0..8 {
            for x in 0..8 {
                let world = field.world_of(x, z);
                assert!(
                    (field.sample_catmull_rom(world) - field.get(x, z)).abs() < 1e-3,
                    "catmull-rom must pass through the lattice at ({x},{z})"
                );
            }
        }
    }

    #[test]
    fn catmull_rom_reproduces_a_linear_ramp() {
        let field = ramp(WorldTopology::Finite);
        for step in 0..=20 {
            let x = -20.0 + step as f32 * 2.0;
            let expected = (x + 40.0) / field.spacing() * 10.0;
            let value = field.sample_catmull_rom(Vec2::new(x, 0.0));
            assert!(
                (value - expected).abs() < 0.05,
                "catmull-rom drifted off the linear ramp at x {x}: {value} vs {expected}"
            );
        }
    }

    #[test]
    fn catmull_rom_wraps_on_the_torus() {
        let field = ramp(WorldTopology::FiniteTorus);
        let left = field.sample_catmull_rom(Vec2::new(-40.0, 0.0));
        let right = field.sample_catmull_rom(Vec2::new(40.0, 0.0));
        assert!(
            (left - right).abs() < 1e-3,
            "catmull-rom must be periodic across the seam: {left} vs {right}"
        );
    }

    #[test]
    fn sort_orders_values_descending() {
        let mut field = ScalarField::new(3, 30.0, WorldTopology::Finite);
        let lattice = [5.0, 1.0, 3.0, 2.0, 4.0, 0.5, 9.0, -2.0, 6.0];
        for (index, value) in lattice.into_iter().enumerate() {
            field.set_at(index, value);
        }
        let order = field.sorted_indices_by_value();
        assert_eq!(order.len(), lattice.len());
        assert_eq!(order[0], 6);
        assert_eq!(*order.last().unwrap(), 7);
        for pair in order.windows(2) {
            assert!(field.get_at(pair[0]) >= field.get_at(pair[1]));
        }
    }
}

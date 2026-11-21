use bevy::prelude::{IVec2, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WorldTopology {
    #[default]
    Finite,
    FiniteTorus,
    Infinite,
}

impl WorldTopology {
    pub fn all() -> [WorldTopology; 3] {
        [WorldTopology::Finite, WorldTopology::FiniteTorus, WorldTopology::Infinite]
    }

    pub fn label(self) -> &'static str {
        match self {
            WorldTopology::Finite => "finite",
            WorldTopology::FiniteTorus => "finite_torus",
            WorldTopology::Infinite => "infinite",
        }
    }

    pub fn parse(label: &str) -> Option<WorldTopology> {
        match label.trim().to_ascii_lowercase().as_str() {
            "finite" => Some(WorldTopology::Finite),
            "torus" | "finite_torus" | "finetorus" => Some(WorldTopology::FiniteTorus),
            "infinite" => Some(WorldTopology::Infinite),
            _ => None,
        }
    }

    pub fn wraps(self) -> bool {
        matches!(self, WorldTopology::FiniteTorus)
    }

    pub fn wrap_axis(self, value: f32, size: f32) -> f32 {
        if !self.wraps() || size <= 0.0 {
            return value;
        }
        let half = size * 0.5;
        (value + half).rem_euclid(size) - half
    }

    pub fn wrap_xz(self, p: Vec2, size: f32) -> Vec2 {
        Vec2::new(self.wrap_axis(p.x, size), self.wrap_axis(p.y, size))
    }

    pub fn delta_xz(self, from: Vec2, to: Vec2, size: f32) -> Vec2 {
        let d = to - from;
        if !self.wraps() || size <= 0.0 {
            return d;
        }
        d - Vec2::splat(size) * (d / size).round()
    }

    pub fn distance_xz(self, a: Vec2, b: Vec2, size: f32) -> f32 {
        self.delta_xz(a, b, size).length()
    }

}

pub fn wrapped_copies(topology: WorldTopology, size: f32) -> Vec<Vec2> {
    if !topology.wraps() {
        return vec![Vec2::ZERO];
    }
    [-1.0, 0.0, 1.0]
        .iter()
        .flat_map(|dx| [-1.0, 0.0, 1.0].map(move |dz| Vec2::new(dx * size, dz * size)))
        .collect()
}

pub fn chunk_key(x: f32, z: f32, chunk_size: f32) -> IVec2 {
    let size = chunk_size.max(1.0);
    IVec2::new(x.div_euclid(size).floor() as i32, z.div_euclid(size).floor() as i32)
}

pub fn chunk_origin(key: IVec2, chunk_size: f32) -> Vec2 {
    let size = chunk_size.max(1.0);
    Vec2::new(key.x as f32 * size, key.y as f32 * size)
}

#[cfg(test)]
pub fn chunk_center(key: IVec2, chunk_size: f32) -> Vec2 {
    let size = chunk_size.max(1.0);
    chunk_origin(key, size) + Vec2::splat(size * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: f32 = 100.0;

    #[test]
    fn finite_topology_does_not_wrap() {
        assert_eq!(WorldTopology::Finite.wrap_axis(80.0, SIZE), 80.0);
        assert_eq!(WorldTopology::Finite.delta_xz(Vec2::ZERO, Vec2::X, SIZE), Vec2::X);
        assert!(!WorldTopology::Finite.wraps());
    }

    #[test]
    fn torus_wraps_coordinates_into_the_canonical_range() {
        let t = WorldTopology::FiniteTorus;
        assert!((t.wrap_axis(60.0, SIZE) - (-40.0)).abs() < 1e-5);
        assert!((t.wrap_axis(-60.0, SIZE) - 40.0).abs() < 1e-5);
        assert!((t.wrap_axis(0.0, SIZE)).abs() < 1e-5);
        for value in [-900.0, -3.0, 0.0, 3.0, 900.0] {
            let w = t.wrap_axis(value, SIZE);
            assert!(w >= -SIZE * 0.5 && w < SIZE * 0.5, "{value} -> {w}");
        }
    }

    #[test]
    fn torus_shortest_distance_crosses_the_seam() {
        let t = WorldTopology::FiniteTorus;
        let a = Vec2::new(48.0, 0.0);
        let b = Vec2::new(-48.0, 0.0);
        assert!((t.distance_xz(a, b, SIZE) - 4.0).abs() < 1e-4, "must wrap, not span 96");
        assert!((t.distance_xz(a, b, SIZE)).abs() <= SIZE * 0.5);
        let d = t.delta_xz(a, b, SIZE);
        assert!(d.x > 0.0, "shortest step from +48 to -48 goes forward over the seam");
        assert!((d.x - 4.0).abs() < 1e-4);
    }

    #[test]
    fn finite_distance_is_euclidean() {
        let a = Vec2::new(48.0, 0.0);
        let b = Vec2::new(-48.0, 0.0);
        assert!((WorldTopology::Finite.distance_xz(a, b, SIZE) - 96.0).abs() < 1e-4);
    }

    #[test]
    fn torus_distance_is_symmetric() {
        let t = WorldTopology::FiniteTorus;
        let a = Vec2::new(49.0, -30.0);
        let b = Vec2::new(-49.0, 31.0);
        assert!((t.distance_xz(a, b, SIZE) - t.distance_xz(b, a, SIZE)).abs() < 1e-4);
    }

    #[test]
    fn wrap_is_idempotent() {
        let t = WorldTopology::FiniteTorus;
        for value in [-123.5, -0.25, 12.0, 77.7] {
            let once = t.wrap_axis(value, SIZE);
            let twice = t.wrap_axis(once, SIZE);
            assert!((once - twice).abs() < 1e-5);
        }
    }

    #[test]
    fn infinite_topology_is_an_explicit_non_wrapping_stub() {
        assert!(!WorldTopology::Infinite.wraps());
        assert_eq!(WorldTopology::Infinite.label(), "infinite");
        assert_eq!(WorldTopology::Infinite.wrap_axis(500.0, SIZE), 500.0);
    }

    #[test]
    fn topology_round_trips_through_labels() {
        for topology in WorldTopology::all() {
            assert_eq!(WorldTopology::parse(topology.label()), Some(topology));
        }
        assert_eq!(WorldTopology::parse("nonsense"), None);
    }

    #[test]
    fn chunk_keys_tile_the_plane() {
        assert_eq!(chunk_key(0.0, 0.0, 50.0), IVec2::new(0, 0));
        assert_eq!(chunk_key(49.9, -0.1, 50.0), IVec2::new(0, -1));
        assert_eq!(chunk_key(-50.1, 100.0, 50.0), IVec2::new(-2, 2));
        assert_eq!(chunk_origin(IVec2::new(2, -1), 50.0), Vec2::new(100.0, -50.0));
        assert_eq!(chunk_center(IVec2::new(0, 0), 50.0), Vec2::new(25.0, 25.0));
    }

    #[test]
    fn every_point_lands_in_its_own_chunk() {
        for x in [-137.0, -1.0, 0.0, 1.0, 63.0] {
            for z in [-137.0, -1.0, 0.0, 1.0, 63.0] {
                let key = chunk_key(x, z, 50.0);
                let origin = chunk_origin(key, 50.0);
                assert!(x >= origin.x && x < origin.x + 50.0);
                assert!(z >= origin.y && z < origin.y + 50.0);
            }
        }
    }
}

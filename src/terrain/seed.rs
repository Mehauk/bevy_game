use bevy::prelude::Vec2;

pub const STAGE_PARAMETERS: u64 = 1;
pub const STAGE_MACRO: u64 = 2;
pub const STAGE_VORONOI: u64 = 3;
pub const STAGE_BOUNDARY_BLEND: u64 = 4;
pub const STAGE_BASE_GRADIENT: u64 = 5;
pub const STAGE_NOISE: u64 = 6;
pub const STAGE_CRATERS: u64 = 9;
#[allow(dead_code)]
pub const STAGE_WIND: u64 = 10;
pub const STAGE_DROPLETS: u64 = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldSeed(pub u64);

impl Default for WorldSeed {
    fn default() -> Self {
        Self(1337)
    }
}

impl WorldSeed {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn stage(&self, stage: u64) -> u64 {
        splitmix64(self.0 ^ splitmix64(stage.wrapping_mul(0x9e37_79b9_7f4a_7c15)))
    }

    #[allow(dead_code)]
    pub fn derive(&self, stage: u64, index: u64) -> u64 {
        splitmix64(
            self.stage(stage)
                ^ index.wrapping_mul(0xbf58_476d_1ce4_e5b9),
        )
    }

    pub fn unit(&self, stage: u64, index: u64, channel: u64) -> f32 {
        let h = splitmix64(
            self.stage(stage)
                ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ channel.wrapping_mul(0xbf58_476d_1ce4_e5b9),
        );
        ((h >> 40) as f32) / ((1u64 << 24) as f32)
    }

    pub fn range(&self, stage: u64, index: u64, channel: u64, min: f32, max: f32) -> f32 {
        min + (max - min) * self.unit(stage, index, channel)
    }

    #[allow(dead_code)]
    pub fn index_of(&self, stage: u64, index: u64, channel: u64, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        ((self.unit(stage, index, channel) * len as f32) as usize).min(len - 1)
    }

    pub fn direction(&self, stage: u64, index: u64, channel: u64) -> Vec2 {
        let angle = self.unit(stage, index, channel) * std::f32::consts::TAU;
        Vec2::new(angle.cos(), angle.sin())
    }
}

pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_values_stay_in_unit_range() {
        for index in 0..512u64 {
            for channel in 0..4u64 {
                let v = WorldSeed::new(7).unit(STAGE_VORONOI, index, channel);
                assert!((0.0..1.0).contains(&v), "unit out of range: {v}");
            }
        }
    }

    #[test]
    fn hashing_is_deterministic() {
        let a = WorldSeed::new(99);
        let b = WorldSeed::new(99);
        assert_eq!(a.derive(STAGE_NOISE, 3), b.derive(STAGE_NOISE, 3));
        assert_eq!(a.unit(STAGE_NOISE, 3, 1), b.unit(STAGE_NOISE, 3, 1));
    }

    #[test]
    fn stages_are_independent() {
        let seed = WorldSeed::new(4242);
        let mut seen = std::collections::HashSet::new();
        for stage in 0..64u64 {
            assert!(seen.insert(seed.stage(stage)), "stage sub-seeds collided at {stage}");
        }
    }

    #[test]
    fn different_seeds_diverge() {
        assert_ne!(
            WorldSeed::new(1).derive(STAGE_MACRO, 0),
            WorldSeed::new(2).derive(STAGE_MACRO, 0)
        );
    }

    #[test]
    fn index_of_covers_the_whole_domain_and_never_overflows() {
        let seed = WorldSeed::new(5);
        assert_eq!(seed.index_of(STAGE_MACRO, 0, 0, 0), 0);
        let mut hits = vec![0; 8];
        for index in 0..2000u64 {
            let i = seed.index_of(STAGE_MACRO, index, 0, 8);
            assert!(i < 8);
            hits[i] += 1;
        }
        assert!(hits.iter().all(|h| *h > 0), "every bucket must be reachable");
    }

    #[test]
    fn directions_are_unit_length() {
        for index in 0..64u64 {
            let d = WorldSeed::new(3).direction(STAGE_WIND, index, 0);
            assert!((d.length() - 1.0).abs() < 1e-4);
        }
    }
}

use bevy::prelude::*;

const EDGE_FEATHER: f32 = 0.1;

#[derive(Clone, Debug, PartialEq)]
pub struct DynWavesSim {
    pub size: usize,
    pub texel: f32,
    pub height: Vec<f32>,
    pub velocity: Vec<f32>,
}

impl DynWavesSim {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            texel: 1.0,
            height: vec![0.0; size * size],
            velocity: vec![0.0; size * size],
        }
    }

    fn edge_weight(&self, index: usize) -> f32 {
        let size = self.size;
        let x = index % size;
        let y = index / size;
        let u = (x as f32 + 0.5) / size as f32;
        let v = (y as f32 + 0.5) / size as f32;
        let dx = u.min(1.0 - u);
        let dy = v.min(1.0 - v);
        let w = (dx.min(dy) / EDGE_FEATHER).clamp(0.0, 1.0);
        0.95f32.lerp(1.0, w)
    }

    pub fn step(&mut self, dt: f32, wave_speed: f32, damping: f32) {
        let size = self.size;
        let texel = self.texel;
        let coeff = dt * wave_speed * wave_speed / (texel * texel);
        let damping_mul = 1.0 - 1.0f32.min(damping * dt);
        let mut next_height = vec![0.0f32; size * size];
        let mut next_velocity = vec![0.0f32; size * size];
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                let xm = y * size + (x + size - 1) % size;
                let xp = y * size + (x + 1) % size;
                let ym = ((y + size - 1) % size) * size + x;
                let yp = ((y + 1) % size) * size + x;
                let laplacian = self.height[xm]
                    + self.height[xp]
                    + self.height[ym]
                    + self.height[yp]
                    - 4.0 * self.height[i];
                let edge = self.edge_weight(i);
                let mut v = self.velocity[i] + coeff * laplacian;
                v *= damping_mul;
                v *= edge;
                let mut h = self.height[i] + dt * v;
                h *= edge;
                if !h.is_finite() || !v.is_finite() {
                    h = 0.0;
                    v = 0.0;
                }
                next_height[i] = h;
                next_velocity[i] = v;
            }
        }
        self.height = next_height;
        self.velocity = next_velocity;
    }

    pub fn splat(&mut self, center: Vec2, radius: f32, strength: f32, velocity_strength: f32) {
        let size = self.size;
        let texel = self.texel;
        let center_x = center.x / texel;
        let center_y = center.y / texel;
        let a = 1.67 / (2.0 * radius);
        let reach = ((radius + 3.0 / a) / texel).ceil() as i32;
        let cx = center_x.round() as i32;
        let cy = center_y.round() as i32;
        for oy in -reach..=reach {
            for ox in -reach..=reach {
                let x = ((cx + ox).rem_euclid(size as i32)) as usize;
                let y = ((cy + oy).rem_euclid(size as i32)) as usize;
                let dx = (cx + ox) as f32 + 0.5 - center_x;
                let dy = (cy + oy) as f32 + 0.5 - center_y;
                let dist = (dx * dx + dy * dy).sqrt() * texel;
                let signed_dist = dist - radius;
                let w = interaction_falloff(a, -signed_dist);
                let i = y * size + x;
                self.height[i] += strength * w;
                self.velocity[i] += velocity_strength * w;
            }
        }
    }

    pub fn sample(&self, x: f32, z: f32, texel: f32) -> f32 {
        let size = self.size;
        let u = (x / texel - 0.5).rem_euclid(size as f32);
        let v = (z / texel - 0.5).rem_euclid(size as f32);
        let u0 = u.floor() as usize % size;
        let v0 = v.floor() as usize % size;
        let u1 = (u0 + 1) % size;
        let v1 = (v0 + 1) % size;
        let fu = u - u.floor();
        let fv = v - v.floor();
        let h00 = self.height[v0 * size + u0];
        let h10 = self.height[v0 * size + u1];
        let h01 = self.height[v1 * size + u0];
        let h11 = self.height[v1 * size + u1];
        h00.lerp(h10, fu).lerp(h01.lerp(h11, fu), fv)
    }
}

fn interaction_falloff(a: f32, x: f32) -> f32 {
    let ax = a * x;
    let ax2 = ax * ax;
    let ax4 = ax2 * ax2;
    ax / (1.0 + ax2 * ax4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splat_raises_height_locally() {
        let mut sim = DynWavesSim::new(32);
        sim.splat(Vec2::new(16.0, 16.0), 3.0, 2.0, 0.0);
        let center = sim.sample(16.0, 16.0, 1.0);
        let far = sim.sample(28.0, 16.0, 1.0);
        assert!(center > 0.5);
        assert!(far.abs() < center / 10.0);
    }

    fn wave_energy(sim: &DynWavesSim, wave_speed: f32) -> f32 {
        let size = sim.size;
        let texel = sim.texel;
        let scale = wave_speed * wave_speed / (texel * texel);
        let mut total = 0.0f32;
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                let dx = sim.height[y * size + (x + 1) % size] - sim.height[i];
                let dy = sim.height[((y + 1) % size) * size + x] - sim.height[i];
                total += sim.velocity[i] * sim.velocity[i] + scale * (dx * dx + dy * dy);
            }
        }
        total
    }

    #[test]
    fn energy_decays_monotonically_with_damping() {
        let mut sim = DynWavesSim::new(32);
        sim.splat(Vec2::new(16.0, 16.0), 3.0, 2.0, 0.0);
        let initial = wave_energy(&sim, 2.0);
        let mut last = initial;
        for step in 0..80 {
            sim.step(0.02, 2.0, 1.5);
            let current = wave_energy(&sim, 2.0);
            assert!(
                current <= last * (1.0 + 1e-4),
                "step {step}: energy grew: {current} > {last}"
            );
            last = current;
        }
        assert!(last < initial * 0.5, "energy barely decayed: {last} of {initial}");
    }

    #[test]
    fn waves_propagate_from_splat() {
        let mut sim = DynWavesSim::new(32);
        sim.splat(Vec2::new(16.0, 16.0), 2.0, 1.0, 0.0);
        let before = sim.sample(24.0, 16.0, 1.0).abs();
        for _ in 0..100 {
            sim.step(0.02, 6.0, 0.0);
        }
        let after = sim.sample(24.0, 16.0, 1.0).abs();
        assert!(after > before);
    }

    #[test]
    fn sample_bilinear_matches_texel_values() {
        let mut sim = DynWavesSim::new(4);
        for i in 0..16 {
            sim.height[i] = i as f32;
        }
        assert!((sim.sample(0.5, 0.5, 1.0) - 0.0).abs() < 1e-6);
        assert!((sim.sample(1.5, 0.5, 1.0) - 1.0).abs() < 1e-6);
        assert!((sim.sample(0.5, 1.5, 1.0) - 4.0).abs() < 1e-6);
        assert!((sim.sample(3.5, 3.5, 1.0) - 15.0).abs() < 1e-6);
    }
}

use bevy::prelude::*;
use bevy_shader_kit::{MAX_RIPPLES, RippleSet, RippleWave};

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput, command};
use crate::utils::registry::debug_info::{DebugInfoAppExt, debug_info};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RippleSystems {
    Age,
    Emit,
    Publish,
}

const MAX_STRENGTH: f32 = 0.3;
const DEFAULT_SPLASH: f32 = 0.12;

#[derive(Resource)]
pub struct RippleField {
    pub waves: Vec<RippleWave>,
    pub time: f32,
    pub frequency: f32,
    pub speed: f32,
    pub decay: f32,
    pub spread: f32,
    pub lifetime: f32,
}

impl Default for RippleField {
    fn default() -> Self {
        Self {
            waves: Vec::new(),
            time: 0.0,
            frequency: 5.0,
            speed: 1.6,
            decay: 0.9,
            spread: 0.2,
            lifetime: 8.0,
        }
    }
}

impl RippleField {
    pub fn splash(&mut self, at: Vec3, strength: f32) {
        let strength = strength.clamp(0.0, MAX_STRENGTH);
        if strength <= 0.0 {
            return;
        }
        if self.waves.len() >= MAX_RIPPLES {
            self.waves.remove(0);
        }
        self.waves.push(RippleWave {
            origin: Vec2::new(at.x, at.z),
            birth: self.time,
            strength,
            frequency: self.frequency,
            speed: self.speed,
            decay: self.decay,
            spread: self.spread,
        });
    }

    pub fn age(&mut self, time: f32) {
        self.time = time;
        let lifetime = self.lifetime;
        self.waves.retain(|wave| time - wave.birth < lifetime);
    }

    pub fn set(&self) -> RippleSet {
        let mut set = RippleSet::default();
        for (index, wave) in self.waves.iter().take(MAX_RIPPLES).enumerate() {
            set.waves[index] = *wave;
        }
        set.count = self.waves.len().min(MAX_RIPPLES) as u32;
        set
    }
}

pub struct RipplePlugin;

impl Plugin for RipplePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RippleField>()
            .configure_sets(
                Update,
                (
                    RippleSystems::Age,
                    RippleSystems::Emit,
                    RippleSystems::Publish,
                )
                    .chain(),
            )
            .add_systems(Update, age_ripples.in_set(RippleSystems::Age));

        command!(
            app,
            "Splash the water surface at <x> <z> [strength]",
            splash
        );
        debug_info!(app, "Ripples", ripple_debug_info);
    }
}

fn age_ripples(time: Res<Time>, mut field: ResMut<RippleField>) {
    field.age(time.elapsed_secs());
}

fn splash(
    args: Res<CommandArgs>,
    mut field: ResMut<RippleField>,
    mut output: ResMut<CommandOutput>,
) {
    let (Some(x), Some(z)) = (args.get_f32(0), args.get_f32(1)) else {
        output.set("Usage: splash <x> <z> [strength]");
        return;
    };
    let strength = args.get_f32(2).unwrap_or(DEFAULT_SPLASH);
    field.splash(Vec3::new(x, 0.0, z), strength);
    output.set(format!(
        "splash at ({x:.1}, {z:.1}), strength {strength:.2}"
    ));
}

fn ripple_debug_info(field: Res<RippleField>) -> Option<String> {
    Some(format!("{} live", field.waves.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_stamps_position_birth_and_tuning() {
        let mut field = RippleField::default();
        field.age(4.0);
        field.splash(Vec3::new(2.0, 9.0, -3.0), 0.1);

        let wave = field.waves.first().unwrap();
        assert_eq!(wave.origin, Vec2::new(2.0, -3.0));
        assert_eq!(wave.birth, 4.0);
        assert_eq!(wave.strength, 0.1);
        assert_eq!(wave.frequency, field.frequency);
    }

    #[test]
    fn splash_ignores_zero_and_clamps_strong_impacts() {
        let mut field = RippleField::default();
        field.splash(Vec3::ZERO, 0.0);
        assert!(field.waves.is_empty());

        field.splash(Vec3::ZERO, 100.0);
        assert_eq!(field.waves.first().unwrap().strength, MAX_STRENGTH);
    }

    #[test]
    fn splash_evicts_the_oldest_once_full() {
        let mut field = RippleField::default();
        for index in 0..=MAX_RIPPLES {
            field.time = index as f32;
            field.splash(Vec3::X * index as f32, 0.1);
        }

        assert_eq!(field.waves.len(), MAX_RIPPLES);
        assert_eq!(field.waves.first().unwrap().birth, 1.0);
        assert_eq!(field.waves.last().unwrap().birth, MAX_RIPPLES as f32);
    }

    #[test]
    fn age_retires_waves_past_their_lifetime() {
        let mut field = RippleField::default();
        field.splash(Vec3::ZERO, 0.1);
        let lifetime = field.lifetime;

        field.age(lifetime * 0.5);
        assert_eq!(field.waves.len(), 1);

        field.age(lifetime * 1.5);
        assert!(field.waves.is_empty());
    }

    #[test]
    fn set_packs_waves_from_the_front_and_counts_them() {
        let mut field = RippleField::default();
        field.splash(Vec3::ZERO, 0.1);
        field.splash(Vec3::X, 0.2);

        let set = field.set();
        assert_eq!(set.count, 2);
        assert_eq!(set.waves[0].strength, 0.1);
        assert_eq!(set.waves[1].strength, 0.2);
        assert_eq!(set.waves[2], RippleWave::default());
    }
}

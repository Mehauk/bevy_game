use bevy::prelude::*;
use crest_water::OceanSettings;

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput, command};

pub(super) fn register(app: &mut App) {
    command!(app, "Set the water level", set_water_level);
    command!(app, "Set the wave energy", set_wave_energy);
    command!(app, "Toggle water ripples", toggle_water_ripples);
    command!(app, "Toggle water foam", toggle_water_foam);
    command!(app, "Set the water weather", set_water_weather);
    command!(app, "Set the FFT swell strength", set_swell_strength);
    command!(app, "Set the FFT chop strength", set_chop_strength);
}

fn set_water_level(
    args: Res<CommandArgs>,
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(height) = args.get_f32(0) else {
        output.set("Usage: set_water_level <height>");
        return;
    };
    settings.water_level = height;
    output.set(format!("Water level: {height:.2}"));
}

fn set_wave_energy(
    args: Res<CommandArgs>,
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(energy) = args.get_f32(0) else {
        output.set("Usage: set_wave_energy <0..1>");
        return;
    };
    let energy = energy.clamp(0.0, 1.0);
    settings.waves.amplitude_multiplier = energy;
    output.set(format!("Wave energy: {energy:.2}"));
}

fn toggle_water_ripples(
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    settings.dynamic_waves_enabled = !settings.dynamic_waves_enabled;
    output.set(if settings.dynamic_waves_enabled {
        "Water ripples: on"
    } else {
        "Water ripples: off"
    });
}

fn toggle_water_foam(
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    settings.foam_enabled = !settings.foam_enabled;
    output.set(if settings.foam_enabled {
        "Water foam: on"
    } else {
        "Water foam: off"
    });
}

fn set_water_weather(
    args: Res<CommandArgs>,
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(preset) = args.args.first().map(|name| name.as_str()) else {
        output.set("Usage: set_water_weather <calm|moderate|storm>");
        return;
    };
    let (wind_speed, chop, turbulence) = match preset {
        "calm" => (2.0, 1.0, 0.2),
        "moderate" => (6.0, 1.6, 0.4),
        "storm" => (14.0, 2.2, 0.8),
        _ => {
            output.set("Usage: set_water_weather <calm|moderate|storm>");
            return;
        }
    };
    settings.waves.wind_speed = wind_speed;
    settings.waves.chop = chop;
    settings.waves.turbulence = turbulence;
    output.set(format!("Weather: {preset}"));
}

fn set_swell_strength(
    args: Res<CommandArgs>,
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(strength) = args.get_f32(0) else {
        output.set("Usage: set_swell_strength <strength>");
        return;
    };
    let strength = strength.clamp(0.0, 2.0);
    settings.waves.amplitude_multiplier = strength;
    output.set(format!("Swell strength: {strength:.2}"));
}

fn set_chop_strength(
    args: Res<CommandArgs>,
    mut settings: ResMut<OceanSettings>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(strength) = args.get_f32(0) else {
        output.set("Usage: set_chop_strength <strength>");
        return;
    };
    let strength = strength.clamp(0.0, 2.0);
    settings.waves.chop = strength;
    output.set(format!("Chop strength: {strength:.2}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_water_level_command_moves_the_ocean() {
        let mut world = World::new();
        world.insert_resource(OceanSettings::default());
        world.insert_resource(CommandArgs {
            args: vec!["1.5".to_string()],
        });
        world.insert_resource(CommandOutput::default());
        let id = world.register_system(set_water_level);
        world.run_system(id).unwrap();
        assert_eq!(world.resource::<OceanSettings>().water_level, 1.5);
    }

    #[test]
    fn the_weather_command_sets_the_wind() {
        let mut world = World::new();
        world.insert_resource(OceanSettings::default());
        world.insert_resource(CommandArgs {
            args: vec!["storm".to_string()],
        });
        world.insert_resource(CommandOutput::default());
        let id = world.register_system(set_water_weather);
        world.run_system(id).unwrap();
        assert_eq!(world.resource::<OceanSettings>().waves.wind_speed, 14.0);
    }

    #[test]
    fn an_unknown_weather_leaves_the_settings_alone() {
        let mut world = World::new();
        world.insert_resource(OceanSettings::default());
        world.insert_resource(CommandArgs {
            args: vec!["hurricane".to_string()],
        });
        world.insert_resource(CommandOutput::default());
        let id = world.register_system(set_water_weather);
        world.run_system(id).unwrap();
        assert_eq!(world.resource::<OceanSettings>().waves.wind_speed, 4.0);
    }
}

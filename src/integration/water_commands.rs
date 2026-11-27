use bevy::prelude::*;
use bevy_shader_kit::WaterMaterial;
use world_water::{DyeBand, SeaState, WaterSurface, WeatherPreset, flip_material_flag};

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput, command};

pub(super) fn register(app: &mut App) {
    command!(app, "Set the water level", set_water_level);
    command!(app, "Set the wave energy", set_wave_energy);
    command!(app, "Set the water dye color", set_water_color);
    command!(app, "Set the water refraction", set_water_refraction);
    command!(app, "Set the caustic strength", set_caustic_strength);
    command!(app, "Toggle water ripples", toggle_water_ripples);
    command!(app, "Toggle water foam", toggle_water_foam);
    command!(app, "Toggle underwater light optics", toggle_water_optics);
    command!(app, "Toggle body foam", toggle_water_body_foam);
    command!(app, "Toggle surface detail", toggle_water_detail);
    command!(app, "Toggle water dye", toggle_water_dye);
    command!(app, "Toggle water caustics", toggle_water_caustics);
    command!(app, "Toggle water refraction", toggle_water_refraction);
    command!(app, "Toggle the tides", toggle_water_tides);
    command!(app, "Toggle ocean swells", toggle_water_swells);
    command!(app, "Toggle FFT water", toggle_water_fft);
    command!(app, "Set the water weather", set_water_weather);
    command!(app, "Set the FFT swell strength", set_swell_strength);
    command!(app, "Set the FFT chop strength", set_chop_strength);
    command!(app, "Toggle wet sand", toggle_water_wetness);
}

fn set_water_level(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(height) = args.get_f32(0) else {
        output.set("Usage: set_water_level <height>");
        return;
    };
    output.set(sea_state.set_base_level(height));
}

fn set_wave_energy(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(energy) = args.get_f32(0) else {
        output.set("Usage: set_wave_energy <0..1>");
        return;
    };
    output.set(sea_state.set_wave_energy(energy));
}

fn set_water_color(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let usage = "Usage: set_water_color <shallow|deep> <r> <g> <b>";
    let band = match args.args.first().map(|kind| kind.as_str()) {
        Some("shallow") => DyeBand::Shallow,
        Some("deep") => DyeBand::Deep,
        _ => {
            output.set(usage);
            return;
        }
    };
    let (Some(r), Some(g), Some(b)) = (args.get_f32(1), args.get_f32(2), args.get_f32(3)) else {
        output.set(usage);
        return;
    };
    output.set(sea_state.set_dye_color(band, Vec3::new(r, g, b)));
}

fn set_water_refraction(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(strength) = args.get_f32(0) else {
        output.set("Usage: set_water_refraction <strength>");
        return;
    };
    output.set(sea_state.set_refraction(strength));
}

fn set_caustic_strength(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(strength) = args.get_f32(0) else {
        output.set("Usage: set_caustic_strength <strength>");
        return;
    };
    output.set(sea_state.set_caustic_strength(strength));
}

fn toggle_water_ripples(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water ripples", |ext| {
            &mut ext.ripples
        })
    {
        output.set(text);
    }
}

fn toggle_water_foam(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water foam", |ext| {
            &mut ext.foam
        })
    {
        output.set(text);
    }
}

fn toggle_water_optics(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water optics", |ext| {
            &mut ext.optics
        })
    {
        output.set(text);
    }
}

fn toggle_water_body_foam(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Body foam", |ext| {
            &mut ext.body_foam
        })
    {
        output.set(text);
    }
}

fn toggle_water_detail(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water detail", |ext| {
            &mut ext.detail
        })
    {
        output.set(text);
    }
}

fn toggle_water_dye(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water dye", |ext| {
            &mut ext.dye
        })
    {
        output.set(text);
    }
}

fn toggle_water_caustics(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water caustics", |ext| {
            &mut ext.caustics
        })
    {
        output.set(text);
    }
}

fn toggle_water_refraction(
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut output: ResMut<CommandOutput>,
) {
    if let Some(text) =
        flip_material_flag(surface.as_deref(), &mut materials, "Water refraction", |ext| {
            &mut ext.refract
        })
    {
        output.set(text);
    }
}

fn toggle_water_tides(
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    output.set(sea_state.toggle_tides());
}

fn toggle_water_swells(
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    output.set(sea_state.toggle_swells());
}

fn toggle_water_fft(
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    output.set(sea_state.toggle_fft());
}

fn toggle_water_wetness(
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    output.set(sea_state.toggle_wetness());
}

fn set_water_weather(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let preset = match args.args.first().map(|name| name.as_str()) {
        Some("calm") => WeatherPreset::Calm,
        Some("moderate") => WeatherPreset::Moderate,
        Some("storm") => WeatherPreset::Storm,
        _ => {
            output.set("Usage: set_water_weather <calm|moderate|storm>");
            return;
        }
    };
    output.set(sea_state.apply_weather(preset));
}

fn set_swell_strength(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(strength) = args.get_f32(0) else {
        output.set("Usage: set_swell_strength <strength>");
        return;
    };
    output.set(sea_state.set_swell_strength(strength));
}

fn set_chop_strength(
    args: Res<CommandArgs>,
    mut sea_state: ResMut<SeaState>,
    mut output: ResMut<CommandOutput>,
) {
    let Some(strength) = args.get_f32(0) else {
        output.set("Usage: set_chop_strength <strength>");
        return;
    };
    output.set(sea_state.set_chop_strength(strength));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dye_for(args: Vec<String>) -> (Vec3, Vec3, Option<String>) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SeaState>()
            .insert_resource(CommandArgs { args })
            .init_resource::<CommandOutput>();
        let paint = app.world_mut().register_system(set_water_color);
        app.world_mut().run_system(paint).unwrap();
        let sea = app.world().resource::<SeaState>();
        let (deep, shallow) = (sea.dye.deep, sea.dye.shallow);
        let output = app.world_mut().resource_mut::<CommandOutput>().take();
        (deep, shallow, output)
    }

    #[test]
    fn the_dye_command_paints_the_requested_band() {
        let (deep, shallow, _) = dye_for(vec![
            "deep".to_string(),
            "0.1".to_string(),
            "0.2".to_string(),
            "0.3".to_string(),
        ]);
        assert_eq!(deep, Vec3::new(0.1, 0.2, 0.3));
        assert_eq!(shallow, bevy_shader_kit::WaterDye::default().shallow);
    }

    #[test]
    fn the_dye_command_leaves_the_sea_alone_without_a_band() {
        let (deep, shallow, output) = dye_for(vec![
            "murk".to_string(),
            "0.1".to_string(),
            "0.2".to_string(),
            "0.3".to_string(),
        ]);
        assert_eq!(deep, bevy_shader_kit::WaterDye::default().deep);
        assert_eq!(shallow, bevy_shader_kit::WaterDye::default().shallow);
        assert_eq!(
            output.as_deref(),
            Some("Usage: set_water_color <shallow|deep> <r> <g> <b>"),
            "an unknown band must report the usage rather than paint"
        );
    }
}

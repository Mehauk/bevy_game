use bevy::{
    pbr::{MaterialPlugin, StandardMaterial},
    prelude::*,
};
use bevy_shader_kit::{DistanceFieldViewMaterial, SdfVolume};

use crate::utils::registry::commands::{CommandAppExt, CommandOutput, command};
use crate::utils::registry::debug_info::{DebugInfoAppExt, debug_info};

mod baker;
mod view;

use baker::bake_distance_fields;
use view::{animate_distance_field, sync_distance_field_view};

pub const MAX_TEXELS: usize = 4_000_000;

#[derive(Resource, Default)]
pub struct DistanceFieldView {
    pub active: bool,
}

impl DistanceFieldView {
    pub fn toggle(&mut self) -> bool {
        self.active = !self.active;
        self.active
    }
}

#[derive(Component, Default)]
pub struct DistanceFieldSource {
    pub texel: Vec3,
    pub water_margin: f32,
    pub surface_top: f32,
    pub clay_alpha: f32,
    pub glow_range: f32,
    pub volume: SdfVolume,
    pub image: Handle<Image>,
    pub view_material: Handle<DistanceFieldViewMaterial>,
    pub baked: bool,
    pub view_entity: Option<Entity>,
    pub hidden_material: Option<Handle<StandardMaterial>>,
}

impl DistanceFieldSource {
    pub fn prop() -> Self {
        Self {
            texel: Vec3::splat(0.1),
            water_margin: 3.0,
            clay_alpha: 1.0,
            ..default()
        }
    }
}

#[derive(Component)]
pub struct PrimaryField;

#[derive(Component)]
pub struct DistanceFieldVolume;

#[derive(Component)]
pub struct DistanceFieldViewHidden;

#[derive(Component)]
pub struct DistanceFieldDimmedColor(pub Color);

#[derive(Component)]
pub struct DimmedColorRestore(pub Color);

#[derive(Component)]
pub struct DimmedFog(pub f32);

#[derive(Resource)]
pub struct InvisibleMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
pub struct DistanceFieldUnitCube(pub Handle<Mesh>);

pub struct DistanceFieldPlugin;

impl Plugin for DistanceFieldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<DistanceFieldViewMaterial>::default())
            .init_resource::<DistanceFieldView>()
            .add_systems(Startup, setup_unit_cube)
            .add_systems(
                Update,
                (
                    bake_distance_fields,
                    animate_distance_field,
                    sync_distance_field_view,
                ),
            );

        command!(app, "Toggle distance field view", toggle_distance_field);
        debug_info!(app, "Distance Field", distance_field_debug_info);
    }
}

fn setup_unit_cube(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(DistanceFieldUnitCube(meshes.add(Cuboid::default())));
}

fn toggle_distance_field(mut view: ResMut<DistanceFieldView>, mut output: ResMut<CommandOutput>) {
    output.set(if view.toggle() {
        "Distance field view: on"
    } else {
        "Distance field view: off"
    });
}

fn distance_field_debug_info(
    view: Res<DistanceFieldView>,
    primary: Query<&DistanceFieldSource, With<PrimaryField>>,
) -> Option<String> {
    let source = primary.iter().next()?;
    let r = source.volume.resolution;
    Some(format!(
        "{} {}x{}x{}",
        if view.active { "on" } else { "off" },
        r.x as i32,
        r.y as i32,
        r.z as i32,
    ))
}

use bevy::{prelude::*, shader::Shader, shader::ShaderDefVal};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderHook {
    DepthSample,
    RippleField,
    FoamEdge,
    FoamPattern,
    FoamRoughness,
    PreLighting,
    PostLighting,
    VolumeOptics,
    FoamShade,
    PostProcess,
    CustomStage(&'static str),
}

impl ShaderHook {
    pub fn slot_name(&self) -> String {
        match self {
            ShaderHook::DepthSample => "depth_sample".to_string(),
            ShaderHook::RippleField => "ripple_field".to_string(),
            ShaderHook::FoamEdge => "foam_edge".to_string(),
            ShaderHook::FoamPattern => "foam_pattern".to_string(),
            ShaderHook::FoamRoughness => "foam_roughness".to_string(),
            ShaderHook::PreLighting => "pre_lighting".to_string(),
            ShaderHook::PostLighting => "post_lighting".to_string(),
            ShaderHook::VolumeOptics => "volume_optics".to_string(),
            ShaderHook::FoamShade => "foam_shade".to_string(),
            ShaderHook::PostProcess => "post_process".to_string(),
            ShaderHook::CustomStage(name) => (*name).to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookMode {
    Append,
    Replace,
}

#[derive(Clone, Debug)]
pub struct ShaderHookPoint {
    pub hook: ShaderHook,
    pub mode: HookMode,
    pub source: String,
}

impl ShaderHookPoint {
    pub fn append(hook: ShaderHook, source: impl Into<String>) -> Self {
        Self { hook, mode: HookMode::Append, source: source.into() }
    }

    pub fn replace(hook: ShaderHook, source: impl Into<String>) -> Self {
        Self { hook, mode: HookMode::Replace, source: source.into() }
    }
}

#[derive(Clone, Debug)]
pub struct ShaderModification {
    pub id: &'static str,
    pub priority: i32,
    pub enabled: bool,
    pub imports: Vec<String>,
    pub shader_defs: Vec<ShaderDefVal>,
    pub declarations: Vec<String>,
    pub hooks: Vec<ShaderHookPoint>,
}

impl ShaderModification {
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            priority: 0,
            enabled: true,
            imports: Vec::new(),
            shader_defs: Vec::new(),
            declarations: Vec::new(),
            hooks: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum BlueprintSegment {
    Text(&'static str),
    Slot { hook: ShaderHook, default: &'static str },
}

#[derive(Clone, Debug)]
pub struct ShaderBlueprint {
    pub name: &'static str,
    pub imports: Vec<&'static str>,
    pub declarations: Vec<&'static str>,
    pub shader_defs: Vec<ShaderDefVal>,
    pub entry: &'static str,
    pub segments: Vec<BlueprintSegment>,
}

#[derive(Clone, Debug)]
pub struct ComposedShader {
    pub source: String,
    pub shader_defs: Vec<ShaderDefVal>,
}

fn def_name(def: &ShaderDefVal) -> &str {
    match def {
        ShaderDefVal::Bool(name, _) => name,
        ShaderDefVal::Int(name, _) => name,
        ShaderDefVal::UInt(name, _) => name,
    }
}

fn enabled_sorted<'a>(modifications: &'a [ShaderModification]) -> Vec<&'a ShaderModification> {
    let mut active: Vec<&ShaderModification> = modifications.iter().filter(|m| m.enabled).collect();
    active.sort_by(|a, b| (a.priority, a.id).cmp(&(b.priority, b.id)));
    active
}

pub fn compose_wesl(
    blueprint: &ShaderBlueprint,
    modifications: &[ShaderModification],
) -> ComposedShader {
    let active = enabled_sorted(modifications);

    let mut imports: Vec<&str> = Vec::new();
    let mut candidates: Vec<&str> = blueprint.imports.iter().copied().collect();
    for modification in &active {
        candidates.extend(modification.imports.iter().map(|line| line.as_str()));
    }
    for line in candidates {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !imports.contains(&trimmed) {
            imports.push(trimmed);
        }
    }

    let mut declarations: Vec<String> = blueprint
        .declarations
        .iter()
        .map(|decl| (*decl).to_string())
        .collect();
    for modification in &active {
        declarations.extend(modification.declarations.iter().cloned());
    }

    let mut shader_defs = blueprint.shader_defs.clone();
    for modification in &active {
        for def in &modification.shader_defs {
            shader_defs.retain(|existing| def_name(existing) != def_name(def));
            shader_defs.push(def.clone());
        }
    }

    let mut body = String::new();
    for segment in &blueprint.segments {
        match segment {
            BlueprintSegment::Text(text) => body.push_str(text),
            BlueprintSegment::Slot { hook, default } => {
                let replacement = active.iter().rev().find_map(|modification| {
                    modification
                        .hooks
                        .iter()
                        .rev()
                        .find(|point| point.hook == *hook && point.mode == HookMode::Replace)
                });
                match replacement {
                    Some(point) => body.push_str(&point.source),
                    None => body.push_str(default),
                }
                for modification in &active {
                    for point in &modification.hooks {
                        if point.hook == *hook && point.mode == HookMode::Append {
                            if !body.ends_with('\n') {
                                body.push('\n');
                            }
                            body.push_str(&point.source);
                            if !body.ends_with('\n') {
                                body.push('\n');
                            }
                        }
                    }
                }
            }
        }
    }

    let mut source = String::new();
    for line in &imports {
        source.push_str(line);
        source.push('\n');
    }
    source.push('\n');
    for declaration in &declarations {
        source.push_str(declaration);
        source.push('\n');
    }
    source.push('\n');
    source.push_str(blueprint.entry);
    source.push('\n');
    source.push_str(&body);

    ComposedShader { source, shader_defs }
}

const WATER_FRAGMENT_ENTRY: &str = "@fragment\nfn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {";

pub fn water_blueprint() -> ShaderBlueprint {
    ShaderBlueprint {
        name: "water",
        imports: vec![
            "import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};",
            "import bevy_pbr::render::mesh_view_bindings::view;",
            "import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;",
            "import bevy_pbr::render::pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing};",
            "import bevy_pbr::render::view_transformations::depth_ndc_to_view_z;",
            "import bevy_pbr::prepass::utils::prepass_depth;",
            "import package::shaders::math::falloff::view_distance_fade;",
            "import package::shaders::noise::fbm2d::scrolling_fbm_2d;",
            "import package::shaders::sdf::volume::volume_sample_world;",
            "import package::shaders::water::body::{body_sdf, body_volume};",
            "import package::shaders::water::coast::{\n    COAST_SWASH_BLEND, COAST_SWASH_RUNUP, coastal_wash_mask, shore_context_at, spill_height_at,\n    terrain_height_at,\n};",
            "import package::shaders::water::swells::swell_height_shored;",
            "import package::shaders::water::foam::{\n    apply_foam_roughness, blend_foam_color, body_contact_mask, foam_noise_mask, shore_edge_mask,\n};",
            "import package::shaders::water::optics::water_volume_in_scatter;",
            "import package::shaders::water::ripples::{ripple_foam_edge, ripple_normal, ripple_slope};",
            "import package::shaders::water::types::{WaterContext, water};",
        ],
        declarations: vec![
            "const WATER_VIEW_FADE_RANGE: f32 = 60.0;",
        ],
        shader_defs: Vec::new(),
        entry: WATER_FRAGMENT_ENTRY,
        segments: vec![
            BlueprintSegment::Text(
                "    var out: FragmentOutput;\n    var pbr_input = pbr_input_from_standard_material(in, front);\n    pbr_input.material.base_color = alpha_discard(\n        pbr_input.material.flags,\n        pbr_input.material.alpha_cutoff,\n        pbr_input.material.base_color,\n    );\n\n    var ctx: WaterContext;\n    ctx.p = pbr_input.world_position.xyz;\n    ctx.view_fade = view_distance_fade(ctx.p, view.world_position, WATER_VIEW_FADE_RANGE);\n\n    ctx.depth = water.max_depth;\n    @if(DEPTH_PREPASS) {\n",
            ),
            BlueprintSegment::Slot {
                hook: ShaderHook::DepthSample,
                default: "        let surface_view_z = depth_ndc_to_view_z(in.position.z);\n        let behind_view_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));\n        ctx.depth = clamp(surface_view_z - behind_view_z, 0.0, water.max_depth);\n",
            },
            BlueprintSegment::Text(
                "    }\n    var coast_rest_depth: f32 = 1.0e6;\n    @if(WATER_COAST) {\n        let terrain_y = terrain_height_at(ctx.p.xz);\n        if (water.water_height + COAST_SWASH_RUNUP < spill_height_at(ctx.p.xz)) {\n            discard;\n        }\n        coast_rest_depth = water.water_height - terrain_y;\n        ctx.depth = min(ctx.depth, clamp(coast_rest_depth, 0.0, water.max_depth));\n    }\n    var body_distance: f32 = 1.0e6;\n    @if(WATER_BODY_FOAM) {\n        body_distance = volume_sample_world(body_volume, body_sdf, ctx.p);\n        ctx.depth = min(ctx.depth, max(body_distance, 0.0));\n    }\n    pbr_input.material.thickness = ctx.depth;\n\n    ctx.ripple = vec2<f32>(0.0);\n    @if(WATER_RIPPLES) {\n",
            ),
            BlueprintSegment::Slot {
                hook: ShaderHook::RippleField,
                default: "        ctx.ripple = ripple_slope(ctx.p.xz, water.time);\n",
            },
            BlueprintSegment::Text(
                "        pbr_input.N = ripple_normal(pbr_input.N, ctx.ripple, water.ripple_normal);\n    }\n\n    @if(WATER_FOAM) {\n",
            ),
            BlueprintSegment::Slot {
                hook: ShaderHook::FoamEdge,
                default: "        ctx.edge = max(shore_edge_mask(ctx.depth, water.foam_width), ripple_foam_edge(length(ctx.ripple), water.ripple_foam));\n",
            },
            BlueprintSegment::Text(
                "        @if(WATER_BODY_FOAM) {\n            let surface_proximity = 1.0 - smoothstep(0.0, water.foam_width, abs(ctx.p.y - water.water_height));\n            ctx.edge = max(ctx.edge, body_contact_mask(body_distance, surface_proximity));\n        }\n        @if(WATER_COAST) {\n            let coast_shore = shore_context_at(ctx.p.xz);\n            let surf_zone = smoothstep(3.0, 1.0, coast_rest_depth) * smoothstep(0.0, 0.5, coast_rest_depth) * water.wave_energy;\n            let wash_line = (1.0 - smoothstep(0.0, COAST_SWASH_BLEND, coast_rest_depth)) * coastal_wash_mask(coast_shore, swell_height_shored(ctx.p.xz, water.time, coast_shore)) * water.wave_energy;\n            ctx.edge = max(ctx.edge, max(surf_zone, wash_line));\n        }\n",
            ),
            BlueprintSegment::Slot {
                hook: ShaderHook::FoamPattern,
                default: "        ctx.pattern = scrolling_fbm_2d(ctx.p.xz * 2.2, water.time);\n",
            },
            BlueprintSegment::Text(
                "        ctx.foam = foam_noise_mask(ctx.edge, ctx.pattern, water.foam_strength, ctx.view_fade);\n        if (ctx.foam > 0.0) {\n",
            ),
            BlueprintSegment::Slot {
                hook: ShaderHook::FoamRoughness,
                default: "            pbr_input.material.perceptual_roughness = apply_foam_roughness(pbr_input.material.perceptual_roughness, ctx.foam);\n",
            },
            BlueprintSegment::Text("        }\n    }\n\n"),
            BlueprintSegment::Slot {
                hook: ShaderHook::PreLighting,
                default: "",
            },
            BlueprintSegment::Text("    out.color = apply_pbr_lighting(pbr_input);\n\n"),
            BlueprintSegment::Slot {
                hook: ShaderHook::PostLighting,
                default: "",
            },
            BlueprintSegment::Text("    @if(WATER_OPTICS) {\n"),
            BlueprintSegment::Slot {
                hook: ShaderHook::VolumeOptics,
                default: "        if (front) {\n            let in_scatter = water_volume_in_scatter(pbr_input.V, pbr_input.N, ctx.depth, water.scattering, water.absorption, water.sun_direction, water.sun_illuminance, water.ambient_illuminance, water.phase_g);\n            out.color = vec4<f32>(out.color.rgb + in_scatter, out.color.a);\n        }\n",
            },
            BlueprintSegment::Text("    }\n\n    @if(WATER_FOAM) {\n        if (ctx.foam > 0.0) {\n"),
            BlueprintSegment::Slot {
                hook: ShaderHook::FoamShade,
                default: "            out.color = vec4<f32>(blend_foam_color(out.color.rgb, pbr_input.N, ctx.foam), out.color.a);\n",
            },
            BlueprintSegment::Text("        }\n    }\n\n    out.color = main_pass_post_lighting_processing(pbr_input, out.color);\n"),
            BlueprintSegment::Slot {
                hook: ShaderHook::PostProcess,
                default: "",
            },
            BlueprintSegment::Text("    return out;\n}\n"),
        ],
    }
}

#[derive(Debug)]
pub struct DynamicShader {
    pub name: String,
    pub blueprint: ShaderBlueprint,
    pub modifications: Vec<ShaderModification>,
    pub shader: Handle<Shader>,
    pub virtual_path: String,
    pub dirty: bool,
}

impl DynamicShader {
    pub fn new(name: &str, blueprint: ShaderBlueprint) -> Self {
        Self {
            name: name.to_string(),
            blueprint,
            modifications: Vec::new(),
            shader: Handle::from(bevy::asset::uuid::Uuid::new_v4()),
            virtual_path: format!("embedded://bevy_shader_kit/shaders/water/{name}.wesl"),
            dirty: true,
        }
    }

    pub fn push(&mut self, modification: ShaderModification) {
        self.modifications.push(modification);
        self.dirty = true;
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        if let Some(modification) = self.modifications.iter_mut().find(|m| m.id == id) {
            modification.enabled = enabled;
            self.dirty = true;
        }
    }

    pub fn compose(&self) -> ComposedShader {
        compose_wesl(&self.blueprint, &self.modifications)
    }
}

#[derive(Resource, Default)]
pub struct DynamicShaderRegistry {
    pub entries: Vec<DynamicShader>,
}

impl DynamicShaderRegistry {
    pub fn add(&mut self, name: &str, blueprint: ShaderBlueprint) -> &mut DynamicShader {
        self.entries.push(DynamicShader::new(name, blueprint));
        self.entries.last_mut().unwrap()
    }

    pub fn get(&self, name: &str) -> Option<&DynamicShader> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut DynamicShader> {
        self.entries.iter_mut().find(|entry| entry.name == name)
    }
}

pub fn rebuild_dynamic_shaders(
    mut registry: ResMut<DynamicShaderRegistry>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    for entry in &mut registry.entries {
        if !entry.dirty {
            continue;
        }
        let composed = entry.compose();
        let shader = Shader::from_wesl(composed.source, entry.virtual_path.clone());
        let _ = shaders.insert(entry.shader.id(), shader);
        entry.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_blueprint() -> ShaderBlueprint {
        ShaderBlueprint {
            name: "test",
            imports: vec!["import base::thing::thing_fn;"],
            declarations: vec!["const BASE: f32 = 1.0;"],
            shader_defs: vec![ShaderDefVal::Bool("KEEP".into(), true)],
            entry: "fn fragment() {",
            segments: vec![
                BlueprintSegment::Text("    let a = base();\n"),
                BlueprintSegment::Slot {
                    hook: ShaderHook::FoamPattern,
                    default: "    let pattern = default_pattern();\n",
                },
                BlueprintSegment::Slot {
                    hook: ShaderHook::FoamRoughness,
                    default: "    let roughness = default_roughness();\n",
                },
                BlueprintSegment::Text("}\n"),
            ],
        }
    }

    #[test]
    fn default_composition_keeps_blueprint_slots_and_hoists_imports() {
        let composed = compose_wesl(&test_blueprint(), &[]);
        assert!(composed.source.starts_with("import base::thing::thing_fn;"));
        assert!(composed.source.contains("let pattern = default_pattern();"));
        assert!(composed.source.contains("let roughness = default_roughness();"));
        assert!(composed.shader_defs.contains(&ShaderDefVal::Bool("KEEP".into(), true)));
    }

    #[test]
    fn modifier_imports_are_hoisted_and_deduplicated() {
        let mut modification = ShaderModification::new("wake");
        modification.imports = vec![
            "import base::thing::thing_fn;".to_string(),
            "import extra::noise::wake_noise;".to_string(),
        ];
        let composed = compose_wesl(&test_blueprint(), &[modification]);
        let head = composed.source.split("const BASE").next().unwrap();
        assert_eq!(head.matches("import base::thing::thing_fn;").count(), 1);
        assert!(head.contains("import extra::noise::wake_noise;"));
        assert!(head.lines().all(|line| line.is_empty() || line.starts_with("import")));
    }

    #[test]
    fn appends_are_ordered_by_priority_then_id() {
        let mut low = ShaderModification::new("b_low");
        low.priority = 1;
        low.hooks = vec![ShaderHookPoint::append(
            ShaderHook::FoamRoughness,
            "    low();\n",
        )];
        let mut high = ShaderModification::new("a_high");
        high.priority = 5;
        high.hooks = vec![ShaderHookPoint::append(
            ShaderHook::FoamRoughness,
            "    high();\n",
        )];
        let composed = compose_wesl(&test_blueprint(), &[low, high]);
        let roughness = composed
            .source
            .split("let roughness")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap();
        assert!(
            roughness.find("low();").unwrap() < roughness.find("high();").unwrap(),
            "appends must follow (priority, id) order: {roughness}"
        );
    }

    #[test]
    fn replace_overrides_only_its_own_slot() {
        let mut modification = ShaderModification::new("checker");
        modification.hooks = vec![ShaderHookPoint::replace(
            ShaderHook::FoamPattern,
            "    let pattern = checker_pattern();\n",
        )];
        let composed = compose_wesl(&test_blueprint(), &[modification]);
        assert!(composed.source.contains("let pattern = checker_pattern();"));
        assert!(!composed.source.contains("default_pattern"));
        assert!(composed.source.contains("let roughness = default_roughness();"));
    }

    #[test]
    fn highest_priority_replacement_wins() {
        let mut weak = ShaderModification::new("weak");
        weak.priority = 1;
        weak.hooks = vec![ShaderHookPoint::replace(
            ShaderHook::FoamPattern,
            "    let pattern = weak_pattern();\n",
        )];
        let mut strong = ShaderModification::new("strong");
        strong.priority = 2;
        strong.hooks = vec![ShaderHookPoint::replace(
            ShaderHook::FoamPattern,
            "    let pattern = strong_pattern();\n",
        )];
        let composed = compose_wesl(&test_blueprint(), &[weak, strong]);
        assert!(composed.source.contains("strong_pattern"));
        assert!(!composed.source.contains("weak_pattern"));
    }

    #[test]
    fn disabled_modifications_are_ignored() {
        let mut modification = ShaderModification::new("off");
        modification.enabled = false;
        modification.imports = vec!["import gone::thing::gone_fn;".to_string()];
        modification.hooks = vec![ShaderHookPoint::replace(
            ShaderHook::FoamPattern,
            "    let pattern = gone();\n",
        )];
        let composed = compose_wesl(&test_blueprint(), &[modification]);
        assert!(!composed.source.contains("gone"));
        assert!(composed.source.contains("default_pattern"));
    }

    #[test]
    fn shader_defs_merge_by_name_with_later_winning() {
        let mut modification = ShaderModification::new("defs");
        modification.priority = 1;
        modification.shader_defs = vec![
            ShaderDefVal::Bool("KEEP".into(), false),
            ShaderDefVal::UInt("STEPS".into(), 6),
        ];
        let composed = compose_wesl(&test_blueprint(), &[modification]);
        assert!(composed.shader_defs.contains(&ShaderDefVal::Bool("KEEP".into(), false)));
        assert!(!composed.shader_defs.contains(&ShaderDefVal::Bool("KEEP".into(), true)));
        assert!(composed.shader_defs.contains(&ShaderDefVal::UInt("STEPS".into(), 6)));
    }

    #[test]
    fn water_blueprint_composes_with_default_slots() {
        let composed = compose_wesl(&water_blueprint(), &[]);
        assert!(composed.source.contains("prepass_depth(in.position, 0u)"));
        assert!(composed.source.contains("scrolling_fbm_2d(ctx.p.xz * 2.2, water.time);"));
        assert!(composed.source.contains("water_volume_in_scatter(pbr_input.V"));
        assert!(composed.source.ends_with("    return out;\n}\n"));
    }

    #[test]
    fn water_blueprint_reproduces_the_shipped_shader() {
        let shipped = include_str!("shaders/water/water.wesl");
        let composed = compose_wesl(&water_blueprint(), &[]).source;
        let glue = |source: &str| -> String { source.split_whitespace().collect() };
        assert_eq!(glue(&composed), glue(shipped));
    }

    #[test]
    fn wake_trail_swaps_pattern_without_touching_roughness() {
        let mut wake = ShaderModification::new("wake");
        wake.imports = vec!["import package::extra::wake::wake_pattern;".to_string()];
        wake.hooks = vec![
            ShaderHookPoint::replace(
                ShaderHook::FoamPattern,
                "        ctx.pattern = wake_pattern(ctx.p.xz, water.time);\n",
            ),
            ShaderHookPoint::append(
                ShaderHook::FoamEdge,
                "        ctx.edge = max(ctx.edge, wake_edge(ctx.p));\n",
            ),
        ];
        let composed = compose_wesl(&water_blueprint(), &[wake]);
        assert!(composed.source.contains("wake_pattern(ctx.p.xz, water.time)"));
        assert!(!composed.source.contains("ctx.pattern = scrolling_fbm_2d"));
        assert!(composed.source.contains("wake_edge(ctx.p)"));
        assert!(composed.source.contains("apply_foam_roughness(pbr_input.material.perceptual_roughness, ctx.foam);"));
    }
}

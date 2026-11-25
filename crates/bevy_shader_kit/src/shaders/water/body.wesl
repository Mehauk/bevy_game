import package::shaders::sdf::volume::SdfVolume;

@group(constants::MATERIAL_BIND_GROUP) @binding(103)
var<uniform> body_volume: SdfVolume;

@group(constants::MATERIAL_BIND_GROUP) @binding(104)
var body_sdf: texture_3d<f32>;

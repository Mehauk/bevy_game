import package::shaders::water::swell_types::SwellSet;

@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var<uniform> swells: SwellSet;

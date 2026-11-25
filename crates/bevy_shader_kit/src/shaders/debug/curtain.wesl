import bevy_pbr::render::forward_io::{VertexOutput, FragmentOutput};

struct DebugCurtainParams {
    alpha: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> curtain: DebugCurtainParams;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    var tint = vec4<f32>(1.0);
    @if(VERTEX_COLORS) {
        tint = in.color;
    }
    var fade = 1.0;
    @if(VERTEX_UVS_A) {
        fade = 1.0 - clamp(in.uv.y, 0.0, 1.0);
    }
    out.color = vec4<f32>(tint.rgb, tint.a * curtain.alpha * fade);
    return out;
}

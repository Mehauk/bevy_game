import bevy_pbr::render::mesh_view_bindings::view;

fn refracted_uv(slope: vec2<f32>, strength: f32, view_z: f32) -> vec2<f32> {
    let side = (view.world_from_view * vec4<f32>(slope.x, 0.0, slope.y, 0.0)).xyz;
    return side.xy * strength / max(view_z, 1.0e-3);
}

fn refracted_probe(frag_coord: vec4<f32>, offset: vec2<f32>) -> vec4<f32> {
    var probe = frag_coord;
    probe.x = probe.x + offset.x * view.viewport.z * 0.5;
    probe.y = probe.y - offset.y * view.viewport.w * 0.5;
    return probe;
}

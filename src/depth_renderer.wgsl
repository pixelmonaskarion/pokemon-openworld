@group(0) @binding(0)
var t_shadow: texture_depth_2d;
@group(1) @binding(0)
var t_depth: texture_depth_2d;

struct ScreenInfo {
    screen_size: vec2f,
    time: f32,
}

@group(2) @binding(0)
var<uniform> screen_info: ScreenInfo;

struct Camera {
    projection: mat4x4<f32>,
    inverse: mat4x4<f32>,
}

@group(3) @binding(0) var<uniform> camera: Camera;

@group(4) @binding(0) var<uniform> sun_camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 1.0);
    out.tex_coords = model.tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    //get screen depth
    let depth = textureLoad(t_depth, vec2<u32>(u32(in.tex_coords.x*screen_info.screen_size.x), u32(in.tex_coords.y*screen_info.screen_size.y)), 0);
    //translate to world pos
    let clipPos = vec4(in.tex_coords.x * 2.0 - 1.0, in.tex_coords.y * -2.0 + 1.0, depth, 1.0);
    let viewPos = camera.inverse * clipPos;
    let worldPos = viewPos.xyz / viewPos.w;
    //translate to shadow texture pos
    let shadowPos = sun_camera.projection * vec4f(worldPos, 1.0);
    let shadowScreenPos = vec2f((shadowPos.x+1.0)/2.0, (shadowPos.x-1.0)/-2.0);
    //get shadow depth
    let shadow_depth = textureLoad(t_shadow, vec2<u32>(u32(shadowScreenPos.x*screen_info.screen_size.x), u32(shadowScreenPos.y*screen_info.screen_size.y)), 0);
    return vec4f(shadow_depth);
}
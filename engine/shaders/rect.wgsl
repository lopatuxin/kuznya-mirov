// Shared by two passes that draw the same kind of instanced rectangle in different coordinate
// spaces: the world pass maps scene cells through the letterboxed viewport, the interface pass
// maps window pixels straight onto the canvas. `scale`/`offset` carry the difference; see
// `render::Renderer::world_globals` / `ui_globals`.
struct Globals {
    scale: vec2<f32>,
    offset: vec2<f32>,
    canvas_size_px: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct VertexInput {
    @location(0) unit: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let corner = instance.position + vertex.unit * instance.size;
    let px = globals.offset + corner * globals.scale;
    let ndc_x = (px.x / globals.canvas_size_px.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (px.y / globals.canvas_size_px.y) * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}

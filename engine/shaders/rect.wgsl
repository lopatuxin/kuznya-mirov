// Shared by two passes that draw the same kind of instanced rectangle in different coordinate
// spaces: the world pass maps scene cells through the letterboxed viewport, the interface pass
// maps window pixels straight onto the canvas. `scale`/`offset` carry the difference; see
// `render::gpu::Renderer::world_globals` / `ui_globals`.
struct Globals {
    scale: vec2<f32>,
    offset: vec2<f32>,
    canvas_size_px: vec2<f32>,
    _padding: vec2<f32>,
};

// «Картинки» → «Атлас и отрисовка»: one shared atlas for both passes — a plain color fill and an
// image go out through the same texture, same sampler, same instance layout.
const ATLAS_SIZE: f32 = 2048.0;

@group(0) @binding(0)
var<uniform> globals: Globals;
@group(0) @binding(1)
var atlas_texture: texture_2d<f32>;
@group(0) @binding(2)
var atlas_sampler: sampler;

struct VertexInput {
    @location(0) unit: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    // In atlas pixels — a plain color fill uses `atlas::WHITE_PIXEL` here (1×1, opaque white),
    // stretched over the whole rectangle exactly like a real image would be.
    @location(4) atlas_pos: vec2<f32>,
    @location(5) atlas_size: vec2<f32>,
    // «Картинки», требование 24: quarter turns (0–3) clockwise; a color fill always gets 0.
    @location(6) rotation_quarters: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let corner = instance.position + vertex.unit * instance.size;
    let px = globals.offset + corner * globals.scale;
    let ndc_x = (px.x / globals.canvas_size_px.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (px.y / globals.canvas_size_px.y) * 2.0;

    // «Картинки», требование 24: rotates the *sampled* corner by `rotation_quarters` clockwise
    // before it stretches to fill the drawn rectangle — the rectangle itself never changes
    // shape, only which part of the atlas lands where. Derived by tracing each quarter turn's
    // corner-to-corner mapping once and folding it into one integer-indexed formula: `k=1`
    // samples `(y, 1-x)`, `k=2` samples `(1-x, 1-y)`, `k=3` samples `(1-y, x)`.
    let k = i32(instance.rotation_quarters + 0.5);
    var sample_unit = vertex.unit;
    if (k == 1) {
        sample_unit = vec2<f32>(vertex.unit.y, 1.0 - vertex.unit.x);
    } else if (k == 2) {
        sample_unit = vec2<f32>(1.0 - vertex.unit.x, 1.0 - vertex.unit.y);
    } else if (k == 3) {
        sample_unit = vec2<f32>(1.0 - vertex.unit.y, vertex.unit.x);
    }

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;
    out.uv = (instance.atlas_pos + sample_unit * instance.atlas_size) / ATLAS_SIZE;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color * textureSample(atlas_texture, atlas_sampler, in.uv);
}

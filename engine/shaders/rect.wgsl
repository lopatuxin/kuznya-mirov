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

// «Картинки» → «Атлас»: one shared atlas for both passes — a plain color fill and an image go
// out through the same texture (a stack of `ATLAS_SIZE`×`ATLAS_SIZE` sheets, one array layer
// each), same sampler, same instance layout.
const ATLAS_SIZE: f32 = 2048.0;

@group(0) @binding(0)
var<uniform> globals: Globals;
@group(0) @binding(1)
var atlas_texture: texture_2d_array<f32>;
// «Картинки» → «Сглаживание»: naga's GLSL writer (WebGL2) rejects sampling one texture through
// two samplers (`ImageMultipleSamplers` — a texture and its sampler are one GLSL object), so the
// crisp path uses `textureLoad` (texel fetch, no sampler) instead of a second one — `fs_main`
// computes both paths unconditionally and picks with `select`, never a branch.
@group(0) @binding(2)
var atlas_sampler_linear: sampler;

struct VertexInput {
    @location(0) unit: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    // In that sheet's own pixels — a plain color fill uses `atlas::WHITE_PIXEL` here (1×1, opaque
    // white, sheet 0), stretched over the whole rectangle exactly like a real image would be.
    @location(4) atlas_pos: vec2<f32>,
    @location(5) atlas_size: vec2<f32>,
    // «Картинки», требование 24: quarter turns (0–3) clockwise; a color fill always gets 0.
    @location(6) rotation_quarters: f32,
    // «Картинки» → «Атлас», требование 14: which array layer of `atlas_texture` to sample.
    @location(7) atlas_layer: f32,
    // «Картинки» → «Сглаживание»: 0.0/1.0 — `atlas::AtlasImage`'s own `smooth`, a color fill
    // always gets 0.0 (doesn't matter: `WHITE_PIXEL` samples the same either way). Named
    // `smooth_flag`, not `smooth` — `smooth` is a WGSL reserved word.
    @location(8) smooth_flag: f32,
    // «Картинки» → «Отражение», требование 9: 0.0/1.0 — mirrors the sampled point in the image's
    // own axes, after `rotation_quarters` above has already picked which corner maps where.
    @location(9) flip_x: f32,
    // «Картинки», «Таблица картинок»: 0.0/1.0 — светящаяся картинка: фрагмент отдаёт `(цвет, 0)`, и
    // смешивание `PREMULTIPLIED_ALPHA_BLENDING` прибавляет цвет к тому, что под ней.
    @location(10) glow_flag: f32,
    // «Ветер и частицы» → «Качание», требование 14: на сколько клеток вбок ушёл верх рисунка.
    @location(11) lean: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv_px: vec2<f32>,
    @location(2) @interpolate(flat) uv_min: vec2<f32>,
    @location(3) @interpolate(flat) uv_max: vec2<f32>,
    @location(4) @interpolate(flat) atlas_layer: i32,
    @location(5) @interpolate(flat) smooth_sample: f32,
    @location(6) @interpolate(flat) glow: f32,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    // «Ветер и частицы» → «Качание», требование 14: нижний край стоит; точка на высоте `y` над ним
    // (доля `s` высоты прямоугольника) уходит вбок на `lean · s²` и встаёт над своим местом на нижнем
    // крае на `√(y² − сдвиг²)` — идёт по дуге и не растягивается. Опускание `y − √(y² − сдвиг²)`
    // записано как `сдвиг² / (y + √(y² − сдвиг²))`: без вычитания близких чисел, и без наклона оно
    // ровно ноль — углы там же, где были.
    let along = 1.0 - vertex.unit.y;
    let above = along * instance.size.y;
    let shift = instance.lean * along * along;
    let reach = sqrt(max(above * above - shift * shift, 0.0));
    let drop = shift * shift / max(above + reach, 0.000001);
    let corner = instance.position + vertex.unit * instance.size + vec2<f32>(shift, drop);
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

    // «Картинки» → «Отражение», требование 9: mirrors the sampled point in the image's own axes,
    // after the rotation mapping above has already picked which corner maps where.
    if (instance.flip_x > 0.5) {
        sample_unit = vec2<f32>(1.0 - sample_unit.x, sample_unit.y);
    }

    // «Картинки», требование 23: the fragment shader clamps the sampled point to this rect's
    // outermost texel centers, so floating-point error from the transform above never crosses
    // into a neighboring frame, grid cell or ground tile; the mapping itself stays linear, so
    // every texel keeps its full on-screen size.
    let sample_px = instance.atlas_pos + sample_unit * instance.atlas_size;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    // «Картинки» → «Сглаживание», требование 5: the atlas's own points are already
    // color-times-alpha (`atlas::blit`) and the pipeline blends with
    // `PREMULTIPLIED_ALPHA_BLENDING` (`render::gpu`), which expects the same of the fragment's
    // whole output — premultiplied here so a color fill's own alpha (opacity, translucent panels)
    // does too, not just the sampled texel.
    out.color = vec4<f32>(instance.color.rgb * instance.color.a, instance.color.a);
    out.uv_px = sample_px;
    out.uv_min = instance.atlas_pos + vec2<f32>(0.5, 0.5);
    out.uv_max = max(instance.atlas_pos + instance.atlas_size - vec2<f32>(0.5, 0.5), out.uv_min);
    out.atlas_layer = i32(instance.atlas_layer + 0.5);
    out.smooth_sample = instance.smooth_flag;
    out.glow = instance.glow_flag;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let clamped = clamp(in.uv_px, in.uv_min, in.uv_max);
    let uv = clamped / ATLAS_SIZE;
    // «Картинки» → «Сглаживание»: both paths run unconditionally and `select` picks the result —
    // a texture sample needs the same control-flow path for every fragment in a quad, so the
    // choice can't be a branch. The crisp path is a texel fetch, not a second sampler (see the
    // binding comment above).
    let nearest = textureLoad(atlas_texture, vec2<i32>(floor(clamped)), in.atlas_layer, 0);
    let linear = textureSample(atlas_texture, atlas_sampler_linear, uv, in.atlas_layer);
    let sampled = select(nearest, linear, in.smooth_sample > 0.5);
    let painted = in.color * sampled;
    return vec4<f32>(painted.rgb, painted.a * (1.0 - in.glow));
}

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
    // «Ветер и частицы» → «Частица», требование 13: поворот вокруг середины прямоугольника на любой
    // угол, градусов по часовой стрелке; 0 — прямоугольник стоит, как стоял.
    @location(12) angle: f32,
    // «Огонь»: 0.0/1.0 — экземпляр рисует пламя по формуле, а не картинку из атласа.
    @location(13) flame_flag: f32,
    // «Огонь»: сила, зерно узора, целая и дробная части хода узора; нули у всего, кроме пламени.
    @location(14) fire: vec4<f32>,
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
    // «Огонь»: точка прямоугольника в долях ширины и высоты; параметры огня; ширина и высота в клетках и
    // признак пламени.
    @location(7) unit: vec2<f32>,
    @location(8) @interpolate(flat) fire: vec4<f32>,
    @location(9) @interpolate(flat) flame: vec3<f32>,
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
    let straight = instance.position + vertex.unit * instance.size + vec2<f32>(shift, drop);
    // Поворот частицы по часовой стрелке (`y` вниз); без угла `select` отдаёт угол прямоугольника как
    // был, не пересчитанный через середину.
    let radians = instance.angle * 0.017453292519943295;
    let middle = instance.position + instance.size * 0.5;
    let from_middle = straight - middle;
    let turned = middle + vec2<f32>(
        from_middle.x * cos(radians) - from_middle.y * sin(radians),
        from_middle.x * sin(radians) + from_middle.y * cos(radians),
    );
    let corner = select(straight, turned, instance.angle != 0.0);
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
    out.unit = vertex.unit;
    out.fire = instance.fire;
    out.flame = vec3<f32>(instance.size, instance.flame_flag);
    return out;
}

// «Огонь»: числа вида пламени — блоком здесь; высота, дыхание, ветер, разгорание и ореол — в
// `render/fire.rs`. Пламя — ряд языков по ширине: язык стоит в своей колонке шириной `FLAME_SPACING`
// клеток, его высота во времени — шум, крайние языки ниже средних, к верху язык сужается до острия.
// Языки качает вбок медленный узор, края рвёт мелкий, оба поднимаются вверх; низ пламени мягко уходит
// в то, на чём оно стоит, неровной линией.
const FLAME_SPACING: f32 = 0.17;
// Насколько язык сдвинут от середины своей колонки, доля её ширины.
const FLAME_JITTER: f32 = 0.5;
// Полуширина языка у основания — доля ширины колонки, у каждого языка своя между этими краями.
const FLAME_HALF_MIN: f32 = 0.8;
const FLAME_HALF_MAX: f32 = 1.1;
// Самый низкий язык — доля высоты пламени; крайние языки ниже средних на эту долю.
const FLAME_LOWEST: f32 = 0.4;
const FLAME_CROWN: f32 = 0.35;
// Сколько раз меняется высота языка, пока узор проходит одну клетку хода.
const FLAME_TONGUE_RATE: i32 = 2;
// Как язык сужается к верху (меньше — пузатее) и как к верху падает его «глубина».
const FLAME_NARROW: f32 = 0.75;
const FLAME_FADE: f32 = 0.8;
// Узор, который качает языки: узлов на клетку по ширине и высоте, на сколько узлов поднимается за клетку
// хода, и на сколько клеток уводит верх языка.
const FLAME_LICK_SCALE: vec2<f32> = vec2<f32>(3.0, 2.5);
const FLAME_LICK_RISE: i32 = 1;
const FLAME_LICK: f32 = 0.09;
// Мелкий узор, который рвёт края языков, — сильнее у верха и у бушующего огня.
const FLAME_RAG_SCALE: vec2<f32> = vec2<f32>(9.0, 6.0);
const FLAME_RAG_RISE: i32 = 4;
const FLAME_RAG_CALM: f32 = 0.3;
const FLAME_RAG_RAGE: f32 = 0.75;
// Сила, при которой пламя занимает прямоугольник целиком; ниже оно поднимается от нижнего края.
const FLAME_GROW_UNTIL: f32 = 0.1;
// Низ пламени: на какой доле высоты оно проступает из того, на чём стоит, и как неровна эта линия —
// размах в долях высоты и узлов на клетку по ширине.
const FLAME_BASE_SOFT: f32 = 0.16;
const FLAME_BASE_WAVE: f32 = 0.1;
const FLAME_BASE_SCALE: f32 = 18.0;
// Мягкость края языка (в «глубине») и перо у боковых краёв прямоугольника (в клетках).
const FLAME_EDGE_SOFT: f32 = 0.05;
const FLAME_FEATHER_CELLS: f32 = 0.04;
// Цветовые полосы по «глубине»: сердцевина глубже `FLAME_CORE_AT`, середина глубже `FLAME_MID_AT`, ниже —
// кончик; `FLAME_BAND_SOFT` — узкая мягкая граница. Сердцевина светлее цвета огня в сторону белого на
// `FLAME_CORE_WHITE`, кончик — это цвет, умноженный на `FLAME_TIP_DARK`.
const FLAME_MID_AT: f32 = 0.2;
const FLAME_CORE_AT: f32 = 0.58;
const FLAME_BAND_SOFT: f32 = 0.035;
const FLAME_CORE_WHITE: f32 = 0.5;
const FLAME_TIP_DARK: f32 = 0.45;

// Число от 0 до 1 в узле решётки `cell` у огня с зерном `seed`: целочисленный хеш, поэтому узор не
// рассыпается, как бы далеко он ни ушёл.
fn flame_cell(cell: vec2<i32>, seed: u32) -> f32 {
    var h = bitcast<u32>(cell.x) * 0x27d4eb2fu + bitcast<u32>(cell.y) * 0x165667b1u + seed * 0x9e3779b1u;
    h = (h ^ (h >> 15u)) * 0x85ebca6bu;
    h = (h ^ (h >> 13u)) * 0xc2b2ae35u;
    h = h ^ (h >> 16u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

// Сглаженный шум решётки в точке `p` (клетки сцены от левого нижнего угла пламени), поднимающийся на
// `rise` узлов за клетку хода узора. Ход `whole + fraction` переведён в узлы отдельно от дробной части,
// чтобы целая часть считалась в целых числах и точности хватало после часов горения.
fn flame_layer(p: vec2<f32>, scale: vec2<f32>, rise: i32, whole: i32, fraction: f32, seed: u32) -> f32 {
    let q = vec2<f32>(p.x * scale.x, p.y * scale.y - f32(rise) * fraction);
    let base = floor(q);
    let t = q - base;
    let smooth_t = t * t * (3.0 - 2.0 * t);
    let cell = vec2<i32>(i32(base.x), i32(base.y) - rise * whole);
    let a = flame_cell(cell, seed);
    let b = flame_cell(cell + vec2<i32>(1, 0), seed);
    let c = flame_cell(cell + vec2<i32>(0, 1), seed);
    let d = flame_cell(cell + vec2<i32>(1, 1), seed);
    return mix(mix(a, b, smooth_t.x), mix(c, d, smooth_t.x), smooth_t.y);
}

// Высота языка колонки `column` от 0 до 1 в этот миг: шум по узлам времени, целая часть хода отдельно.
fn flame_tongue_height(column: i32, whole: i32, fraction: f32, seed: u32) -> f32 {
    let t = f32(FLAME_TONGUE_RATE) * fraction;
    let node = floor(t);
    let s = t - node;
    let eased = s * s * (3.0 - 2.0 * s);
    let at = FLAME_TONGUE_RATE * whole + i32(node);
    return mix(flame_cell(vec2<i32>(column, at), seed + 7u), flame_cell(vec2<i32>(column, at + 1), seed + 7u), eased);
}

// Цвет пламени в точке: «глубина» — 1 у основания посередине языка, 0 на его краю и острие; узор рвёт её,
// полосы цвета экземпляра красят её. Результат — свет, который прибавляется к тому, что под ним.
fn flame_color(in: VertexOutput) -> vec4<f32> {
    let strength = in.fire.x;
    let seed = u32(in.fire.y);
    let whole = i32(in.fire.z);
    let fraction = in.fire.w;
    let size = in.flame.xy;
    let up = 1.0 - in.unit.y;
    let x = in.unit.x * size.x;
    let grow = clamp(strength / FLAME_GROW_UNTIL, 0.0, 1.0);
    let rise = up / max(grow, 0.001);
    let spot = vec2<f32>(x, up * size.y);

    let lick = (flame_layer(spot, FLAME_LICK_SCALE, FLAME_LICK_RISE, whole, fraction, seed) - 0.5) * 2.0 * FLAME_LICK * rise;
    let swayed = x + lick;
    let column = i32(floor(swayed / FLAME_SPACING));
    var depth = -1.0;
    for (var k = -1; k <= 1; k++) {
        let i = column + k;
        let center = (f32(i) + 0.5 + (flame_cell(vec2<i32>(i, 1), seed) - 0.5) * FLAME_JITTER) * FLAME_SPACING;
        let half_width = FLAME_SPACING * mix(FLAME_HALF_MIN, FLAME_HALF_MAX, flame_cell(vec2<i32>(i, 2), seed));
        let across = clamp(center / size.x, 0.0, 1.0) * 2.0 - 1.0;
        let tall = mix(FLAME_LOWEST, 1.0, flame_tongue_height(i, whole, fraction, seed)) * (1.0 - FLAME_CROWN * across * across);
        let left = 1.0 - rise / tall;
        if (left > 0.0) {
            let q = abs(swayed - center) / (half_width * pow(left, FLAME_NARROW));
            if (q < 1.0) {
                depth = max(depth, (1.0 - q) * pow(left, FLAME_FADE));
            }
        }
    }
    let rag = flame_layer(spot, FLAME_RAG_SCALE, FLAME_RAG_RISE, whole, fraction, seed + 3u) - 0.5;
    depth = depth - rag * mix(FLAME_RAG_CALM, FLAME_RAG_RAGE, strength) * rise;
    let ember = flame_layer(vec2<f32>(x, 0.0), vec2<f32>(FLAME_BASE_SCALE, 1.0), 0, whole, fraction, seed + 5u) * FLAME_BASE_WAVE;
    let edge_cells = min(x, size.x - x);
    let coverage = smoothstep(0.0, FLAME_EDGE_SOFT, depth)
        * smoothstep(0.0, FLAME_FEATHER_CELLS, edge_cells)
        * smoothstep(0.0, FLAME_BASE_SOFT, up - ember);

    let tint = in.color.rgb / max(in.color.a, 0.000001);
    let core = mix(tint, vec3<f32>(1.0, 1.0, 1.0), FLAME_CORE_WHITE);
    let tip = tint * FLAME_TIP_DARK;
    var hue = mix(tip, tint, smoothstep(FLAME_MID_AT - FLAME_BAND_SOFT, FLAME_MID_AT + FLAME_BAND_SOFT, depth));
    hue = mix(hue, core, smoothstep(FLAME_CORE_AT - FLAME_BAND_SOFT, FLAME_CORE_AT + FLAME_BAND_SOFT, depth));
    return vec4<f32>(hue * coverage * in.color.a, 0.0);
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
    if (in.flame.z > 0.5) {
        return flame_color(in);
    }
    return vec4<f32>(painted.rgb, painted.a * (1.0 - in.glow));
}

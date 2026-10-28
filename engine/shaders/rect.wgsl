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
    // «Ветер и частицы» → «Качание», требование 14: на сколько клеток вбок ушёл верх рисунка (x).
    // «Ветер и частицы» → «Частица», требование 13: поворот вокруг середины прямоугольника на любой
    // угол, градусов по часовой стрелке; 0 — прямоугольник стоит, как стоял (y).
    @location(11) lean_angle: vec2<f32>,
    // «Картинки» → «Атлас»: доли высоты рисунка, отрезанные от него снизу (x) и сверху (y), когда
    // прямоугольник — только кусок кадра без прозрачной каймы; наклон считается от низа и высоты
    // рисунка целиком. `(0, 0)` — прямоугольник и есть рисунок.
    @location(12) lean_cut: vec2<f32>,
    // «Огонь»: 0.0/1.0/2.0 — экземпляр рисует пламя (1.0) или угли (2.0) по формуле, а не картинку из атласа.
    @location(13) flame_flag: f32,
    // «Огонь»: сила (у углей — накал), зерно узора, целая и дробная части хода узора; нули у всего, кроме
    // пламени и углей.
    @location(14) fire: vec4<f32>,
    // «Картинки» → «Растягивание рамки»: `[x, y, ширина, высота]` кадра в точках листа — выборка
    // прижимается к его краю, а не к краю `atlas_pos`/`atlas_size`: у куска рамки сглаживание на стыке
    // берёт точки соседнего места той же картинки. У всего, кроме кусков рамки, совпадает с ними.
    @location(15) clamp_rect: vec4<f32>,
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
    // признак пламени или углей.
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
    let drawn_share = 1.0 - instance.lean_cut.x - instance.lean_cut.y;
    let along = instance.lean_cut.x + (1.0 - vertex.unit.y) * drawn_share;
    let above = along * instance.size.y / drawn_share;
    let shift = instance.lean_angle.x * along * along;
    let reach = sqrt(max(above * above - shift * shift, 0.0));
    let drop = shift * shift / max(above + reach, 0.000001);
    let straight = instance.position + vertex.unit * instance.size + vec2<f32>(shift, drop);
    // Поворот частицы по часовой стрелке (`y` вниз); без угла `select` отдаёт угол прямоугольника как
    // был, не пересчитанный через середину.
    let radians = instance.lean_angle.y * 0.017453292519943295;
    let middle = instance.position + instance.size * 0.5;
    let from_middle = straight - middle;
    let turned = middle + vec2<f32>(
        from_middle.x * cos(radians) - from_middle.y * sin(radians),
        from_middle.x * sin(radians) + from_middle.y * cos(radians),
    );
    let corner = select(straight, turned, instance.lean_angle.y != 0.0);
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
    out.uv_min = instance.clamp_rect.xy + vec2<f32>(0.5, 0.5);
    out.uv_max = max(instance.clamp_rect.xy + instance.clamp_rect.zw - vec2<f32>(0.5, 0.5), out.uv_min);
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

// «Угли»: числа вида кучи — блоком здесь; высота кучи, накал, остывание, порыв и ход переливания — в
// `render/fire.rs`. Куча стоит на нижнем крае прямоугольника, во всю его высоту посередине; к краям она
// ниже. Куски — круглые и чуть приплюснутые, разной величины, вразброс; узор слегка кривит их; внизу к
// крупным кускам добавляются мелкие — там куски мельче и лежат плотнее. Кусок есть, если его середина
// под линией кучи, и верх кучи — макушки таких кусков: линия опущена так, что макушка самого большого
// куска посередине, даже сдвинутая узором, не выходит за верх прямоугольника. Щели между кусками внутри кучи получают весь жар,
// края кусков — меньше, середины и макушки — ещё меньше; жар растёт к низу кучи и к середине по ширине,
// а по куче медленно переливается. Цвет жара — полосами, как у пламени; ниже порога — холодный кусок,
// почти чёрный с тёплым отливом.
// Линия кучи у боковых краёв — доля линии посередине; шаг кусков — доля высоты кучи; куски по высоте
// ниже, чем по ширине; насколько середина куска сдвинута в своей ячейке, доля ячейки; радиус куска —
// доля шага; шаг мелких кусков — доля шага; мелкие лежат ниже этой доли линии кучи; насколько узор
// кривит куски (доля шага) и его мелкость (пятен на шаг); щели светятся ниже линии кучи на эту долю
// шага; перо краёв (клетки).
const COALS_SIDE: f32 = 0.15;
const COALS_STEP: f32 = 0.3;
const COALS_FLAT: f32 = 0.85;
const COALS_JITTER: f32 = 1.0;
const COALS_RADIUS_MIN: f32 = 0.5;
const COALS_RADIUS_MAX: f32 = 0.85;
const COALS_FINE: f32 = 0.85;
const COALS_FINE_BELOW: f32 = 0.45;
const COALS_WARP: f32 = 0.5;
const COALS_WARP_SCALE: f32 = 1.3;
const COALS_GAP_BELOW: f32 = 0.35;
const COALS_EDGE_SOFT: f32 = 0.008;
// Насколько макушка самого большого куска, сдвинутая узором, выше его середины, доля высоты кучи: на
// столько опущена линия.
const COALS_CROWN: f32 = COALS_STEP * (COALS_RADIUS_MAX * COALS_FLAT + COALS_WARP * 0.5);
// Жар: накал в этой степени — слабый накал тлеет тёмным; доля у верха кучи (у низа — вся); насколько
// меньше у боковых краёв; ширина светлого края куска у щели (доля радиуса куска); край у макушки — эта доля
// края у низа; середина куска — доля жара щели; макушка куска — доля его низа; размер пятна переливания —
// доля высоты кучи; размах переливания.
const COALS_HEAT_POWER: f32 = 1.2;
const COALS_TOP_HEAT: f32 = 0.55;
const COALS_SIDE_FALL: f32 = 0.45;
const COALS_RIM: f32 = 0.6;
const COALS_RIM_TOP: f32 = 0.5;
const COALS_INNER: f32 = 0.35;
const COALS_UPPER: f32 = 0.4;
const COALS_SPOT: f32 = 1.0;
const COALS_FLICKER_LOW: f32 = 0.7;
const COALS_FLICKER_HIGH: f32 = 1.3;
// Холодный кусок; разброс тона между кусками; щель темнее куска; светлее у макушки куска — куски
// читаются и на холодной куче.
const COALS_COLD: vec3<f32> = vec3<f32>(0.085, 0.07, 0.062);
const COALS_TONE: f32 = 0.3;
const COALS_GAP_DARK: f32 = 0.35;
const COALS_TOP_LIGHT: f32 = 0.06;
// Цветовые полосы по жару: слабый — цвет огня, умноженный на `COALS_WEAK_DARK`; средний — сам цвет;
// сильный — светлее его на `COALS_CORE_WHITE` в сторону белого. `COALS_BAND_SOFT` — узкая мягкая граница.
const COALS_WEAK_AT: f32 = 0.12;
const COALS_MID_AT: f32 = 0.36;
const COALS_CORE_AT: f32 = 0.66;
const COALS_BAND_SOFT: f32 = 0.04;
const COALS_WEAK_DARK: f32 = 0.5;
const COALS_CORE_WHITE: f32 = 0.55;

// Линия кучи над нижним краем в точке `x` у кучи шириной `width` и высотой посередине `height`: под ней
// лежат середины кусков.
fn coals_surface(x: f32, width: f32, height: f32) -> f32 {
    let across = clamp(x / max(width, 0.0001), 0.0, 1.0) * 2.0 - 1.0;
    return height * (1.0 - COALS_CROWN) * mix(COALS_SIDE, 1.0, 1.0 - across * across);
}

// Сглаженный шум решётки в точке `p` на узле времени `node`: пятна переливания.
fn coals_spot_node(p: vec2<f32>, node: i32, seed: u32) -> f32 {
    let base = floor(p);
    let t = p - base;
    let smooth_t = t * t * (3.0 - 2.0 * t);
    let cell = vec2<i32>(base);
    let moment = seed + 41u + bitcast<u32>(node) * 0x7f4a7c15u;
    let a = flame_cell(cell, moment);
    let b = flame_cell(cell + vec2<i32>(1, 0), moment);
    let c = flame_cell(cell + vec2<i32>(0, 1), moment);
    let d = flame_cell(cell + vec2<i32>(1, 1), moment);
    return mix(mix(a, b, smooth_t.x), mix(c, d, smooth_t.x), smooth_t.y);
}

// Переливание в точке `p`: пятна меняются от узла времени к узлу, целая часть хода отдельно от дробной.
fn coals_spot(p: vec2<f32>, whole: i32, fraction: f32, seed: u32) -> f32 {
    let eased = fraction * fraction * (3.0 - 2.0 * fraction);
    return mix(coals_spot_node(p, whole, seed), coals_spot_node(p, whole + 1, seed), eased);
}

// Кусок, в который точка заходит глубже всего: насколько глубоко (меньше нуля — точка в щели; −8 — рядом
// куска нет), его середина, радиус и зерно его тона.
struct CoalsPiece {
    inside: f32,
    center: vec2<f32>,
    radius: f32,
    tone_seed: f32,
};

// Ищет кусок по двум решёткам: крупные куски лежат серединой где угодно под линией кучи, мелкие — только
// ниже `COALS_FINE_BELOW` линии. Кусок, чья середина не там, отбрасывается до сравнения глубины: точку
// забирает соседний кусок, а не щель.
fn coals_piece(p: vec2<f32>, step: f32, width: f32, height: f32, seed: u32) -> CoalsPiece {
    var out = CoalsPiece(-8.0, vec2<f32>(0.0, 0.0), 0.0, 0.0);
    for (var layer = 0u; layer < 2u; layer++) {
        let fine = layer == 1u;
        let layer_step = select(step, step * COALS_FINE, fine);
        let layer_seed = seed + layer * 30u;
        let grid = vec2<f32>(layer_step, layer_step * COALS_FLAT);
        let base = vec2<i32>(floor(p / grid));
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let cell = base + vec2<i32>(i, j);
                let jitter = vec2<f32>(flame_cell(cell, layer_seed + 21u), flame_cell(cell, layer_seed + 22u)) - vec2<f32>(0.5, 0.5);
                let center = (vec2<f32>(cell) + vec2<f32>(0.5, 0.5) + jitter * COALS_JITTER) * grid;
                let line = coals_surface(center.x, width, height);
                let low = center.y <= COALS_FINE_BELOW * line;
                if (center.y > line || (fine && !low)) {
                    continue;
                }
                let radius = layer_step * mix(COALS_RADIUS_MIN, COALS_RADIUS_MAX, flame_cell(cell, layer_seed + 24u));
                let d = length(vec2<f32>(p.x - center.x, (p.y - center.y) / COALS_FLAT));
                if (radius - d > out.inside) {
                    out = CoalsPiece(radius - d, center, radius, flame_cell(cell, layer_seed + 23u));
                }
            }
        }
    }
    return out;
}

// Цвет кучи углей в точке: силуэт и куски, жар по ним, цвет жара полосами. Результат непрозрачен внутри
// кучи и прозрачен вне её.
fn coals_color(in: VertexOutput) -> vec4<f32> {
    let heat = in.fire.x;
    let seed = u32(in.fire.y);
    let whole = i32(in.fire.z);
    let fraction = in.fire.w;
    let size = in.flame.xy;
    let spot_at = vec2<f32>(in.unit.x * size.x, (1.0 - in.unit.y) * size.y);
    let step = max(COALS_STEP * size.y, 0.0001);

    let warp_scale = vec2<f32>(COALS_WARP_SCALE / step, COALS_WARP_SCALE / step);
    let warp = vec2<f32>(
        flame_layer(spot_at, warp_scale, 0, 0, 0.0, seed + 31u),
        flame_layer(spot_at, warp_scale, 0, 0, 0.0, seed + 32u),
    ) - vec2<f32>(0.5, 0.5);
    let p = spot_at + warp * COALS_WARP * step;
    let piece = coals_piece(p, step, size.x, size.y, seed);
    let line = coals_surface(spot_at.x, size.x, size.y);
    let gap = line - COALS_GAP_BELOW * step - spot_at.y;
    let cover = smoothstep(-COALS_EDGE_SOFT, COALS_EDGE_SOFT, max(piece.inside, gap))
        * smoothstep(0.0, COALS_EDGE_SOFT, min(spot_at.x, size.x - spot_at.x));
    // 1 в куске, 0 в щели, мягко на границе
    let on_piece = smoothstep(-COALS_EDGE_SOFT, COALS_EDGE_SOFT, piece.inside);

    let top = line + COALS_CROWN * size.y;
    let deep = clamp(1.0 - spot_at.y / max(top, 0.0001), 0.0, 1.0);
    let across = in.unit.x * 2.0 - 1.0;
    let middle = 1.0 - COALS_SIDE_FALL * across * across;
    let spot = coals_spot(spot_at / max(size.y * COALS_SPOT, 0.0001), whole, fraction, seed);
    let hot = pow(max(heat, 0.0), COALS_HEAT_POWER) * mix(COALS_TOP_HEAT, 1.0, deep) * middle
        * mix(COALS_FLICKER_LOW, COALS_FLICKER_HIGH, spot);
    let rim = 1.0 - smoothstep(0.0, max(COALS_RIM * piece.radius, 0.0001), piece.inside);
    let below = clamp(0.5 + (piece.center.y - p.y) / max(piece.radius * 2.0 * COALS_FLAT, 0.0001), 0.0, 1.0);
    let exposure = mix(COALS_INNER * mix(COALS_UPPER, 1.0, below), 1.0, rim * mix(COALS_RIM_TOP, 1.0, below));
    let glow = hot * mix(1.0, exposure, on_piece);

    let tone = mix(1.0 - COALS_TONE, 1.0 + COALS_TONE, piece.tone_seed);
    let lit = COALS_TOP_LIGHT * (1.0 - below) * (1.0 - rim);
    var hue = mix(COALS_COLD * COALS_GAP_DARK, COALS_COLD * tone + vec3<f32>(lit, lit, lit), on_piece);
    let tint = in.color.rgb / max(in.color.a, 0.000001);
    let core = mix(tint, vec3<f32>(1.0, 1.0, 1.0), COALS_CORE_WHITE);
    hue = mix(hue, tint * COALS_WEAK_DARK, smoothstep(COALS_WEAK_AT - COALS_BAND_SOFT, COALS_WEAK_AT + COALS_BAND_SOFT, glow));
    hue = mix(hue, tint, smoothstep(COALS_MID_AT - COALS_BAND_SOFT, COALS_MID_AT + COALS_BAND_SOFT, glow));
    hue = mix(hue, core, smoothstep(COALS_CORE_AT - COALS_BAND_SOFT, COALS_CORE_AT + COALS_BAND_SOFT, glow));
    return vec4<f32>(hue * cover, cover);
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
    if (in.flame.z > 1.5) {
        return coals_color(in);
    }
    if (in.flame.z > 0.5) {
        return flame_color(in);
    }
    return vec4<f32>(painted.rgb, painted.a * (1.0 - in.glow));
}

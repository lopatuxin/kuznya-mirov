// «Трёхмерная сцена», «Свет и материалы»: рельеф с покрытиями и мокрым берегом, вода, фигуры, плитки и
// плоские объекты на земле и карта теней от солнца. Проходы: тени (`vs_shadow` — фигуры,
// `vs_shadow_terrain` — рельеф; только глубина от солнца), затем рельеф (`vs_terrain`, `fs_terrain`),
// фигуры (`vs_shape`, `fs_shape`), модели (`vs_model`; части с прозрачностью `OPAQUE` — `fs_model`, `MASK` —
// `fs_model_mask`, `BLEND` и куски дорог — `fs_model_blend`; в тенях — `vs_shadow_model` и для `MASK`
// `vs_shadow_model_mask` с `fs_shadow_model_mask`), перекрёстки дорог (изогнутые копии частей моделей:
// `vs_junction` и `fs_model_blend`; в тени не рисуются), плитки с плоскими объектами (`vs_ground`, `fs_ground`),
// которые лежат на рельефе и верху настилов, и после них вода (`vs_water`, `fs_water`): она смешивается с
// нарисованным и пишет глубину.
// Надписи, полоски и интерфейс рисует `rect.wgsl` отдельным проходом без глубины.
//
// Свет считается в линейной яркости, кадр сводится к экрану кривой Khronos PBR Neutral и кодируется в
// sRGB в конце каждого фрагментного шейдера.
//
// Ловушки WebGL2: одна текстура — одна выборка, поэтому атлас читается тем же способом, что в
// `rect.wgsl` (точка и линейная выборка, выбор через `select`), а карта теней — только выборкой
// сравнением. Выборки внутри ветвлений — только с явным уровнем или производными, а сами производные
// (`dpdx`, `dpdy`) считаются до всех ветвлений.

struct Globals3d {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    // xyz — направление от земли к солнцу, w — синус высоты солнца.
    sun: vec4<f32>,
    // y — единица глубины карты теней на клетку сцены; z — сдвиг точки вдоль нормали при выборке
    // тени, в клетках: полторы точки карты теней, но не меньше 0,02, чтобы земля не затеняла сама себя.
    shade: vec4<f32>,
    // Место камеры: от него считается блик.
    eye: vec4<f32>,
    // Цвет и сила солнца и неба в линейной яркости.
    sun_light: vec4<f32>,
    sky_light: vec4<f32>,
    // Вода: x — на какой глубине в клетках дна уже не видно, y — длина ряби в клетках, z — часы воды в секундах.
    water: vec4<f32>,
};

struct Covers {
    // x — число слоёв, yz — размер сцены в клетках, w — слой карты цвета в массиве масок плюс один
    // (0 — карты цвета нет).
    head: vec4<f32>,
    // По слою снизу вверх: x — слой массивов материалов, y — карт материала на клетку сцены, z — номер
    // маски слоя в массиве масок (−1 — маски нет), w — `slope` слоя в градусах (−1 — правила крутизны нет).
    layers: array<vec4<f32>, 8>,
};

const ATLAS_SIZE: f32 = 2048.0;
const PI: f32 = 3.14159265;

// Маска не меньше этого — слой лежит целиком.
const SOLID_MASK: f32 = 0.99;
// Ширина полосы, на которой слой проступает по высоте своих камней.
const COVER_BAND: f32 = 0.25;
// Проекции склона с весом меньше этого не читаются.
const MIN_PROJECTION: f32 = 0.03;
// На сколько градусов круче порога `slope` слой набирает силу от 0 до 1.
const SLOPE_BAND: f32 = 5.0;

// Вода: числа вида, их подбирают на глаз.
// Толща пропускает `exp(−WATER_ABSORPTION × глубина / clarity)` света дна.
const WATER_ABSORPTION: f32 = 3.0;
// Отсвет неба по Шлику при взгляде по нормали и шероховатость блика солнца.
const WATER_SKY_F0: f32 = 0.02;
const WATER_ROUGHNESS: f32 = 0.08;
// Небо в отсвете: у горизонта светлее своего цвета, к зениту гуще; доля ряби в свете толщи.
const WATER_SKY_HORIZON: f32 = 1.5;
const WATER_SKY_ZENITH: f32 = 0.8;
const WATER_BODY_RIPPLE: f32 = 0.5;
// Мелкий слой ряби короче крупного в стольких раз; рябь гаснет, когда точка экрана больше этой доли её длины.
const RIPPLE_FINE: f32 = 0.43;
const RIPPLE_FADE_FROM: f32 = 0.25;
const RIPPLE_FADE_TO: f32 = 0.5;
// Уклон поверхности на единицу шума и секунд на цикл фазы течения (карта течения Valve).
const RIPPLE_SLOPE: f32 = 0.3;
const FLOW_CYCLE: f32 = 1.6;
// Рябь стоячей воды гонит ветер: куда и как быстро, в длинах слоя в секунду; слои идут накрест, чтобы узор не
// повторялся. Скорость течения, ниже которой вода стоячая.
const WIND_COARSE: vec2<f32> = vec2<f32>(0.32, 0.18);
const WIND_FINE: vec2<f32> = vec2<f32>(-0.24, 0.36);
const WIND_SWELL: vec2<f32> = vec2<f32>(-0.03, -0.11);
const STILL_SPEED: f32 = 0.2;
// Крупная пологая волна: во столько раз длиннее ряби и с такой долей её уклона; её видно и с высоты, где мелкая
// рябь уже погасла, и вода не замирает.
const SWELL_LENGTH: f32 = 4.0;
const SWELL_SLOPE: f32 = 0.7;
// Среднее двух выборок вдоль течения тише одной: на столько оно усиливается, чтобы рябь реки была той же силы, что
// рябь моря и озера, и река не отличалась от них цветом и бликами.
const STREAM_GAIN: f32 = 1.29;
// То же для значения шума — по нему рисуется пена: соседние выборки по значению похожи сильнее, чем по уклону.
const STREAM_VALUE_GAIN: f32 = 1.14;
// На сколько длин шума в обе стороны по течению разнесены выборки ряби и пены на реке: так узор вытянут вдоль течения.
const FLOW_STRETCH: f32 = 0.3;
// Пена: полоса у кромки глубиной долю `clarity`, пятно в долях длины ряби, дыхание в радианах в секунду, цвет.
const FOAM_EDGE_DEPTH: f32 = 0.05;
const FOAM_SCALE: f32 = 0.6;
const FOAM_BREATH: f32 = 0.9;
// Толщина нитей пены течения: насколько далеко от середины шума нить ещё светлая.
const FOAM_LACE: f32 = 0.06;
// Узор пены гаснет, когда точка экрана — от первой до второй доли длины пятна; вдали пена ровная, густотой
// `FOAM_FAR_BASE + FOAM_FAR_PER_FOAM × сила пены` — так в среднем густ узор вблизи.
const FOAM_FADE_FROM: f32 = 0.1;
const FOAM_FADE_TO: f32 = 0.3;
const FOAM_FAR_BASE: f32 = 0.05;
const FOAM_FAR_PER_FOAM: f32 = 0.25;
const FOAM_COLOR: vec3<f32> = vec3<f32>(0.92, 0.95, 0.96);
// Наибольшая плотность пены у кромки и пены течения.
const FOAM_EDGE_STRENGTH: f32 = 0.35;
const FOAM_FLOW_STRENGTH: f32 = 0.8;
// Падающая вода между струями — такая доля пены.
const FALL_FOAM_BASE: f32 = 0.75;
// Падающая вода тоньше такой доли `clarity` сходит на нет: тонкий край, облепивший скалу, не белеет.
const FALL_EDGE: f32 = 0.08;
// Падающая вода: от этого уклона глади вода начинает падать, на столько круче — падает целиком (середина — 45°);
// полос на клетку поперёк и вдоль.
const FALL_SLOPE: f32 = 0.6;
const FALL_BAND: f32 = 0.8;
const FALL_STREAKS_ACROSS: f32 = 4.0;
const FALL_STREAKS_ALONG: f32 = 0.7;
// Мокрый берег: высота полосы над кромкой в долях `clarity`, во сколько раз темнее и во сколько раз глаже у кромки.
const WET_BAND: f32 = 0.12;
const WET_DARKEN: f32 = 0.6;
const WET_ROUGHNESS: f32 = 0.5;

@group(0) @binding(0)
var<uniform> globals: Globals3d;
@group(0) @binding(1)
var atlas_texture: texture_2d_array<f32>;
@group(0) @binding(2)
var atlas_sampler_linear: sampler;
@group(0) @binding(3)
var shadow_map: texture_depth_2d;
@group(0) @binding(4)
var shadow_sampler: sampler_comparison;
@group(0) @binding(5)
var<uniform> covers: Covers;
// Цвет в sRGB, в прозрачности высота.
@group(0) @binding(6)
var material_color: texture_2d_array<f32>;
// Нормаль `xy`, шероховатость, затенение.
@group(0) @binding(7)
var material_data: texture_2d_array<f32>;
// Маски покрытий по четыре в слой, за ними слоем — карта цвета.
@group(0) @binding(8)
var cover_masks: texture_2d_array<f32>;
@group(0) @binding(9)
var material_sampler: sampler;

// --- Цвет и кривая яркости ------------------------------------------------------------------

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, c / 12.92, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    let high = 1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(high, x * 12.92, x <= vec3<f32>(0.0031308));
}

// Khronos PBR Neutral Tone Mapper: цвета ниже 0,76 почти не меняются, яркие сжимаются плавно и
// теряют насыщенность.
fn tone_map(color: vec3<f32>) -> vec3<f32> {
    let start_compression = 0.76;
    let desaturation = 0.15;
    let x = min(color.r, min(color.g, color.b));
    let offset = select(0.04, x - 6.25 * x * x, x < 0.08);
    let shifted = color - vec3<f32>(offset);
    let peak = max(shifted.r, max(shifted.g, shifted.b));
    let d = 1.0 - start_compression;
    let new_peak = 1.0 - d * d / (peak + d - start_compression);
    let scaled = shifted * (new_peak / peak);
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    let compressed = mix(scaled, vec3<f32>(new_peak), g);
    return select(compressed, shifted, peak < start_compression);
}

// Кадр на экран: кривая яркости, затем sRGB.
fn finish(color: vec3<f32>) -> vec3<f32> {
    return linear_to_srgb(tone_map(color));
}

// --- Свет -----------------------------------------------------------------------------------

// Видит ли точку солнце: 1 — да, 0 — тень. Сравнение всегда выполняется, а вне карты его результат
// отбрасывает `select`: выборка не терпит ветвления.
fn shadow_lit(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let facing = clamp(dot(normal, globals.sun.xyz), 0.15, 1.0);
    let tan_angle = sqrt(1.0 - facing * facing) / facing;
    let bias_cells = 0.04 + 0.025 * tan_angle;
    let moved = world + normal * globals.shade.z;
    let clip = globals.light_view_proj * vec4<f32>(moved, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let lit = textureSampleCompare(shadow_map, shadow_sampler, uv, ndc.z - bias_cells * globals.shade.y);
    let inside = uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 && ndc.z <= 1.0;
    return select(1.0, lit, inside);
}

// Блик солнца на поверхности с шероховатостью `roughness` и отражением `f0` при взгляде по нормали:
// модель GGX, геометрия Смита — Шлика, отражение по Шлику.
fn sun_glint(
    normal: vec3<f32>,
    world: vec3<f32>,
    roughness: f32,
    f0: f32,
    visible: f32,
) -> vec3<f32> {
    let to_sun = globals.sun.xyz;
    let n_l = max(dot(normal, to_sun), 0.0);
    let to_eye = normalize(globals.eye.xyz - world);
    let half_way = normalize(to_sun + to_eye);
    let n_v = max(dot(normal, to_eye), 0.0);
    let n_h = max(dot(normal, half_way), 0.0);
    let v_h = max(dot(to_eye, half_way), 0.0);
    let alpha = max(roughness * roughness, 0.002);
    let alpha2 = alpha * alpha;
    let denominator = n_h * n_h * (alpha2 - 1.0) + 1.0;
    let distribution = alpha2 / (PI * denominator * denominator);
    let k = alpha * 0.5;
    let visibility = 0.25 / ((n_l * (1.0 - k) + k) * (n_v * (1.0 - k) + k));
    let fresnel = f0 + (1.0 - f0) * pow(clamp(1.0 - v_h, 0.0, 1.0), 5.0);
    // `sun_light` — освещённость, уже делённая на π (рассеянный свет — `albedo × sun` без 1/π), поэтому
    // блик GGX, как в glTF, умножается на π обратно.
    return globals.sun_light.rgb * (PI * distribution * visibility * fresnel * n_l * visible);
}

// Поверхность цвета `albedo` (линейная яркость) в точке `world` с нормалью `normal`: свет солнца по
// косинусу и по тени, свет неба сверху сильнее, чем сбоку, и только там, куда он доходит (`ao`),
// блик солнца — `sun_glint` с F0 = 0,04.
fn shade(
    albedo: vec3<f32>,
    world: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    ao: f32,
    visible: f32,
) -> vec3<f32> {
    let n_l = max(dot(normal, globals.sun.xyz), 0.0);
    let sun = globals.sun_light.rgb * (n_l * visible);
    let sky = globals.sky_light.rgb * ((1.0 + normal.z) * 0.5 * ao);
    return albedo * (sun + sky) + sun_glint(normal, world, roughness, 0.04, visible);
}

// Ровная матовая поверхность: шероховатость 1, без карты мелкого рельефа.
fn shade_matte(albedo: vec3<f32>, world: vec3<f32>, normal: vec3<f32>, visible: f32) -> vec3<f32> {
    return shade(albedo, world, normal, 1.0, 1.0, visible);
}

// --- Материалы: hex-tiling ------------------------------------------------------------------

struct MapSample {
    color: vec3<f32>,
    height: f32,
    // Наклон нормали в осях картинки: x — вправо, y — вверх по картинке.
    tilt: vec2<f32>,
    roughness: f32,
    ao: f32,
};

fn hash_u32(value: u32) -> u32 {
    var h = value;
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    return h ^ (h >> 16u);
}

// Три случайных числа от 0 до 1 у вершины шестиугольной сетки: сдвиг и доля оборота.
fn cell_random(cell: vec2<i32>) -> vec3<f32> {
    let a = hash_u32(bitcast<u32>(cell.x) + 0x9e3779b9u);
    let b = hash_u32(a ^ bitcast<u32>(cell.y));
    let c = hash_u32(b + 0x85ebca6bu);
    let d = hash_u32(c ^ 0xc2b2ae35u);
    return vec3<f32>(f32(b >> 8u), f32(c >> 8u), f32(d >> 8u)) / 16777216.0;
}

fn hex_center(vertex: vec2<i32>) -> vec2<f32> {
    let v = vec2<f32>(vertex);
    return vec2<f32>(v.x + 0.5 * v.y, v.y / 1.15470054) / (2.0 * sqrt(3.0));
}

// Mikkelsen, «Practical Real-Time Hex-Tiling»: точка `st` лежит в треугольнике сетки; вес каждой из
// трёх его вершин — доля пути к ней.
struct HexGrid {
    v1: vec2<i32>,
    v2: vec2<i32>,
    v3: vec2<i32>,
    weights: vec3<f32>,
};

fn hex_grid(st: vec2<f32>) -> HexGrid {
    let scaled = st * (2.0 * sqrt(3.0));
    let skewed = vec2<f32>(scaled.x - 0.57735027 * scaled.y, 1.15470054 * scaled.y);
    let base = vec2<i32>(floor(skewed));
    let f = fract(skewed);
    let z = 1.0 - f.x - f.y;
    let upper = z <= 0.0;
    let s = select(0, 1, upper);
    let sf = f32(s);
    let direction = 2.0 * sf - 1.0;
    var grid: HexGrid;
    grid.weights = vec3<f32>(-z * direction, sf - f.y * direction, sf - f.x * direction);
    grid.v1 = base + vec2<i32>(s, s);
    grid.v2 = base + vec2<i32>(s, 1 - s);
    grid.v3 = base + vec2<i32>(1 - s, s);
    return grid;
}

// Одна ячейка: своя случайная точка карты и свой поворот; нормаль поворачивается вместе с ячейкой.
fn hex_cell(
    material: i32,
    vertex: vec2<i32>,
    st: vec2<f32>,
    dx: vec2<f32>,
    dy: vec2<f32>,
    turn: f32,
) -> MapSample {
    let random = cell_random(vertex);
    let angle = (random.z * 2.0 - 1.0) * PI * turn;
    let c = cos(angle);
    let s = sin(angle);
    let rotation = mat2x2<f32>(c, s, -s, c);
    let center = hex_center(vertex);
    let uv = rotation * (st - center) + center + random.xy;
    let color = textureSampleGrad(material_color, material_sampler, uv, material, rotation * dx, rotation * dy);
    let data = textureSampleGrad(material_data, material_sampler, uv, material, rotation * dx, rotation * dy);
    var out: MapSample;
    out.color = color.rgb;
    out.height = color.a;
    out.tilt = rotation * (data.xy * 2.0 - 1.0);
    out.roughness = data.z;
    out.ao = data.w;
    return out;
}

// Карты материала `material` в точке `st` (в картах), без видимого повтора: картинка в каждом шестиграннике
// сдвинута, а при `turn` 1 ещё и повёрнута случайно. На стенах `turn` 0: пласты камня идут ровно.
fn hex_sample(material: i32, st: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, turn: f32) -> MapSample {
    let grid = hex_grid(st);
    var w = pow(max(grid.weights, vec3<f32>(0.0)), vec3<f32>(7.0));
    w = w / (w.x + w.y + w.z);
    let a = hex_cell(material, grid.v1, st, dx, dy, turn);
    let b = hex_cell(material, grid.v2, st, dx, dy, turn);
    let c = hex_cell(material, grid.v3, st, dx, dy, turn);
    var out: MapSample;
    out.color = a.color * w.x + b.color * w.y + c.color * w.z;
    out.height = a.height * w.x + b.height * w.y + c.height * w.z;
    out.tilt = a.tilt * w.x + b.tilt * w.y + c.tilt * w.z;
    out.roughness = a.roughness * w.x + b.roughness * w.y + c.roughness * w.z;
    out.ao = a.ao * w.x + b.ao * w.y + c.ao * w.z;
    return out;
}

// --- Покрытия рельефа -----------------------------------------------------------------------

struct Surface {
    albedo: vec3<f32>,
    height: f32,
    normal: vec3<f32>,
    roughness: f32,
    ao: f32,
};

fn mix_surface(low: Surface, high: Surface, t: f32) -> Surface {
    var out: Surface;
    out.albedo = mix(low.albedo, high.albedo, t);
    out.height = mix(low.height, high.height, t);
    out.normal = normalize(mix(low.normal, high.normal, t));
    out.roughness = mix(low.roughness, high.roughness, t);
    out.ao = mix(low.ao, high.ao, t);
    return out;
}

// Нормаль в осях картинки по её наклону: `z` восстановлена.
fn tangent_normal(tilt: vec2<f32>) -> vec3<f32> {
    let flat_part = dot(tilt, tilt);
    let z = sqrt(max(1.0 - flat_part, 0.0));
    return vec3<f32>(tilt, z);
}

// Материал слоя `layer` в точке `world` на земле с нормалью `n`. Картинка проецируется сверху и с двух
// боков, чтобы на стене камни были того же размера, что на ровном месте; веса — по нормали земли, с
// резкой границей, и проекции с малым весом не читаются. Нормали сводятся по способу whiteout (Ben
// Golus, «Normal Mapping for a Triplanar Shader»).
fn layer_surface(
    layer: i32,
    world: vec3<f32>,
    n: vec3<f32>,
    ddx: vec3<f32>,
    ddy: vec3<f32>,
) -> Surface {
    let entry = covers.layers[layer];
    let material = i32(entry.x + 0.5);
    let scale = entry.y;

    var w = pow(abs(n), vec3<f32>(8.0));
    w = w / (w.x + w.y + w.z);
    w = select(w, vec3<f32>(0.0), w < vec3<f32>(MIN_PROJECTION));
    w = w / (w.x + w.y + w.z);

    var out: Surface;
    out.albedo = vec3<f32>(0.0);
    out.height = 0.0;
    out.normal = vec3<f32>(0.0);
    out.roughness = 0.0;
    out.ao = 0.0;

    // Сверху: картинка вправо по x, вверх по картинке — к дальнему краю сцены, то есть против y.
    if (w.z > 0.0) {
        let s = hex_sample(material, world.xy * scale, ddx.xy * scale, ddy.xy * scale, 1.0);
        let t = tangent_normal(s.tilt);
        let blended = vec3<f32>(t.x + n.x, -t.y + n.y, abs(t.z) * n.z);
        out.albedo += s.color * w.z;
        out.height += s.height * w.z;
        out.normal += normalize(blended) * w.z;
        out.roughness += s.roughness * w.z;
        out.ao += s.ao * w.z;
    }
    // Сбоку, лицом вдоль x: картинка вправо по y, вверх по z.
    if (w.x > 0.0) {
        let st = vec2<f32>(world.y, -world.z) * scale;
        let s = hex_sample(material, st, vec2<f32>(ddx.y, -ddx.z) * scale, vec2<f32>(ddy.y, -ddy.z) * scale, 0.0);
        let t = tangent_normal(s.tilt);
        let blended = vec3<f32>(abs(t.z) * n.x, t.x + n.y, t.y + n.z);
        out.albedo += s.color * w.x;
        out.height += s.height * w.x;
        out.normal += normalize(blended) * w.x;
        out.roughness += s.roughness * w.x;
        out.ao += s.ao * w.x;
    }
    // Сбоку, лицом вдоль y: картинка вправо по x, вверх по z.
    if (w.y > 0.0) {
        let st = vec2<f32>(world.x, -world.z) * scale;
        let s = hex_sample(material, st, vec2<f32>(ddx.x, -ddx.z) * scale, vec2<f32>(ddy.x, -ddy.z) * scale, 0.0);
        let t = tangent_normal(s.tilt);
        let blended = vec3<f32>(t.x + n.x, abs(t.z) * n.y, t.y + n.z);
        out.albedo += s.color * w.y;
        out.height += s.height * w.y;
        out.normal += normalize(blended) * w.y;
        out.roughness += s.roughness * w.y;
        out.ao += s.ao * w.y;
    }
    out.normal = normalize(out.normal);
    return out;
}

// Доля слоя поверх нижних: при сером крае маски проступают верхушки его камней, при белом он закрывает
// нижний целиком. Маска 0 — слоя нет, маска 1 — слой лежит целиком.
fn cover_weight(mask: f32, height: f32) -> f32 {
    return clamp((height + mask * (1.0 + COVER_BAND) - 1.0) / COVER_BAND, 0.0, 1.0);
}

// Земля с покрытиями в точке `world`: сила слоя — большее из его маски и крутизны склона (от порога
// `slope` до порога плюс `SLOPE_BAND`), слои ниже самого верхнего слоя с силой 1 и слои с силой 0
// не читаются. Массив масок читается, только если у какого-то слоя есть маска.
fn cover_surface(world: vec3<f32>, n: vec3<f32>, ddx: vec3<f32>, ddy: vec3<f32>) -> Surface {
    let count = i32(covers.head.x + 0.5);
    let uv = world.xy / covers.head.yz;
    // Угол между вертикалью и нормалью земли, той же, что даёт свет.
    let tilt = degrees(acos(clamp(n.z, 0.0, 1.0)));
    var mask_values = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    var reads_first = false;
    var reads_second = false;
    for (var k = 1; k < count; k++) {
        let channel = i32(round(covers.layers[k].z));
        reads_first = reads_first || (channel >= 0 && channel < 4);
        reads_second = reads_second || channel >= 4;
    }
    if (reads_first) {
        let first = textureSampleLevel(cover_masks, atlas_sampler_linear, uv, 0, 0.0);
        mask_values[0] = first.x;
        mask_values[1] = first.y;
        mask_values[2] = first.z;
        mask_values[3] = first.w;
    }
    if (reads_second) {
        let second = textureSampleLevel(cover_masks, atlas_sampler_linear, uv, 1, 0.0);
        mask_values[4] = second.x;
        mask_values[5] = second.y;
        mask_values[6] = second.z;
    }
    var strengths = array<f32, 8>(1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    var start = 0;
    for (var k = 1; k < count; k++) {
        let entry = covers.layers[k];
        let channel = i32(round(entry.z));
        var strength = 0.0;
        if (channel >= 0) {
            strength = clamp(mask_values[channel] / SOLID_MASK, 0.0, 1.0);
        }
        if (entry.w >= 0.0) {
            strength = max(strength, clamp((tilt - entry.w) / SLOPE_BAND, 0.0, 1.0));
        }
        strengths[k] = strength;
        if (strengths[k] >= 1.0) {
            start = k;
        }
    }
    var surface = layer_surface(start, world, n, ddx, ddy);
    for (var k = start + 1; k < count; k++) {
        if (strengths[k] > 0.0) {
            let top = layer_surface(k, world, n, ddx, ddy);
            surface = mix_surface(surface, top, cover_weight(strengths[k], top.height));
        }
    }
    return surface;
}

// --- Рельеф и вода --------------------------------------------------------------------------

// У земли `extra.x` — гладь воды над точкой (вдали от воды — ниже земли на `clarity`, в сцене без воды — около
// −1000); у воды `extra` — глубина,
// течение по осям сцены в клетках в секунду и пена.
struct TerrainVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) extra: vec4<f32>,
};

@vertex
fn vs_shadow_terrain(vertex: TerrainVertex) -> @builtin(position) vec4<f32> {
    return globals.light_view_proj * vec4<f32>(vertex.position, 1.0);
}

struct TerrainOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) surface: f32,
};

@vertex
fn vs_terrain(vertex: TerrainVertex) -> TerrainOutput {
    var out: TerrainOutput;
    out.clip_position = globals.view_proj * vec4<f32>(vertex.position, 1.0);
    out.world = vertex.position;
    out.normal = vertex.normal;
    out.color = vertex.color;
    out.surface = vertex.extra.x;
    return out;
}

// Мокрый берег: 1 у кромки и ниже глади, к верху полосы высотой `WET_BAND × clarity` над гладью — 0.
fn wetness(height: f32, surface: f32) -> f32 {
    return clamp(1.0 - (height - surface) / (WET_BAND * globals.water.x), 0.0, 1.0);
}

// Карта цвета рельефа в точке `world`: `rgb` — во сколько раз цвет земли светлее (0,5 — как есть,
// 0 — чёрный, 1 — вдвое светлее), `a` — сколько неба видно из этой точки. Без карты — ничего не меняет.
fn terrain_tint(world: vec3<f32>) -> vec4<f32> {
    if (covers.head.w < 0.5) {
        return vec4<f32>(0.5, 0.5, 0.5, 1.0);
    }
    let layer = i32(covers.head.w - 0.5);
    return textureSampleLevel(cover_masks, atlas_sampler_linear, world.xy / covers.head.yz, layer, 0.0);
}

// Земля: без покрытий залита цветом сцены и освещена как матовая поверхность. Карта цвета красит
// землю и гасит свет неба там, куда он не доходит. У воды земля темнее и глаже (`wetness`).
@fragment
fn fs_terrain(in: TerrainOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let ddx = dpdx(in.world);
    let ddy = dpdy(in.world);
    let visible = shadow_lit(in.world, normal);
    let tint = terrain_tint(in.world);
    let wet = wetness(in.world.z, in.surface);
    let darken = mix(1.0, WET_DARKEN, wet);
    if (covers.head.x < 0.5) {
        let albedo = srgb_to_linear(in.color) * tint.rgb * 2.0 * darken;
        return vec4<f32>(finish(shade(albedo, in.world, normal, mix(1.0, WET_ROUGHNESS, wet), tint.a, visible)), 1.0);
    }
    let surface = cover_surface(in.world, normal, ddx, ddy);
    let albedo = surface.albedo * tint.rgb * 2.0 * darken;
    let roughness = mix(surface.roughness, surface.roughness * WET_ROUGHNESS, wet);
    let color = shade(albedo, in.world, surface.normal, roughness, surface.ao * tint.a, visible);
    return vec4<f32>(finish(color), 1.0);
}

// --- Вода -----------------------------------------------------------------------------------

struct WaterOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // Глубина, течение по осям сцены и пена.
    @location(3) data: vec4<f32>,
};

@vertex
fn vs_water(vertex: TerrainVertex) -> WaterOutput {
    var out: WaterOutput;
    out.clip_position = globals.view_proj * vec4<f32>(vertex.position, 1.0);
    out.world = vertex.position;
    out.normal = vertex.normal;
    out.color = vertex.color;
    out.data = vertex.extra;
    return out;
}

fn cell_value(cell: vec2<i32>) -> f32 {
    let a = hash_u32(bitcast<u32>(cell.x) + 0x9e3779b9u);
    let b = hash_u32(a ^ bitcast<u32>(cell.y));
    return f32(b >> 8u) / 16777216.0;
}

// Шум значения от 0 до 1 с производными: x — значение, yz — производные по осям точки.
fn value_noise(p: vec2<f32>) -> vec3<f32> {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    let a = cell_value(i);
    let b = cell_value(i + vec2<i32>(1, 0));
    let c = cell_value(i + vec2<i32>(0, 1));
    let d = cell_value(i + vec2<i32>(1, 1));
    let k = a - b - c + d;
    let value = a + (b - a) * u.x + (c - a) * u.y + k * u.x * u.y;
    let slope = du * vec2<f32>((b - a) + k * u.y, (c - a) + k * u.x);
    return vec3<f32>(value, slope);
}

// Шум, который несёт течение `flow` (длин шума в секунду): две фазы, сдвинутые на половину цикла, с весом
// треугольной волной — одна гаснет, когда сбрасывается, а другая в силе (Vlachos, «Water Flow in Portal 2»).
fn flowing_noise(st: vec2<f32>, flow: vec2<f32>, seed: f32) -> vec3<f32> {
    let cycle = globals.water.z / FLOW_CYCLE;
    let cycle_b = cycle + 0.5;
    let phase_a = fract(cycle);
    let phase_b = fract(cycle_b);
    let weight_a = 1.0 - abs(1.0 - 2.0 * phase_a);
    let weight_b = 1.0 - abs(1.0 - 2.0 * phase_b);
    let jump_a = vec2<f32>(floor(cycle) * 0.37 + seed, floor(cycle) * 0.91) * 5.0;
    let jump_b = vec2<f32>(floor(cycle_b) * 0.37 + seed + 7.0, floor(cycle_b) * 0.91 + 3.0) * 5.0;
    let a = value_noise(st - flow * (phase_a * FLOW_CYCLE) + jump_a);
    let b = value_noise(st - flow * (phase_b * FLOW_CYCLE) + jump_b);
    // Смесь двух независимых узоров тише каждого: делится на корень из суммы квадратов весов, чтобы сила ряби на реке
    // не пульсировала с фазой и не была меньше, чем на стоячей воде.
    let blended = a * weight_a + b * weight_b;
    let gain = inverseSqrt(weight_a * weight_a + weight_b * weight_b);
    return vec3<f32>(0.5 + (blended.x - 0.5) * gain, blended.yz * gain);
}

// Шум, который несёт течение `flow` (клеток в секунду), вытянутый вдоль течения: среднее двух выборок, разнесённых
// по течению на `FLOW_STRETCH` длины шума в обе стороны, — на реке рябь и пена ложатся струями по течению. Сетка
// шума не поворачивается за течением: поворот вокруг начала сцены дробил бы узор на изгибах реки тем мельче, чем
// дальше река от начала. Шум длиной `length_cells` клеток; x — значение, yz — уклон на единицу шума, как у
// `value_noise`.
fn stream_noise(world: vec2<f32>, flow: vec2<f32>, length_cells: f32, seed: f32) -> vec3<f32> {
    let speed = length(flow);
    let along = select(vec2<f32>(1.0, 0.0), flow / max(speed, 0.00001), speed > 0.00001);
    let st = world / length_cells;
    let reach = along * FLOW_STRETCH;
    let drift = flow / length_cells;
    let mean = 0.5 * (flowing_noise(st + reach, drift, seed) + flowing_noise(st - reach, drift, seed));
    return vec3<f32>(0.5 + (mean.x - 0.5) * STREAM_VALUE_GAIN, mean.yz * STREAM_GAIN);
}

// Слой ряби длиной `length_cells`: на стоячей воде узор гонит ветер `wind`, на реке — течение `flow`; `moving` — насколько
// вода течёт. Уклон на единицу шума.
fn ripple_layer(world: vec2<f32>, flow: vec2<f32>, moving: f32, length_cells: f32, wind: vec2<f32>, seed: f32) -> vec2<f32> {
    let still = value_noise(world / length_cells - wind * globals.water.z + vec2<f32>(seed * 3.7, seed * 1.3));
    let stream = stream_noise(world + vec2<f32>(seed * 1.9, seed * 2.6), flow, length_cells, seed);
    // Смесь двух независимых узоров тише каждого — там, где течение гаснет у устья, рябь слабела бы полосой.
    return mix(still.yz, stream.yz, moving) * blend_gain(moving);
}

// Во сколько раз усилить смесь двух независимых узоров с долей второго `share`, чтобы она была той же силы, что каждый.
fn blend_gain(share: f32) -> f32 {
    return inverseSqrt((1.0 - share) * (1.0 - share) + share * share);
}

// Уклон поверхности от ряби в точке `world` (в клетках) при течении `flow`: слои шума длиной `ripple`, `RIPPLE_FINE` от
// неё и пологая волна в `SWELL_LENGTH` раз длиннее. Слой гаснет, когда точка экрана `footprint` (клеток) становится
// больше половины его длины.
fn ripple_slope(world: vec2<f32>, flow: vec2<f32>, footprint: f32) -> vec2<f32> {
    let coarse_length = globals.water.y;
    let fine_length = coarse_length * RIPPLE_FINE;
    let swell_length = coarse_length * SWELL_LENGTH;
    let moving = smoothstep(0.0, STILL_SPEED, length(flow));
    let coarse = ripple_layer(world, flow, moving, coarse_length, WIND_COARSE, 0.0);
    let fine = ripple_layer(world, flow, moving, fine_length, WIND_FINE, 3.0);
    let swell = ripple_layer(world, flow, moving, swell_length, WIND_SWELL, 7.0);
    let coarse_fade = 1.0 - smoothstep(RIPPLE_FADE_FROM * coarse_length, RIPPLE_FADE_TO * coarse_length, footprint);
    let fine_fade = 1.0 - smoothstep(RIPPLE_FADE_FROM * fine_length, RIPPLE_FADE_TO * fine_length, footprint);
    let swell_fade = 1.0 - smoothstep(RIPPLE_FADE_FROM * swell_length, RIPPLE_FADE_TO * swell_length, footprint);
    return RIPPLE_SLOPE * (coarse * coarse_fade + fine * fine_fade + swell * (SWELL_SLOPE * swell_fade));
}

// Узор пены: на реке его несёт течение струями, на стоячей воде гонит ветер.
fn foam_noise(world: vec2<f32>, flow: vec2<f32>) -> f32 {
    let foam_length = globals.water.y * FOAM_SCALE;
    let st = world / foam_length;
    let moving = smoothstep(0.0, STILL_SPEED, length(flow));
    let still = value_noise(st - WIND_COARSE * globals.water.z).x;
    return 0.5 + (mix(still, stream_noise(world, flow, foam_length, 5.0).x, moving) - 0.5) * blend_gain(moving);
}

// Вода: сквозь неё видно дно на долю `exp(−WATER_ABSORPTION × глубина / clarity)`, остальное — цвет толщи,
// освещённый как матовая поверхность, с тенью. Поверх — отсвет неба по Шлику и блик солнца по нормали с рябью,
// пена у кромки, на перекатах и в падающей воде. Выход — цвет, уже умноженный на свою долю
// (`PREMULTIPLIED_ALPHA_BLENDING`): `1 − clear × (1 − пена)` перекрывает дно, остальное просвечивает.
// Производные — до всех ветвлений.
@fragment
fn fs_water(in: WaterOutput) -> @location(0) vec4<f32> {
    let footprint = max(length(dpdx(in.world.xy)), length(dpdy(in.world.xy)));
    let base_normal = normalize(in.normal);
    let visible = shadow_lit(in.world, base_normal);
    let depth = max(in.data.x, 0.0);
    let flow = in.data.yz;
    let clarity = globals.water.x;
    let clear = exp(-WATER_ABSORPTION * depth / clarity);

    let steepness = length(base_normal.xy) / max(base_normal.z, 0.001);
    let fall = smoothstep(FALL_SLOPE, FALL_SLOPE + FALL_BAND, steepness);
    // Наклон глади свет учитывает только у падающей воды: гладь переката собрана из отрезков с разным уклоном, и
    // узкий блик вспыхивал бы полосой на каждом их стыке.
    let surface_normal = normalize(mix(vec3<f32>(0.0, 0.0, 1.0), base_normal, fall));
    let tilt = ripple_slope(in.world.xy, flow, footprint);
    let normal = normalize(surface_normal - vec3<f32>(tilt, 0.0));
    // Толща освещена по нормали с ослабленной рябью: сверху рябь видна светом солнца на склонах волн, а не отсветом.
    let body_normal = normalize(surface_normal - vec3<f32>(tilt * WATER_BODY_RIPPLE, 0.0));
    let body = shade_matte(srgb_to_linear(in.color), in.world, body_normal, visible);
    let to_eye = normalize(globals.eye.xyz - in.world);
    let mirrored = reflect(-to_eye, normal);
    let sky = globals.sky_light.rgb * mix(WATER_SKY_HORIZON, WATER_SKY_ZENITH, clamp(mirrored.z, 0.0, 1.0));
    let sky_reflection = sky * (WATER_SKY_F0 + (1.0 - WATER_SKY_F0) * pow(1.0 - max(dot(normal, to_eye), 0.0), 5.0));
    let glint = sun_glint(normal, in.world, WATER_ROUGHNESS, WATER_SKY_F0, visible);
    let lit = finish(body);
    let specular = max(finish(body + sky_reflection + glint) - lit, vec3<f32>(0.0));

    let pattern = foam_noise(in.world.xy, flow);
    // Вдали узор пены мельче точки экрана и мерцал бы: нити и пятна сменяются ровной пеной той же густоты.
    let foam_length = globals.water.y * FOAM_SCALE;
    let foam_far = smoothstep(FOAM_FADE_FROM * foam_length, FOAM_FADE_TO * foam_length, footprint);
    let breath = 0.5 + 0.5 * sin(globals.water.z * FOAM_BREATH + pattern * 6.2831853);
    let edge = 1.0 - smoothstep(0.0, FOAM_EDGE_DEPTH * clarity, depth);
    // На самой падающей воде пену рисуют струи; полоса у кромки и пятна течения — только у её подножия и на перекатах.
    let edge_foam = FOAM_EDGE_STRENGTH * edge * (1.0 - fall) * (0.5 + 0.5 * mix(smoothstep(0.3, 0.85, pattern * 0.6 + breath * 0.5), 0.5, foam_far));
    // Пена течения — тонкие нити по линиям шума, как кружево на перекате; сплошные пятна — только где её больше всего.
    let lace = 1.0 - smoothstep(0.0, FOAM_LACE * (0.5 + in.data.w), abs(pattern - 0.5));
    let patches = smoothstep(0.62, 0.85, pattern) * in.data.w;
    // Вдали — средняя густота узора при этой силе пены, чтобы бахрома ниже переката издали не светлела.
    let woven = mix(max(lace, patches), FOAM_FAR_BASE + FOAM_FAR_PER_FOAM * in.data.w, foam_far);
    let flow_foam = FOAM_FLOW_STRENGTH * in.data.w * (1.0 - fall) * woven;
    // Струи — по течению: оно плавно меняется от точки к точке, а наклон глади скачет на каждом треугольнике уступа.
    let fall_speed = length(flow);
    let down = select(normalize(base_normal.xy + vec2<f32>(0.00001, 0.0)), flow / max(fall_speed, 0.00001), fall_speed > 0.01);
    let across = vec2<f32>(-down.y, down.x);
    let streak = value_noise(vec2<f32>(
        dot(in.world.xy, across) * FALL_STREAKS_ACROSS,
        (dot(in.world.xy, down) - length(flow) * globals.water.z) * FALL_STREAKS_ALONG,
    )).x;
    let fall_foam = fall * smoothstep(0.0, FALL_EDGE * clarity, depth) * mix(FALL_FOAM_BASE, 1.0, smoothstep(0.4, 0.8, streak));
    let foam = clamp(max(edge_foam, max(flow_foam, fall_foam)), 0.0, 1.0);

    // Пена падающей воды освещена как ровная: иначе каждый треугольник уступа светится по-своему.
    let foam_normal = normalize(mix(base_normal, vec3<f32>(0.0, 0.0, 1.0), fall));
    let foam_color = finish(shade_matte(FOAM_COLOR, in.world, foam_normal, visible));
    // На падающей воде отсвета и блика нет: их грани уступа дробят на осколки.
    let rgb = lit * ((1.0 - clear) * (1.0 - foam)) + foam_color * foam + specular * (1.0 - fall);
    return vec4<f32>(rgb, 1.0 - clear * (1.0 - foam));
}

// --- Фигуры ---------------------------------------------------------------------------------

struct ShapeVertex {
    // Единичный объём: x, y от −0,5 до 0,5, z от 0 до 1.
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Сдвиг точки полушария капсулы на долю его высоты и признак полушария.
    @location(2) cap: vec2<f32>,
};

struct ShapeInstance {
    // Середина на земле и косинус с синусом поворота.
    @location(3) placement: vec4<f32>,
    // Ширина, глубина, высота и высота полушария капсулы.
    @location(4) dims: vec4<f32>,
    // rgb — цвет, w — высота основания над нулём сцены.
    @location(5) tint: vec4<f32>,
};

fn shape_world(vertex: ShapeVertex, instance: ShapeInstance) -> vec3<f32> {
    let local = vec3<f32>(
        vertex.position.x * instance.dims.x,
        vertex.position.y * instance.dims.y,
        vertex.position.z * instance.dims.z + vertex.cap.x * instance.dims.w,
    );
    let c = instance.placement.z;
    let s = instance.placement.w;
    return vec3<f32>(
        instance.placement.x + c * local.x - s * local.y,
        instance.placement.y + s * local.x + c * local.y,
        instance.tint.w + local.z,
    );
}

// Нормаль при растяжении — через обратное растяжение; полушарие капсулы растянуто по высоте на
// `2 × cap_height`, а не на `height`.
fn shape_normal(vertex: ShapeVertex, instance: ShapeInstance) -> vec3<f32> {
    let height = mix(instance.dims.z, 2.0 * instance.dims.w, vertex.cap.y);
    let stretched = vertex.normal / vec3<f32>(instance.dims.x, instance.dims.y, max(height, 0.000001));
    let n = normalize(stretched);
    let c = instance.placement.z;
    let s = instance.placement.w;
    return vec3<f32>(c * n.x - s * n.y, s * n.x + c * n.y, n.z);
}

@vertex
fn vs_shadow(vertex: ShapeVertex, instance: ShapeInstance) -> @builtin(position) vec4<f32> {
    return globals.light_view_proj * vec4<f32>(shape_world(vertex, instance), 1.0);
}

struct ShapeOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn vs_shape(vertex: ShapeVertex, instance: ShapeInstance) -> ShapeOutput {
    let world = shape_world(vertex, instance);
    var out: ShapeOutput;
    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = shape_normal(vertex, instance);
    out.color = instance.tint.rgb;
    return out;
}

@fragment
fn fs_shape(in: ShapeOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let visible = shadow_lit(in.world, normal);
    return vec4<f32>(finish(shade_matte(srgb_to_linear(in.color), in.world, normal, visible)), 1.0);
}

// --- Плитки и плоские объекты на земле ------------------------------------------------------

struct GroundVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Точка в листе атласа, в пикселях, как у `rect.wgsl`.
    @location(2) uv_px: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) uv_min: vec2<f32>,
    @location(5) uv_max: vec2<f32>,
    // x — лист атласа, y — признак сглаживания, z — признак свечения.
    @location(6) sheet: vec3<f32>,
};

struct GroundOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv_px: vec2<f32>,
    @location(2) @interpolate(flat) uv_min: vec2<f32>,
    @location(3) @interpolate(flat) uv_max: vec2<f32>,
    @location(4) @interpolate(flat) atlas_layer: i32,
    @location(5) @interpolate(flat) smooth_sample: f32,
    @location(6) world: vec3<f32>,
    @location(7) normal: vec3<f32>,
    @location(8) @interpolate(flat) glow: f32,
};

@vertex
fn vs_ground(vertex: GroundVertex) -> GroundOutput {
    var out: GroundOutput;
    out.clip_position = globals.view_proj * vec4<f32>(vertex.position, 1.0);
    out.color = vertex.color;
    out.uv_px = vertex.uv_px;
    out.uv_min = vertex.uv_min;
    out.uv_max = vertex.uv_max;
    out.atlas_layer = i32(vertex.sheet.x + 0.5);
    out.smooth_sample = vertex.sheet.y;
    out.glow = vertex.sheet.z;
    out.world = vertex.position;
    out.normal = vertex.normal;
    return out;
}

// Цвет картинки уже умножен на прозрачность: свет считается по цвету без неё, затем цвет умножается
// снова. Светящаяся картинка не освещается и не затеняется: отдаёт `(цвет, 0)`, и смешивание
// `PREMULTIPLIED_ALPHA_BLENDING` прибавляет цвет к тому, что под ней.
@fragment
fn fs_ground(in: GroundOutput) -> @location(0) vec4<f32> {
    let clamped = clamp(in.uv_px, in.uv_min, in.uv_max);
    let uv = clamped / ATLAS_SIZE;
    let nearest = textureLoad(atlas_texture, vec2<i32>(floor(clamped)), in.atlas_layer, 0);
    let linear = textureSample(atlas_texture, atlas_sampler_linear, uv, in.atlas_layer);
    let sampled = select(nearest, linear, in.smooth_sample > 0.5);
    let base = in.color * sampled;
    let normal = normalize(in.normal);
    let visible = shadow_lit(in.world, normal);
    let straight = base.rgb / max(base.a, 0.0001);
    let lit = finish(shade_matte(srgb_to_linear(straight), in.world, normal, visible));
    return vec4<f32>(select(lit * base.a, base.rgb, in.glow > 0.5), base.a * (1.0 - in.glow));
}

// --- Модели ---------------------------------------------------------------------------------

// Материал части модели по стандарту glTF: цвет и затенение освещают её так же, как рельеф и фигуры, но с
// металлом и своими картами мелкого рельефа. Все четыре карты есть всегда: у части без карты — белая точка
// (у мелкого рельефа — ровная).
struct ModelMaterial {
    // Цвет `baseColorFactor` в линейной яркости.
    base: vec4<f32>,
    // x — металл, y — шероховатость, z — `scale` мелкого рельефа, w — `strength` затенения.
    factors: vec4<f32>,
    // x — `alphaMode`: 0 — `OPAQUE`, 1 — `MASK`, 2 — `BLEND`; y — `alphaCutoff`.
    alpha: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> model_material: ModelMaterial;
@group(1) @binding(1)
var model_base_texture: texture_2d<f32>;
@group(1) @binding(2)
var model_base_sampler: sampler;
@group(1) @binding(3)
var model_metal_texture: texture_2d<f32>;
@group(1) @binding(4)
var model_metal_sampler: sampler;
@group(1) @binding(5)
var model_normal_texture: texture_2d<f32>;
@group(1) @binding(6)
var model_normal_sampler: sampler;
@group(1) @binding(7)
var model_occlusion_texture: texture_2d<f32>;
@group(1) @binding(8)
var model_occlusion_sampler: sampler;

struct ModelVertex {
    // Вершина в осях сцены: середина основания модели в начале, низ на нуле.
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // xyz — направление вдоль u карты, w — знак: cross(normal, tangent.xyz) * w идёт вверх по карте.
    @location(3) tangent: vec4<f32>,
    @location(4) color: vec4<f32>,
};

struct ModelInstance {
    // x, y — середина основания на земле, z, w — косинус и синус поворота.
    @location(5) placement: vec4<f32>,
    // x — высота основания над нулём сцены, y — во сколько раз модель увеличена, z — во сколько раз она ещё
    // растянута вдоль своей оси +Y (у кусков дороги; у остальных 1).
    @location(6) base: vec4<f32>,
};

fn model_world(vertex: ModelVertex, instance: ModelInstance) -> vec3<f32> {
    let local = vertex.position * instance.base.y * vec3<f32>(1.0, instance.base.z, 1.0);
    let c = instance.placement.z;
    let s = instance.placement.w;
    return vec3<f32>(
        instance.placement.x + c * local.x - s * local.y,
        instance.placement.y + s * local.x + c * local.y,
        instance.base.x + local.z,
    );
}

// Направление модели, повёрнутое вместе с объектом; увеличение его длины не меняет.
fn model_direction(direction: vec3<f32>, instance: ModelInstance) -> vec3<f32> {
    let c = instance.placement.z;
    let s = instance.placement.w;
    return vec3<f32>(c * direction.x - s * direction.y, s * direction.x + c * direction.y, direction.z);
}

@vertex
fn vs_shadow_model(vertex: ModelVertex, instance: ModelInstance) -> @builtin(position) vec4<f32> {
    return globals.light_view_proj * vec4<f32>(model_world(vertex, instance), 1.0);
}

struct ShadowMaskOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) vertex_alpha: f32,
};

// Тень части `MASK`: той же формы, но точки с прозрачностью меньше порога в неё не попадают.
@vertex
fn vs_shadow_model_mask(vertex: ModelVertex, instance: ModelInstance) -> ShadowMaskOutput {
    var out: ShadowMaskOutput;
    out.clip_position = globals.light_view_proj * vec4<f32>(model_world(vertex, instance), 1.0);
    out.uv = vertex.uv;
    out.vertex_alpha = vertex.color.a;
    return out;
}

@fragment
fn fs_shadow_model_mask(in: ShadowMaskOutput) {
    let base_sample = textureSample(model_base_texture, model_base_sampler, in.uv);
    if (model_material.base.a * in.vertex_alpha * base_sample.a < model_material.alpha.y) {
        discard;
    }
}

// На сколько поднимается прозрачность выреза `MASK` за каждую ступень уменьшения картинки цвета, которую берёт
// видеокарта: издали листья с просветами усредняются ниже порога, и листва дерева пропала бы целиком, а с подъёмом
// карточка веточки становится пятном цвета листвы и крона читается сплошной. Вблизи, на первой ступени, прозрачность
// прежняя. Тень не поднимается: с подъёмом крона сплошь затеняла сама себя и темнела, а без него тень листвы
// видна на ближнем и среднем виде игры.
const MASK_BOOST_PER_LEVEL: f32 = 0.25;

// Прозрачность выреза в точке `uv`, поднятая по ступени уменьшения картинки цвета — её видеокарта выбирает по тому,
// сколько точек картинки ложится в одну точку экрана. Производные — до любых ветвлений.
fn mask_alpha(alpha: f32, uv: vec2<f32>) -> f32 {
    let texels = uv * vec2<f32>(textureDimensions(model_base_texture));
    let dx = dpdx(texels);
    let dy = dpdy(texels);
    let level = max(0.0, 0.5 * log2(max(max(dot(dx, dx), dot(dy, dy)), 1e-8)));
    return alpha * (1.0 + level * MASK_BOOST_PER_LEVEL);
}

struct ModelOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) color: vec4<f32>,
};

@vertex
fn vs_model(vertex: ModelVertex, instance: ModelInstance) -> ModelOutput {
    let world = model_world(vertex, instance);
    var out: ModelOutput;
    out.clip_position = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    // При растяжении вдоль +Y нормаль идёт через обратное растяжение, касательная — через само.
    let stretch = vec3<f32>(1.0, instance.base.z, 1.0);
    out.normal = model_direction(normalize(vertex.normal / stretch), instance);
    out.uv = vertex.uv;
    out.tangent = vec4<f32>(model_direction(normalize(vertex.tangent.xyz * stretch), instance), vertex.tangent.w);
    out.color = vertex.color;
    return out;
}

// Поверхность `albedo` с металлом `metallic`: металл не рассеивает свет, а отражает им с цветом поверхности
// (F0 — цвет, а не 0,04). Предметов вокруг для отражения нет, поэтому гладкий металл отражает небо ровным
// светом, слабее по мере шероховатости; блик солнца — тот же GGX, что у остальных поверхностей.
fn shade_pbr(
    albedo: vec3<f32>,
    metallic: f32,
    roughness: f32,
    ao: f32,
    world: vec3<f32>,
    normal: vec3<f32>,
    visible: f32,
) -> vec3<f32> {
    let to_sun = globals.sun.xyz;
    let n_l = max(dot(normal, to_sun), 0.0);
    let sun = globals.sun_light.rgb * (n_l * visible);
    let sky = globals.sky_light.rgb * ((1.0 + normal.z) * 0.5 * ao);

    let to_eye = normalize(globals.eye.xyz - world);
    let half_way = normalize(to_sun + to_eye);
    let n_v = max(dot(normal, to_eye), 0.0);
    let n_h = max(dot(normal, half_way), 0.0);
    let v_h = max(dot(to_eye, half_way), 0.0);
    let alpha = max(roughness * roughness, 0.002);
    let alpha2 = alpha * alpha;
    let denominator = n_h * n_h * (alpha2 - 1.0) + 1.0;
    let distribution = alpha2 / (PI * denominator * denominator);
    let k = alpha * 0.5;
    let visibility = 0.25 / ((n_l * (1.0 - k) + k) * (n_v * (1.0 - k) + k));
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - v_h, 0.0, 1.0), 5.0);
    let glint = globals.sun_light.rgb * fresnel * (PI * distribution * visibility * n_l * visible);
    let reflected_sky = globals.sky_light.rgb * f0 * ((1.0 + normal.z) * 0.5 * ao * (1.0 - roughness));

    return albedo * (1.0 - metallic) * (sun + sky) + glint + reflected_sky;
}

// Точка модели: освещённый цвет после кривой яркости и прозрачность точки — `baseColorFactor` на картинку цвета
// на цвет вершины, все три по альфе. Режим материала решает, что с прозрачностью делать, точка входа.
fn model_point(in: ModelOutput) -> vec4<f32> {
    // Выборки до любых ветвлений: одна текстура — одна выборка.
    let base_sample = textureSample(model_base_texture, model_base_sampler, in.uv);
    let metal_sample = textureSample(model_metal_texture, model_metal_sampler, in.uv);
    let normal_sample = textureSample(model_normal_texture, model_normal_sampler, in.uv);
    let occlusion_sample = textureSample(model_occlusion_texture, model_occlusion_sampler, in.uv);

    let albedo = model_material.base.rgb * in.color.rgb * base_sample.rgb;
    let metallic = clamp(model_material.factors.x * metal_sample.b, 0.0, 1.0);
    let roughness = clamp(model_material.factors.y * metal_sample.g, 0.0, 1.0);
    let ao = 1.0 + model_material.factors.w * (occlusion_sample.r - 1.0);

    let n = normalize(in.normal);
    var tangent = in.tangent.xyz - n * dot(n, in.tangent.xyz);
    if (dot(tangent, tangent) < 1e-8) {
        let helper = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.9);
        tangent = cross(helper, n);
    }
    tangent = normalize(tangent);
    let bitangent = cross(n, tangent) * in.tangent.w;
    let tilt = (normal_sample.xyz * 2.0 - vec3<f32>(1.0)) * vec3<f32>(model_material.factors.z, model_material.factors.z, 1.0);
    let normal = normalize(tangent * tilt.x + bitangent * tilt.y + n * tilt.z);

    let visible = shadow_lit(in.world, normal);
    let alpha = model_material.base.a * in.color.a * base_sample.a;
    return vec4<f32>(finish(shade_pbr(albedo, metallic, roughness, ao, in.world, normal, visible)), alpha);
}

// `OPAQUE`: прозрачность не учитывается.
@fragment
fn fs_model(in: ModelOutput) -> @location(0) vec4<f32> {
    let point = model_point(in);
    return vec4<f32>(point.rgb, 1.0);
}

// `MASK`: точки с прозрачностью меньше порога не рисуются, остальные сплошные; издали прозрачность поднята —
// `mask_alpha`.
@fragment
fn fs_model_mask(in: ModelOutput) -> @location(0) vec4<f32> {
    let point = model_point(in);
    if (mask_alpha(point.a, in.uv) < model_material.alpha.y) {
        discard;
    }
    return vec4<f32>(point.rgb, 1.0);
}

// Часть со смешиванием: цвет с прозрачностью точки ложится на то, что уже нарисовано; конвейер смешивает по
// альфе. Так же рисуются куски дорог, у чьих материалов любой режим: `OPAQUE` ложится сплошным, `MASK` с
// вырезом.
@fragment
fn fs_model_blend(in: ModelOutput) -> @location(0) vec4<f32> {
    let point = model_point(in);
    let mode = model_material.alpha.x;
    let cut = mask_alpha(point.a, in.uv);
    if (mode > 0.5 && mode < 1.5 && cut < model_material.alpha.y) {
        discard;
    }
    return vec4<f32>(point.rgb, select(1.0, point.a, mode > 1.5));
}

// --- Перекрёстки дорог ----------------------------------------------------------------------

// Вершина изогнутой копии перекрёстка уже в осях сцены: экземпляров нет. Копия рисуется точкой входа `fs_model_blend`.
@vertex
fn vs_junction(vertex: ModelVertex) -> ModelOutput {
    var out: ModelOutput;
    out.clip_position = globals.view_proj * vec4<f32>(vertex.position, 1.0);
    out.world = vertex.position;
    out.normal = vertex.normal;
    out.uv = vertex.uv;
    out.tangent = vertex.tangent;
    out.color = vertex.color;
    return out;
}

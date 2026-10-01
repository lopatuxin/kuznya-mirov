// «Трёхмерная сцена», «Свет и материалы»: рельеф с водой и покрытиями, фигуры, плитки и плоские
// объекты на земле и карта теней от солнца. Проходы: тени (`vs_shadow` — фигуры, `vs_shadow_terrain`
// — рельеф; только глубина от солнца), затем рельеф (`vs_terrain`, `fs_terrain`), вода (`fs_water`),
// фигуры (`vs_shape`, `fs_shape`) и плитки с плоскими объектами (`vs_ground`, `fs_ground`), которые
// лежат на рельефе и верху настилов. Надписи, полоски и интерфейс рисует `rect.wgsl` отдельным
// проходом без глубины.
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
    // тени, в клетках.
    shade: vec4<f32>,
    // Место камеры: от него считается блик.
    eye: vec4<f32>,
    // Цвет и сила солнца и неба в линейной яркости.
    sun_light: vec4<f32>,
    sky_light: vec4<f32>,
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

// Поверхность цвета `albedo` (линейная яркость) в точке `world` с нормалью `normal`: свет солнца по
// косинусу и по тени, свет неба сверху сильнее, чем сбоку, и только там, куда он доходит (`ao`),
// блик солнца — модель GGX, геометрия Смита — Шлика, отражение по Шлику с F0 = 0,04.
fn shade(
    albedo: vec3<f32>,
    world: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    ao: f32,
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
    let fresnel = 0.04 + 0.96 * pow(clamp(1.0 - v_h, 0.0, 1.0), 5.0);
    // `sun_light` — освещённость, уже делённая на π (рассеянный свет — `albedo × sun` без 1/π), поэтому
    // блик GGX, как в glTF, умножается на π обратно.
    let glint = globals.sun_light.rgb * (PI * distribution * visibility * fresnel * n_l * visible);

    return albedo * (sun + sky) + glint;
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

struct TerrainVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
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
};

@vertex
fn vs_terrain(vertex: TerrainVertex) -> TerrainOutput {
    var out: TerrainOutput;
    out.clip_position = globals.view_proj * vec4<f32>(vertex.position, 1.0);
    out.world = vertex.position;
    out.normal = vertex.normal;
    out.color = vertex.color;
    return out;
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
// землю и гасит свет неба там, куда он не доходит.
@fragment
fn fs_terrain(in: TerrainOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let ddx = dpdx(in.world);
    let ddy = dpdy(in.world);
    let visible = shadow_lit(in.world, normal);
    let tint = terrain_tint(in.world);
    if (covers.head.x < 0.5) {
        let albedo = srgb_to_linear(in.color) * tint.rgb * 2.0;
        return vec4<f32>(finish(shade(albedo, in.world, normal, 1.0, tint.a, visible)), 1.0);
    }
    let surface = cover_surface(in.world, normal, ddx, ddy);
    let albedo = surface.albedo * tint.rgb * 2.0;
    let color = shade(albedo, in.world, surface.normal, surface.roughness, surface.ao * tint.a, visible);
    return vec4<f32>(finish(color), 1.0);
}

@fragment
fn fs_water(in: TerrainOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let visible = shadow_lit(in.world, normal);
    return vec4<f32>(finish(shade_matte(srgb_to_linear(in.color), in.world, normal, visible)), 1.0);
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
    // x — лист атласа, y — признак сглаживания.
    @location(6) sheet: vec2<f32>,
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
    out.world = vertex.position;
    out.normal = vertex.normal;
    return out;
}

// Цвет картинки уже умножен на прозрачность: свет считается по цвету без неё, затем цвет умножается
// снова.
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
    return vec4<f32>(lit * base.a, base.a);
}

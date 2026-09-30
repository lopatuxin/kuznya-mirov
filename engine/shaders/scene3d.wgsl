// «Трёхмерная сцена»: рельеф с водой, фигуры, плитки и плоские объекты на земле и карта теней от
// солнца. Проходы: тени (`vs_shadow` — фигуры, `vs_shadow_terrain` — рельеф; только глубина от
// солнца), затем рельеф и вода (`vs_terrain`, `fs_terrain`), фигуры (`vs_shape`, `fs_shape`) и
// плитки с плоскими объектами (`vs_ground`, `fs_ground`), которые лежат на рельефе и верху настилов.
// Надписи, полоски и интерфейс рисует `rect.wgsl` отдельным проходом без глубины.
//
// Ловушки WebGL2: одна текстура — одна выборка, поэтому атлас читается тем же способом, что в
// `rect.wgsl` (точка и линейная выборка, выбор через `select`), а карта теней — только выборкой
// сравнением.

struct Globals3d {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    // xyz — направление от земли к солнцу, w — синус высоты солнца.
    sun: vec4<f32>,
    // x — насколько темна тень; y — единица глубины карты теней на клетку сцены; z — сдвиг точки
    // вдоль нормали при выборке тени, в клетках.
    shade: vec4<f32>,
};

const ATLAS_SIZE: f32 = 2048.0;

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

// Освещённость точки: `(1 − shadow) + shadow × освещено × min(1, max(0, cos угла к солнцу) / sin
// высоты солнца)`. Сравнение всегда выполняется, а вне карты его результат отбрасывает `select`:
// выборка не терпит ветвления.
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

fn lighting(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let strength = globals.shade.x;
    let facing = min(1.0, max(0.0, dot(normal, globals.sun.xyz)) / globals.sun.w);
    return (1.0 - strength) + strength * shadow_lit(world, normal) * facing;
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

@fragment
fn fs_terrain(in: TerrainOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    return vec4<f32>(in.color * lighting(in.world, normal), 1.0);
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
    return vec4<f32>(in.color * lighting(in.world, normal), 1.0);
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

@fragment
fn fs_ground(in: GroundOutput) -> @location(0) vec4<f32> {
    let clamped = clamp(in.uv_px, in.uv_min, in.uv_max);
    let uv = clamped / ATLAS_SIZE;
    let nearest = textureLoad(atlas_texture, vec2<i32>(floor(clamped)), in.atlas_layer, 0);
    let linear = textureSample(atlas_texture, atlas_sampler_linear, uv, in.atlas_layer);
    let sampled = select(nearest, linear, in.smooth_sample > 0.5);
    let base = in.color * sampled;
    let light = lighting(in.world, normalize(in.normal));
    return vec4<f32>(base.rgb * light, base.a);
}

// «Картинки» → «Видео»: проход, который переносит кадр видео в его место в атласе. Файл видео — двойной
// высоты: сверху цвет, снизу маска того же размера. Проход рисует в место видео цвет верхней половины,
// умноженный на яркость маски, и саму яркость — так атлас хранит точки картинки (`atlas::blit`).
@group(0) @binding(0)
var video_texture: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) unit: vec2<f32>,
};

// Четыре вершины полосой треугольников; окно вывода — ровно место видео, так что единичный
// прямоугольник покрывает его точка в точку.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let unit = vec2<f32>(f32(index & 1u), f32(index >> 1u));
    var out: VertexOutput;
    out.clip_position = vec4<f32>(unit.x * 2.0 - 1.0, 1.0 - unit.y * 2.0, 0.0, 1.0);
    out.unit = unit;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let file = vec2<i32>(textureDimensions(video_texture));
    let frame = vec2<i32>(file.x, file.y / 2);
    let at = min(vec2<i32>(in.unit * vec2<f32>(frame)), frame - vec2<i32>(1, 1));
    let color = textureLoad(video_texture, at, 0);
    let mask = textureLoad(video_texture, at + vec2<i32>(0, frame.y), 0);
    // Яркость точки маски, а не один канал: цвет соседней половины сквозь сжатие в маску не течёт.
    let alpha = dot(mask.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(color.rgb * alpha, alpha);
}

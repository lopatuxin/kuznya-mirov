//! «Ветер и частицы» → «Частицы»: встроенные рисунки частиц — мягкая точка, клуб дыма, искра и четыре
//! кадра листа. Движок рисует их сам при сборке атласа; цвет — в самом рисунке, края прозрачные.

use crate::core::particles::ParticleShape;

use super::atlas::AtlasImage;

pub(super) const DOT_SIZE: u32 = 64;
pub(super) const SMOKE_SIZE: u32 = 64;
pub(super) const SPARK_SIZE: u32 = 32;
/// Сторона одного кадра листа; кадры лежат в ряд слева направо.
pub(super) const LEAF_SIZE: u32 = 32;

/// Средне-серый, а не светлый: светлый клуб пропадал на белых облаках неба платформера.
const SMOKE_TOP: [f32; 3] = [160.0, 160.0, 166.0];
const SMOKE_BOTTOM: [f32; 3] = [118.0, 118.0, 126.0];
const SMOKE_OPACITY: f32 = 0.9;
/// Комки клуба: середина и радиус в долях стороны рисунка.
const SMOKE_LUMPS: [(f32, f32, f32); 7] = [
    (0.50, 0.55, 0.24),
    (0.31, 0.58, 0.17),
    (0.69, 0.58, 0.17),
    (0.39, 0.40, 0.17),
    (0.60, 0.37, 0.18),
    (0.43, 0.70, 0.15),
    (0.62, 0.71, 0.14),
];

const SPARK_CORE: [f32; 3] = [255.0, 245.0, 210.0];
const SPARK_HALO: [f32; 3] = [255.0, 170.0, 60.0];

const LEAF_COLORS: [[f32; 3]; 4] = [
    [95.0, 140.0, 45.0],
    [160.0, 165.0, 50.0],
    [215.0, 175.0, 45.0],
    [195.0, 110.0, 40.0],
];
const LEAF_VEIN: [f32; 3] = [235.0, 230.0, 170.0];
/// Поворот листа в каждом кадре, градусов по часовой стрелке, и изгиб его средней линии.
const LEAF_TURNS: [(f32, f32); 4] = [(-35.0, 0.14), (20.0, -0.12), (75.0, 0.13), (130.0, -0.14)];
/// Половина длины и наибольшая половина ширины листа в долях половины кадра.
const LEAF_HALF_LENGTH: f32 = 0.86;
const LEAF_HALF_WIDTH: f32 = 0.3;

/// Рисунок `shape` в прямых, не умноженных на прозрачность цветах — как картинка игры до атласа.
pub(super) fn draw(shape: ParticleShape) -> AtlasImage {
    match shape {
        ParticleShape::Dot => paint(DOT_SIZE, DOT_SIZE, dot),
        ParticleShape::Smoke => paint(SMOKE_SIZE, SMOKE_SIZE, smoke),
        ParticleShape::Spark => paint(SPARK_SIZE, SPARK_SIZE, spark),
        ParticleShape::Leaf => paint(LEAF_SIZE * shape.frames(), LEAF_SIZE, |x, y| {
            let frame = (x / LEAF_SIZE) as usize;
            leaf(x % LEAF_SIZE, y, frame)
        }),
    }
}

/// `point(x, y)` — цвет точки от 0 до 255 и её прозрачность от 0 до 1.
fn paint(width: u32, height: u32, point: impl Fn(u32, u32) -> [f32; 4]) -> AtlasImage {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let [r, g, b, a] = point(x, y);
            pixels.extend([r, g, b].map(|channel| channel.round().clamp(0.0, 255.0) as u8));
            pixels.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    AtlasImage {
        width,
        height,
        pixels,
    }
}

/// Середина точки `(x, y)` в долях стороны `size`: от 0 до 1.
fn unit(x: u32, y: u32, size: u32) -> (f32, f32) {
    let size = size as f32;
    ((x as f32 + 0.5) / size, (y as f32 + 0.5) / size)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Число от −1 до 1 в узле решётки — одно и то же при каждой сборке.
fn lattice(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0
}

/// Плавный шум от −1 до 1: узлы решётки, между ними — сглаженно.
fn noise(x: f32, y: f32) -> f32 {
    let (cx, cy) = (x.floor(), y.floor());
    let (ix, iy) = (cx as i32, cy as i32);
    let (tx, ty) = (smoothstep(0.0, 1.0, x - cx), smoothstep(0.0, 1.0, y - cy));
    let top = lattice(ix, iy) + (lattice(ix + 1, iy) - lattice(ix, iy)) * tx;
    let bottom = lattice(ix, iy + 1) + (lattice(ix + 1, iy + 1) - lattice(ix, iy + 1)) * tx;
    top + (bottom - top) * ty
}

/// Белая точка: прозрачность `(1 − r²)²` по доле радиуса `r`.
fn dot(x: u32, y: u32) -> [f32; 4] {
    let (u, v) = unit(x, y, DOT_SIZE);
    let (dx, dy) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
    let falloff = (1.0 - (dx * dx + dy * dy)).max(0.0);
    [255.0, 255.0, 255.0, falloff * falloff]
}

/// Клуб дыма: объединение размытых комков с рваным шумом краем; каждый комок светлее сверху, весь
/// клуб — светлее сверху, темнее снизу.
fn smoke(x: u32, y: u32) -> [f32; 4] {
    let (u, v) = unit(x, y, SMOKE_SIZE);
    let ragged = 1.0 + 0.13 * noise(u * 6.0, v * 6.0) + 0.05 * noise(u * 13.0 + 5.0, v * 13.0);
    let mut clear = 1.0;
    let mut lit = 0.0;
    let mut weight = 0.0;
    for (cx, cy, radius) in SMOKE_LUMPS {
        let reach = (u - cx).hypot(v - cy) / radius * ragged;
        let lump = 1.0 - smoothstep(0.2, 1.1, reach);
        clear *= 1.0 - lump;
        lit += lump * ((cy - v) / radius).clamp(-1.0, 1.0);
        weight += lump;
    }
    let shade = 1.0 + 0.07 * lit / weight.max(1e-6) + 0.04 * noise(u * 4.0 + 11.0, v * 4.0 + 3.0);
    let [r, g, b] = mix(SMOKE_TOP, SMOKE_BOTTOM, smoothstep(0.25, 0.85, v));
    [
        r * shade,
        g * shade,
        b * shade,
        SMOKE_OPACITY * (1.0 - clear),
    ]
}

/// Искра: горячее бело-жёлтое ядро, вокруг жёлто-оранжевый ореол, к краю в ноль.
fn spark(x: u32, y: u32) -> [f32; 4] {
    let (u, v) = unit(x, y, SPARK_SIZE);
    let r = (u * 2.0 - 1.0).hypot(v * 2.0 - 1.0);
    let alpha = (1.0 - smoothstep(0.08, 1.0, r)).powf(1.3);
    let [red, green, blue] = mix(SPARK_CORE, SPARK_HALO, smoothstep(0.06, 0.4, r));
    [red, green, blue, alpha]
}

/// Лист кадра `frame`: вытянутый, с острыми концами, изогнутый, со светлой жилкой посередине.
fn leaf(x: u32, y: u32, frame: usize) -> [f32; 4] {
    let (u, v) = unit(x, y, LEAF_SIZE);
    let (px, py) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
    let (degrees, bend) = LEAF_TURNS[frame];
    let (sin, cos) = degrees.to_radians().sin_cos();
    let along = (px * cos + py * sin) / LEAF_HALF_LENGTH;
    let across = -px * sin + py * cos - bend * (along * along - 0.5);
    let half_width = LEAF_HALF_WIDTH * (1.0 - along * along);
    let point = 2.0 / LEAF_SIZE as f32;
    let alpha = (0.5 - (across.abs() - half_width) / point).clamp(0.0, 1.0);
    let side = 0.85 + 0.15 * smoothstep(-half_width, half_width, across);
    let vein = (1.0 - across.abs() / (0.8 * point)).clamp(0.0, 1.0)
        * (1.0 - smoothstep(0.65, 0.95, along.abs()));
    let color = LEAF_COLORS[frame].map(|channel| channel * side);
    let [r, g, b] = mix(color, LEAF_VEIN, 0.45 * vein);
    [r, g, b, alpha]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &AtlasImage, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * image.width + x) * 4) as usize;
        image.pixels[start..start + 4]
            .try_into()
            .expect("четыре канала")
    }

    #[test]
    fn every_picture_has_the_size_of_its_place() {
        for (shape, width, height) in [
            (ParticleShape::Dot, DOT_SIZE, DOT_SIZE),
            (ParticleShape::Smoke, SMOKE_SIZE, SMOKE_SIZE),
            (ParticleShape::Spark, SPARK_SIZE, SPARK_SIZE),
            (ParticleShape::Leaf, LEAF_SIZE * 4, LEAF_SIZE),
        ] {
            let image = draw(shape);
            assert_eq!((image.width, image.height), (width, height), "{shape:?}");
            assert_eq!(image.pixels.len(), (width * height * 4) as usize);
        }
    }

    #[test]
    fn the_smoke_is_lighter_on_top_and_ragged_at_the_edge() {
        let image = draw(ParticleShape::Smoke);
        let top = pixel(&image, 30, 18);
        let bottom = pixel(&image, 32, 46);
        assert!(top[3] > 0 && bottom[3] > 0, "{top:?} {bottom:?}");
        assert!(top[0] > bottom[0] + 20, "{top:?} {bottom:?}");
        let half = SMOKE_SIZE / 2;
        let reach = |dx: i32, dy: i32| {
            (1..half as i32)
                .take_while(|step| {
                    let x = (half as i32 + dx * step) as u32;
                    let y = (half as i32 + dy * step) as u32;
                    pixel(&image, x, y)[3] > 64
                })
                .count()
        };
        let reaches: Vec<usize> = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1)]
            .into_iter()
            .map(|(dx, dy)| reach(dx, dy))
            .collect();
        let (low, high) = (reaches.iter().min(), reaches.iter().max());
        assert!(high > low, "край неровный, не круг: {reaches:?}");
    }

    #[test]
    fn a_leaf_turns_from_frame_to_frame() {
        let image = draw(ParticleShape::Leaf);
        let shapes: Vec<Vec<bool>> = (0..4)
            .map(|frame| {
                (0..LEAF_SIZE * LEAF_SIZE)
                    .map(|i| {
                        pixel(&image, frame * LEAF_SIZE + i % LEAF_SIZE, i / LEAF_SIZE)[3] > 128
                    })
                    .collect()
            })
            .collect();
        for (a, b) in [(0, 1), (1, 2), (2, 3), (0, 3)] {
            assert_ne!(shapes[a], shapes[b], "кадры {a} и {b}");
        }
    }
}

//! Мелкая трёхмерная арифметика для камеры, лучей и тени: векторы в `f64` (ход игры и мышь),
//! матрицы в `f32` по столбцам — так их читает видеокарта (`mat4x4<f32>` в WGSL).

pub type Vec3 = [f64; 3];

/// Матрица 4×4 по столбцам: `m[столбец][строка]`.
pub type Mat4 = [[f32; 4]; 4];

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn normalize(a: Vec3) -> Vec3 {
    let len = dot(a, a).sqrt();
    if len == 0.0 { a } else { scale(a, 1.0 / len) }
}

pub const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `a * b`: сначала действует `b`, потом `a`.
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0_f32; 4]; 4];
    for (col, out_col) in out.iter_mut().enumerate() {
        for (row, cell) in out_col.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    out
}

/// Вид: камера в `eye`, оси `right`, `up`, `forward` (единичные, взаимно перпендикулярные);
/// в её пространстве взгляд идёт вдоль −Z.
pub fn view(eye: Vec3, right: Vec3, up: Vec3, forward: Vec3) -> Mat4 {
    let back = scale(forward, -1.0);
    let row = |axis: Vec3| [axis[0] as f32, axis[1] as f32, axis[2] as f32];
    let (r, u, b) = (row(right), row(up), row(back));
    [
        [r[0], u[0], b[0], 0.0],
        [r[1], u[1], b[1], 0.0],
        [r[2], u[2], b[2], 0.0],
        [
            -dot(right, eye) as f32,
            -dot(up, eye) as f32,
            -dot(back, eye) as f32,
            1.0,
        ],
    ]
}

/// Перспектива с глубиной `0..1` (WebGPU): `fov_y` — угол по высоте в радианах.
pub fn perspective(fov_y: f64, aspect: f64, near: f64, far: f64) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    let range = near - far;
    [
        [(f / aspect) as f32, 0.0, 0.0, 0.0],
        [0.0, f as f32, 0.0, 0.0],
        [0.0, 0.0, (far / range) as f32, -1.0],
        [0.0, 0.0, (near * far / range) as f32, 0.0],
    ]
}

/// Прямоугольная проекция с глубиной `0..1`: пространство камеры `[left, right] × [bottom, top]`,
/// вдоль взгляда (−Z) от `near` до `far`.
pub fn ortho(left: f64, right: f64, bottom: f64, top: f64, near: f64, far: f64) -> Mat4 {
    let (w, h, d) = (right - left, top - bottom, near - far);
    [
        [(2.0 / w) as f32, 0.0, 0.0, 0.0],
        [0.0, (2.0 / h) as f32, 0.0, 0.0],
        [0.0, 0.0, (1.0 / d) as f32, 0.0],
        [
            (-(right + left) / w) as f32,
            (-(top + bottom) / h) as f32,
            (near / d) as f32,
            1.0,
        ],
    ]
}

/// `m * (x, y, z, 1)`, обе части делятся на `w`: точка в пространстве выреза после проекции.
pub fn transform_point(m: &Mat4, p: Vec3) -> [f64; 4] {
    let v = [p[0] as f32, p[1] as f32, p[2] as f32, 1.0];
    let out: [f32; 4] = std::array::from_fn(|row| (0..4).map(|k| m[k][row] * v[k]).sum());
    [out[0] as f64, out[1] as f64, out[2] as f64, out[3] as f64]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_neutral_for_products() {
        let m = perspective(0.6, 1.5, 0.1, 100.0);
        assert_eq!(mul(&IDENTITY, &m), m);
        assert_eq!(mul(&m, &IDENTITY), m);
    }

    #[test]
    fn perspective_maps_the_near_and_far_planes_to_depth_zero_and_one() {
        let m = perspective(0.6, 1.0, 0.5, 50.0);
        let near = transform_point(&m, [0.0, 0.0, -0.5]);
        let far = transform_point(&m, [0.0, 0.0, -50.0]);
        assert!((near[2] / near[3]).abs() < 1e-5, "{near:?}");
        assert!((far[2] / far[3] - 1.0).abs() < 1e-5, "{far:?}");
    }

    #[test]
    fn ortho_maps_the_box_corners_to_the_clip_cube() {
        let m = ortho(-2.0, 4.0, -1.0, 3.0, 1.0, 9.0);
        let a = transform_point(&m, [-2.0, -1.0, -1.0]);
        let b = transform_point(&m, [4.0, 3.0, -9.0]);
        for (got, want) in a.iter().zip([-1.0, -1.0, 0.0, 1.0]) {
            assert!((got - want).abs() < 1e-6, "{a:?}");
        }
        for (got, want) in b.iter().zip([1.0, 1.0, 1.0, 1.0]) {
            assert!((got - want).abs() < 1e-6, "{b:?}");
        }
    }
}

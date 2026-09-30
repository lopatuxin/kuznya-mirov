//! «Трёхмерная сцена» → «Объект в объёме»: простые фигуры — коробка, цилиндр, капсула, шар. Фигура
//! заполняет прямоугольник объекта на земле и `height` над ним; цилиндр, капсула и шар растянуты по
//! этому объёму. Здесь — то, что нужно и ходу игры, и рисованию, и ничему из них не нужна видеокарта:
//! место фигуры (`Body`), сетки единичных фигур, попадание луча по форме и охватывающий
//! прямоугольник на экране.

use std::f64::consts::TAU;
use std::sync::OnceLock;

use super::camera::Camera3d;
use super::math3::{self, Vec3};
use super::property;
use super::value::{Rotation, Shape, Vec2};
use super::world::World;

/// Число делений по кругу у цилиндра, капсулы и шара.
pub const SEGMENTS: usize = 24;
/// Колец от полюса до экватора у шара и у полушария капсулы.
const RINGS_PER_QUARTER: usize = 6;

/// Точка сетки единичной фигуры. `position` — в единичном объёме: `x`, `y` от −0,5 до 0,5, `z` от 0
/// до 1; в мире точка стоит в `(x·ширина, y·глубина, z·height + cap.0·cap_height)` — `cap.0` двигает
/// точку полушария капсулы на долю его высоты, у остальных фигур он ноль. `normal` — нормаль
/// единичной фигуры, в мире её делят на размеры (нормали при растяжении), у полушария по высоте
/// `2·cap_height` (`cap.1` — признак полушария).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub cap: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u16>,
}

/// Кольцо вращения: радиус, высота основания, сдвиг полушария, признак полушария и нормаль.
struct Ring {
    radius: f32,
    z: f32,
    cap_offset: f32,
    is_cap: f32,
    normal_horizontal: f32,
    normal_z: f32,
}

fn ring(radius: f32, z: f32, normal_horizontal: f32, normal_z: f32) -> Ring {
    Ring {
        radius,
        z,
        cap_offset: 0.0,
        is_cap: 0.0,
        normal_horizontal,
        normal_z,
    }
}

/// Сетка, полученная вращением колец вокруг вертикальной оси: соседние кольца соединены полосой.
fn revolve(rings: &[Ring]) -> Mesh {
    let mut vertices = Vec::with_capacity(rings.len() * SEGMENTS);
    for r in rings {
        for j in 0..SEGMENTS {
            let angle = TAU * j as f64 / SEGMENTS as f64;
            let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
            vertices.push(MeshVertex {
                position: [r.radius * cos, r.radius * sin, r.z],
                normal: [
                    r.normal_horizontal * cos,
                    r.normal_horizontal * sin,
                    r.normal_z,
                ],
                cap: [r.cap_offset, r.is_cap],
            });
        }
    }
    let mut indices = Vec::new();
    for i in 0..rings.len().saturating_sub(1) {
        for j in 0..SEGMENTS {
            let next = (j + 1) % SEGMENTS;
            let a = (i * SEGMENTS + j) as u16;
            let b = (i * SEGMENTS + next) as u16;
            let c = ((i + 1) * SEGMENTS + j) as u16;
            let d = ((i + 1) * SEGMENTS + next) as u16;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    Mesh { vertices, indices }
}

fn box_mesh() -> Mesh {
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    // Каждая грань: нормаль и два направления по ней (u × v = нормаль).
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    for (normal, u, v) in faces {
        let base = vertices.len() as u16;
        for (su, sv) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let mut position = [0.0_f32; 3];
            for axis in 0..3 {
                let centered = 0.5 * normal[axis] + 0.5 * su * u[axis] + 0.5 * sv * v[axis];
                position[axis] = if axis == 2 { centered + 0.5 } else { centered };
            }
            vertices.push(MeshVertex {
                position,
                normal,
                cap: [0.0, 0.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh { vertices, indices }
}

fn cylinder_mesh() -> Mesh {
    revolve(&[
        ring(0.0, 0.0, 0.0, -1.0),
        ring(0.5, 0.0, 0.0, -1.0),
        ring(0.5, 0.0, 1.0, 0.0),
        ring(0.5, 1.0, 1.0, 0.0),
        ring(0.5, 1.0, 0.0, 1.0),
        ring(0.0, 1.0, 0.0, 1.0),
    ])
}

fn sphere_mesh() -> Mesh {
    let steps = 2 * RINGS_PER_QUARTER;
    let rings: Vec<Ring> = (0..=steps)
        .map(|k| {
            let phi = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * k as f64 / steps as f64;
            ring(
                0.5 * phi.cos() as f32,
                0.5 + 0.5 * phi.sin() as f32,
                phi.cos() as f32,
                phi.sin() as f32,
            )
        })
        .collect();
    revolve(&rings)
}

fn capsule_mesh() -> Mesh {
    let quarter = |bottom: bool| {
        (0..=RINGS_PER_QUARTER).map(move |k| {
            let t = std::f64::consts::FRAC_PI_2 * k as f64 / RINGS_PER_QUARTER as f64;
            let phi = if bottom {
                t - std::f64::consts::FRAC_PI_2
            } else {
                t
            };
            let (sin, cos) = (phi.sin() as f32, phi.cos() as f32);
            Ring {
                radius: 0.5 * cos,
                z: if bottom { 0.0 } else { 1.0 },
                cap_offset: if bottom { 1.0 + sin } else { sin - 1.0 },
                is_cap: 1.0,
                normal_horizontal: cos,
                normal_z: sin,
            }
        })
    };
    let rings: Vec<Ring> = quarter(true).chain(quarter(false)).collect();
    revolve(&rings)
}

/// Сетка единичной фигуры — строится один раз.
pub fn unit_mesh(shape: Shape) -> &'static Mesh {
    static MESHES: OnceLock<[Mesh; 4]> = OnceLock::new();
    let meshes =
        MESHES.get_or_init(|| [box_mesh(), cylinder_mesh(), capsule_mesh(), sphere_mesh()]);
    match shape {
        Shape::Box => &meshes[0],
        Shape::Cylinder => &meshes[1],
        Shape::Capsule => &meshes[2],
        Shape::Sphere => &meshes[3],
    }
}

/// Фигура, стоящая на земле: место, размеры, поворот. Всё в единицах сцены.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub shape: Shape,
    /// Середина прямоугольника на земле.
    pub center: Vec2,
    /// Ширина вдоль `x` фигуры, глубина вдоль её `y`.
    pub size: Vec2,
    pub height: f64,
    /// Высота основания фигуры над нулём сцены.
    pub base: f64,
    pub sin: f64,
    pub cos: f64,
}

impl Body {
    /// Фигура объекта `id`; `None`, если у объекта нет `shape`, `position` или `size`.
    /// `height` без значения — одна клетка.
    pub fn of_object(world: &World, id: u32) -> Option<Body> {
        let shape = world.shape(id, property::SHAPE)?;
        let position = world.vec2(id, property::POSITION)?;
        let size = world.vec2(id, property::SIZE)?;
        let height = world.number_like(id, property::HEIGHT).unwrap_or(1.0);
        let (sin, cos) = world
            .rotation(id, property::ROTATION)
            .map_or((0.0, 1.0), Rotation::sin_cos);
        Some(Body {
            shape,
            center: [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0],
            size,
            height,
            base: world.base_z(id),
            sin,
            cos,
        })
    }

    /// Высота полушария капсулы: половина меньшей стороны прямоугольника, но не больше половины
    /// `height`.
    pub fn cap_height(&self) -> f64 {
        (self.size[0].min(self.size[1]) / 2.0).min(self.height / 2.0)
    }

    fn is_solid(&self) -> bool {
        self.size[0] > 0.0 && self.size[1] > 0.0 && self.height > 0.0
    }

    /// Точка сетки в мире — то же, что считает вершинный шейдер.
    pub fn place(&self, vertex: &MeshVertex) -> Vec3 {
        let local = [
            vertex.position[0] as f64 * self.size[0],
            vertex.position[1] as f64 * self.size[1],
            self.base
                + vertex.position[2] as f64 * self.height
                + vertex.cap[0] as f64 * self.cap_height(),
        ];
        self.to_world(local)
    }

    fn to_world(self, local: Vec3) -> Vec3 {
        [
            self.center[0] + self.cos * local[0] - self.sin * local[1],
            self.center[1] + self.sin * local[0] + self.cos * local[1],
            local[2],
        ]
    }

    fn to_local(self, world: Vec3) -> Vec3 {
        let (dx, dy) = (world[0] - self.center[0], world[1] - self.center[1]);
        [
            self.cos * dx + self.sin * dy,
            -self.sin * dx + self.cos * dy,
            world[2] - self.base,
        ]
    }

    fn direction_to_local(&self, direction: Vec3) -> Vec3 {
        [
            self.cos * direction[0] + self.sin * direction[1],
            -self.sin * direction[0] + self.cos * direction[1],
            direction[2],
        ]
    }

    /// Ближайшая точка луча `origin + t·direction` (`t ≥ 0`) в объёме фигуры по её форме.
    pub fn ray_hit(&self, origin: Vec3, direction: Vec3) -> Option<f64> {
        if !self.is_solid() {
            return None;
        }
        let o = self.to_local(origin);
        let d = self.direction_to_local(direction);
        let (hx, hy) = (self.size[0] / 2.0, self.size[1] / 2.0);
        match self.shape {
            Shape::Box => hit_box(o, d, hx, hy, self.height),
            Shape::Cylinder => hit_cylinder(o, d, hx, hy, 0.0, self.height, true),
            Shape::Sphere => hit_ellipsoid(
                o,
                d,
                hx,
                hy,
                self.height / 2.0,
                self.height / 2.0,
                Half::Both,
            ),
            Shape::Capsule => {
                let cap = self.cap_height();
                [
                    hit_cylinder(o, d, hx, hy, cap, self.height - cap, false),
                    hit_ellipsoid(o, d, hx, hy, cap, cap, Half::Below),
                    hit_ellipsoid(o, d, hx, hy, cap, self.height - cap, Half::Above),
                ]
                .into_iter()
                .flatten()
                .min_by(f64::total_cmp)
            }
        }
    }

    /// Прямоугольник, который фигура занимает на экране, по точкам её сетки: `[x0, y0, x1, y1]` в
    /// точках окна. `None`, если хоть одна точка за камерой.
    pub fn screen_rect(&self, camera: &Camera3d) -> Option<[f64; 4]> {
        let mut rect = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for vertex in &unit_mesh(self.shape).vertices {
            let point = camera.project(self.place(vertex))?;
            rect = [
                rect[0].min(point[0]),
                rect[1].min(point[1]),
                rect[2].max(point[0]),
                rect[3].max(point[1]),
            ];
        }
        Some(rect)
    }
}

const NEAR_ZERO: f64 = 1e-12;

fn nearest_positive(roots: [Option<f64>; 2], accept: impl Fn(f64) -> bool) -> Option<f64> {
    roots
        .into_iter()
        .flatten()
        .filter(|&t| t >= 0.0 && accept(t))
        .min_by(f64::total_cmp)
}

/// Корни `a·t² + b·t + c = 0`.
fn quadratic(a: f64, b: f64, c: f64) -> [Option<f64>; 2] {
    if a.abs() < NEAR_ZERO {
        return [None, None];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return [None, None];
    }
    let root = discriminant.sqrt();
    [Some((-b - root) / (2.0 * a)), Some((-b + root) / (2.0 * a))]
}

fn hit_box(o: Vec3, d: Vec3, hx: f64, hy: f64, height: f64) -> Option<f64> {
    let bounds = [(-hx, hx), (-hy, hy), (0.0, height)];
    let (mut near, mut far) = (0.0_f64, f64::INFINITY);
    for axis in 0..3 {
        let (lo, hi) = bounds[axis];
        if d[axis].abs() < NEAR_ZERO {
            if o[axis] < lo || o[axis] > hi {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo - o[axis]) / d[axis], (hi - o[axis]) / d[axis]);
        near = near.max(t0.min(t1));
        far = far.min(t0.max(t1));
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// Вертикальный эллиптический цилиндр между `z0` и `z1`; `capped` — с крышками сверху и снизу.
fn hit_cylinder(o: Vec3, d: Vec3, hx: f64, hy: f64, z0: f64, z1: f64, capped: bool) -> Option<f64> {
    let (ox, oy) = (o[0] / hx, o[1] / hy);
    let (dx, dy) = (d[0] / hx, d[1] / hy);
    let side = nearest_positive(
        quadratic(
            dx * dx + dy * dy,
            2.0 * (ox * dx + oy * dy),
            ox * ox + oy * oy - 1.0,
        ),
        |t| (z0..=z1).contains(&(o[2] + t * d[2])),
    );
    let caps = if capped && d[2].abs() > NEAR_ZERO {
        [z0, z1]
            .into_iter()
            .map(|z| (z - o[2]) / d[2])
            .filter(|&t| {
                let (x, y) = (ox + t * dx, oy + t * dy);
                t >= 0.0 && x * x + y * y <= 1.0
            })
            .min_by(f64::total_cmp)
    } else {
        None
    };
    [side, caps].into_iter().flatten().min_by(f64::total_cmp)
}

#[derive(Clone, Copy)]
enum Half {
    Both,
    Below,
    Above,
}

/// Эллипсоид с полуосями `hx`, `hy`, `hz` вокруг `(0, 0, center_z)`; `half` оставляет только нижнюю
/// или верхнюю половину — полушарие капсулы.
fn hit_ellipsoid(
    o: Vec3,
    d: Vec3,
    hx: f64,
    hy: f64,
    hz: f64,
    center_z: f64,
    half: Half,
) -> Option<f64> {
    let (ox, oy, oz) = (o[0] / hx, o[1] / hy, (o[2] - center_z) / hz);
    let (dx, dy, dz) = (d[0] / hx, d[1] / hy, d[2] / hz);
    nearest_positive(
        quadratic(
            dx * dx + dy * dy + dz * dz,
            2.0 * (ox * dx + oy * dy + oz * dz),
            ox * ox + oy * oy + oz * oz - 1.0,
        ),
        |t| {
            let z = oz + t * dz;
            match half {
                Half::Both => true,
                Half::Below => z <= 0.0,
                Half::Above => z >= 0.0,
            }
        },
    )
}

/// Луч от глаза камеры через точку под курсором на высоте `point[2]`: начало и единичное направление.
pub fn ray_through_point(eye: Vec3, point: Vec3) -> (Vec3, Vec3) {
    (eye, math3::normalize(math3::sub(point, eye)))
}

/// Луч от глаза камеры через точку земли высоты 0 под курсором.
pub fn ray_through_ground(eye: Vec3, point: Vec2) -> (Vec3, Vec3) {
    ray_through_point(eye, [point[0], point[1], 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(shape: Shape, size: Vec2, height: f64, degrees: f64) -> Body {
        let (sin, cos) = Rotation::from_degrees(degrees).expect("finite").sin_cos();
        Body {
            shape,
            center: [10.0, 10.0],
            size,
            height,
            base: 0.0,
            sin,
            cos,
        }
    }

    fn from_above(x: f64, y: f64) -> (Vec3, Vec3) {
        ([x, y, 20.0], [0.0, 0.0, -1.0])
    }

    /// «Сетки фигур»: все точки в единичном объёме и касаются всех его граней, нормали единичные.
    #[test]
    fn every_unit_mesh_fills_the_unit_volume_with_unit_normals() {
        for shape in Shape::ALL {
            let mesh = unit_mesh(shape);
            assert!(!mesh.indices.is_empty());
            assert!(
                mesh.indices
                    .iter()
                    .all(|&i| (i as usize) < mesh.vertices.len())
            );
            let mut low = [f32::INFINITY; 3];
            let mut high = [f32::NEG_INFINITY; 3];
            for v in &mesh.vertices {
                for axis in 0..3 {
                    low[axis] = low[axis].min(v.position[axis]);
                    high[axis] = high[axis].max(v.position[axis]);
                }
                let length = v.normal.iter().map(|c| c * c).sum::<f32>().sqrt();
                assert!(
                    (length - 1.0).abs() < 1e-5,
                    "{shape:?}: нормаль {:?}",
                    v.normal
                );
            }
            assert!(
                low[0] >= -0.5 - 1e-6 && high[0] <= 0.5 + 1e-6,
                "{shape:?} {low:?} {high:?}"
            );
            assert!(low[1] >= -0.5 - 1e-6 && high[1] <= 0.5 + 1e-6, "{shape:?}");
            assert!(low[2] >= -1e-6 && high[2] <= 1.0 + 1e-6, "{shape:?}");
            assert!(
                (low[0] + 0.5).abs() < 1e-5 && (high[0] - 0.5).abs() < 1e-5,
                "{shape:?} {low:?} {high:?}"
            );
            assert!(
                (low[1] + 0.5).abs() < 1e-5 && (high[1] - 0.5).abs() < 1e-5,
                "{shape:?}"
            );
            assert!(
                low[2].abs() < 1e-6 && (high[2] - 1.0).abs() < 1e-6,
                "{shape:?}"
            );
        }
    }

    /// Капсула 0,6 × 0,6 высотой 1,8 — полушария высотой 0,3; на низком и узком тоже не больше
    /// половины меньшей стороны и половины высоты.
    #[test]
    fn a_capsule_has_hemispheres_of_half_the_smaller_side_capped_by_half_the_height() {
        let hero = body(Shape::Capsule, [0.6, 0.6], 1.8, 0.0);
        assert!((hero.cap_height() - 0.3).abs() < 1e-12);
        let low = body(Shape::Capsule, [2.0, 3.0], 1.0, 0.0);
        assert!(
            (low.cap_height() - 0.5).abs() < 1e-12,
            "не больше половины height"
        );
        let mesh = unit_mesh(Shape::Capsule);
        let placed: Vec<Vec3> = mesh.vertices.iter().map(|v| hero.place(v)).collect();
        let lowest = placed.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        let highest = placed
            .iter()
            .map(|p| p[2])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(lowest.abs() < 1e-6 && (highest - 1.8).abs() < 1e-6);
        // Кольцо шва нижнего полушария лежит на высоте 0,3, верхнего — на 1,5.
        let rims: Vec<f64> = placed
            .iter()
            .filter(|p| ((p[0] - 10.0).powi(2) + (p[1] - 10.0).powi(2)).sqrt() > 0.3 - 1e-6)
            .map(|p| p[2])
            .collect();
        assert!(rims.iter().any(|z| (z - 0.3).abs() < 1e-6));
        assert!(rims.iter().any(|z| (z - 1.5).abs() < 1e-6));
    }

    #[test]
    fn a_body_reads_its_object_with_height_one_by_default() {
        let mut properties = crate::core::property::PropertyTable::new();
        properties.set_three_d(true);
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_vec2(id, property::POSITION, [2.0, 4.0]);
        world.set_vec2(id, property::SIZE, [2.0, 1.0]);
        world.set_shape(id, property::SHAPE, Shape::Cylinder);
        let plain = Body::of_object(&world, id).expect("has a shape");
        assert_eq!(plain.center, [3.0, 4.5]);
        assert_eq!(plain.height, 1.0);
        world.set_number(id, property::HEIGHT, 2.5);
        world.set_rotation(
            id,
            property::ROTATION,
            Rotation::from_degrees(90.0).unwrap(),
        );
        let tall = Body::of_object(&world, id).expect("has a shape");
        assert_eq!((tall.height, tall.sin, tall.cos), (2.5, 1.0, 0.0));
        let other = world.create();
        assert_eq!(Body::of_object(&world, other), None);
    }

    /// Попадание по форме: коробка, цилиндр, капсула и шар, в том числе повёрнутые и растянутые.
    #[test]
    fn a_ray_from_above_hits_each_shape_only_where_its_body_is() {
        // Коробка 4×1, повёрнутая на 90°: занимает 1 по x и 4 по y.
        let wall = body(Shape::Box, [4.0, 1.0], 2.0, 90.0);
        let (o, d) = from_above(10.0, 11.8);
        assert!((wall.ray_hit(o, d).expect("hit") - 18.0).abs() < 1e-9);
        let (o, d) = from_above(11.8, 10.0);
        assert_eq!(wall.ray_hit(o, d), None);

        // Цилиндр 4×2: эллипс, по углу описанного прямоугольника не попасть.
        let drum = body(Shape::Cylinder, [4.0, 2.0], 1.0, 0.0);
        let (o, d) = from_above(11.5, 10.0);
        assert!(drum.ray_hit(o, d).is_some());
        let (o, d) = from_above(11.7, 10.8);
        assert_eq!(drum.ray_hit(o, d), None);

        // Шар, вытянутый по высоте: сверху попадает на высоте 3.
        let egg = body(Shape::Sphere, [1.0, 1.0], 3.0, 0.0);
        let (o, d) = from_above(10.0, 10.0);
        assert!((egg.ray_hit(o, d).expect("hit") - 17.0).abs() < 1e-9);
        let (o, d) = from_above(10.45, 10.0);
        assert!(egg.ray_hit(o, d).is_some());
        let (o, d) = from_above(10.49, 10.49);
        assert_eq!(egg.ray_hit(o, d), None);
    }

    /// «Луч мимо капсулы рядом с её верхом — не попал»: у верха капсулы полушарие, а не шапка
    /// цилиндра; сбоку у тела — попал.
    #[test]
    fn a_ray_just_beside_the_top_of_a_capsule_misses_it() {
        let hero = body(Shape::Capsule, [0.6, 0.6], 1.8, 0.0);
        // Горизонтальный луч в 0,25 от оси на высоте 1,7 идёт над плечом полушария (его радиус
        // там 0,22), а будь верх шапкой цилиндра — попал бы.
        let direction = [1.0, 0.0, 0.0];
        assert_eq!(hero.ray_hit([5.0, 10.25, 1.7], direction), None);
        // На высоте 1,4, в теле, тот же луч попадает у самой стенки.
        let hit = hero
            .ray_hit([5.0, 10.25, 1.4], direction)
            .expect("hit the trunk");
        let expected = 10.0 - (0.3_f64.powi(2) - 0.25_f64.powi(2)).sqrt() - 5.0;
        assert!((hit - expected).abs() < 1e-9, "{hit}");
        // Сверху по оси — попал в макушку.
        let (o, d) = from_above(10.0, 10.0);
        assert!((hero.ray_hit(o, d).expect("hit") - 18.2).abs() < 1e-9);
    }

    #[test]
    fn a_ray_beginning_beyond_the_body_or_pointing_away_does_not_hit() {
        let block = body(Shape::Box, [1.0, 1.0], 1.0, 0.0);
        assert_eq!(block.ray_hit([10.0, 10.0, 5.0], [0.0, 0.0, 1.0]), None);
        let flat = body(Shape::Box, [0.0, 1.0], 1.0, 0.0);
        let (o, d) = from_above(10.0, 10.0);
        assert_eq!(flat.ray_hit(o, d), None, "нулевая ширина — нет объёма");
    }

    /// Прямоугольник на экране — по точкам сетки: у капсулы верх — над её макушкой.
    #[test]
    fn the_screen_rectangle_of_a_capsule_tops_out_above_the_top_point() {
        let hero = body(Shape::Capsule, [0.6, 0.6], 1.8, 0.0);
        let camera = Camera3d::looking_at([10.0, 10.0], 55.0, 12.0, [1920.0, 1080.0]);
        let [x0, y0, x1, y1] = hero.screen_rect(&camera).expect("in front");
        let top = camera.project([10.0, 10.0, 1.8]).expect("in front");
        assert!(y0 <= top[1] && top[1] - y0 < 25.0, "{y0} {top:?}");
        assert!(y1 > y0 && x1 > x0);
        let middle = (x0 + x1) / 2.0;
        assert!((middle - top[0]).abs() < 1.0);
    }
}

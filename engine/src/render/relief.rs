//! «Рельеф» → «Вид»: поверхность земли трёхмерной сцены в треугольниках, без видеокарты. Сетка
//! рельефа и воды строится один раз при загрузке (`TerrainMesh`); плитки слоёв `ground` и плоские
//! объекты каждый кадр ложатся на неё треугольниками (`surface_triangles`), так что склон, настил и
//! ровная земля рисуются одним путём.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::math3::{self, Vec3};
use crate::core::surface::{self, Lies};
use crate::core::terrain::Terrain;
use crate::core::value::Vec2;
use crate::core::world::World;

use super::atlas::RectPaint;

static NEXT_MESH: AtomicU64 = AtomicU64::new(1);

const UP: Vec3 = [0.0, 0.0, 1.0];

/// Вершина поверхности рельефа или воды: место, нормаль треугольника и цвет.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

/// Сетка рельефа: треугольники земли, залитые фоном сцены, затем, если в файле есть вода, её гладь
/// только над землёй ниже уровня воды: под землёй воды нет, иначе она просвечивает под краем
/// сцены, у которой нет боковой стенки. Плоские треугольники — вершины у каждого свои, с нормалью треугольника.
#[derive(Debug, Clone)]
pub struct TerrainMesh {
    /// Один на каждую построенную сетку: видеокарта перезаписывает буфер, только когда номер сменился.
    pub id: u64,
    pub vertices: Vec<TerrainVertex>,
    /// Первые `land` вершин — земля, остальные — вода.
    pub land: usize,
}

fn to_f32(point: Vec3) -> [f32; 3] {
    [point[0] as f32, point[1] as f32, point[2] as f32]
}

impl TerrainMesh {
    /// Сетка рельефа, залитого `background`; `None`, пока в сцене нет файла высот.
    pub fn build(terrain: &Terrain, background: [f32; 4]) -> Option<TerrainMesh> {
        let [columns, rows] = terrain.squares();
        if columns == 0 {
            return None;
        }
        let color = [background[0], background[1], background[2]];
        let mut vertices = Vec::with_capacity(columns * rows * 6 + 6);
        for row in 0..rows {
            for column in 0..columns {
                for which in 0..2 {
                    let triangle = terrain.triangle(column, row, which);
                    let normal = to_f32(Terrain::normal(&triangle));
                    vertices.extend(triangle.map(|corner| TerrainVertex {
                        position: to_f32(corner),
                        normal,
                        color,
                    }));
                }
            }
        }
        let land = vertices.len();
        if let Some(water) = terrain.water() {
            let corner = |point: Vec2| TerrainVertex {
                position: to_f32([point[0], point[1], water.level]),
                normal: to_f32(UP),
                color: [water.color[0], water.color[1], water.color[2]],
            };
            let scene = [columns as f64 / 2.0, rows as f64 / 2.0];
            for piece in terrain.pieces_below(water.level, [[0.0, 0.0], scene]) {
                for at in 1..piece.len() - 1 {
                    vertices.extend([corner(piece[0]), corner(piece[at]), corner(piece[at + 1])]);
                }
            }
        }
        Some(TerrainMesh {
            id: NEXT_MESH.fetch_add(1, Ordering::Relaxed),
            vertices,
            land,
        })
    }

    pub fn has_water(&self) -> bool {
        self.vertices.len() > self.land
    }
}

/// Вершина треугольника плитки или плоского объекта, лежащего на поверхности.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Точка в листе атласа, в пикселях.
    pub uv: [f32; 2],
    /// Заливка с уже умноженным на непрозрачность цветом.
    pub color: [f32; 4],
    /// Пределы выборки в листе: половина пикселя внутрь от края картинки.
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub layer: f32,
    pub smooth: f32,
}

/// Общее у всех вершин одного прямоугольника.
struct Paint<'a> {
    rect: &'a RectPaint,
    uv_min: [f32; 2],
    uv_max: [f32; 2],
}

impl<'a> Paint<'a> {
    fn new(rect: &'a RectPaint) -> Paint<'a> {
        let (x, y) = (rect.atlas_rect.x as f32, rect.atlas_rect.y as f32);
        let (w, h) = (rect.atlas_rect.w as f32, rect.atlas_rect.h as f32);
        let uv_min = [x + 0.5, y + 0.5];
        Paint {
            rect,
            uv_min,
            uv_max: [(x + w - 0.5).max(uv_min[0]), (y + h - 0.5).max(uv_min[1])],
        }
    }

    /// `unit` — место вершины в прямоугольнике от 0 до 1 по каждой оси.
    fn vertex(&self, position: Vec3, normal: Vec3, unit: Vec2) -> SurfaceVertex {
        let rect = self.rect;
        let unit_x = if rect.flip_x { 1.0 - unit[0] } else { unit[0] };
        let [r, g, b, a] = rect.color;
        SurfaceVertex {
            position: to_f32(position),
            normal: to_f32(normal),
            uv: [
                rect.atlas_rect.x as f32 + unit_x as f32 * rect.atlas_rect.w as f32,
                rect.atlas_rect.y as f32 + unit[1] as f32 * rect.atlas_rect.h as f32,
            ],
            color: [r * a, g * a, b * a, a],
            uv_min: self.uv_min,
            uv_max: self.uv_max,
            layer: rect.atlas_rect.sheet as f32,
            smooth: f32::from(u8::from(rect.smooth)),
        }
    }

    /// Место на земле, где стоит вершина `unit` прямоугольника после его поворота.
    fn ground_point(&self, unit: Vec2) -> Vec2 {
        let rect = self.rect;
        let corner = [
            f64::from(rect.position[0]) + unit[0] * f64::from(rect.size[0]),
            f64::from(rect.position[1]) + unit[1] * f64::from(rect.size[1]),
        ];
        let pivot = [f64::from(rect.turn.pivot[0]), f64::from(rect.turn.pivot[1])];
        let (sin, cos) = (f64::from(rect.turn.sin), f64::from(rect.turn.cos));
        let offset = [corner[0] - pivot[0], corner[1] - pivot[1]];
        [
            pivot[0] + cos * offset[0] - sin * offset[1],
            pivot[1] + sin * offset[0] + cos * offset[1],
        ]
    }

    /// Обратное к `ground_point`: где в прямоугольнике лежит место на земле.
    fn unit_of(&self, point: Vec2) -> Vec2 {
        let rect = self.rect;
        let pivot = [f64::from(rect.turn.pivot[0]), f64::from(rect.turn.pivot[1])];
        let (sin, cos) = (f64::from(rect.turn.sin), f64::from(rect.turn.cos));
        let offset = [point[0] - pivot[0], point[1] - pivot[1]];
        let corner = [
            pivot[0] + cos * offset[0] + sin * offset[1],
            pivot[1] - sin * offset[0] + cos * offset[1],
        ];
        [
            (corner[0] - f64::from(rect.position[0])) / f64::from(rect.size[0]),
            (corner[1] - f64::from(rect.position[1])) / f64::from(rect.size[1]),
        ]
    }

    fn corners(&self) -> [Vec2; 4] {
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|unit| self.ground_point(unit))
    }

    /// Прямоугольник целиком на высоте `z`.
    fn push_level_quad(&self, z: f64, out: &mut Vec<SurfaceVertex>) {
        let units = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for index in [0, 1, 2, 0, 2, 3] {
            let point = self.ground_point(units[index]);
            out.push(self.vertex([point[0], point[1], z], UP, units[index]));
        }
    }
}

/// Часть выпуклого многоугольника `subject`, что лежит внутри треугольника `clip`.
fn clip_to_triangle(subject: &[Vec2], clip: &[Vec2; 3]) -> Vec<Vec2> {
    let span = |a: Vec2, b: Vec2| [b[0] - a[0], b[1] - a[1]];
    let (e1, e2) = (span(clip[0], clip[1]), span(clip[0], clip[2]));
    let orientation = (e1[0] * e2[1] - e1[1] * e2[0]).signum();
    let mut polygon = subject.to_vec();
    for index in 0..3 {
        let (a, b) = (clip[index], clip[(index + 1) % 3]);
        let side =
            |p: Vec2| orientation * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));
        let input = std::mem::take(&mut polygon);
        for (at, &current) in input.iter().enumerate() {
            let previous = input[(at + input.len() - 1) % input.len()];
            let (before, now) = (side(previous), side(current));
            if (before >= 0.0) != (now >= 0.0) {
                let t = before / (before - now);
                polygon.push([
                    previous[0] + t * (current[0] - previous[0]),
                    previous[1] + t * (current[1] - previous[1]),
                ]);
            }
            if now >= 0.0 {
                polygon.push(current);
            }
        }
        if polygon.is_empty() {
            break;
        }
    }
    polygon
}

/// Высота плоскости треугольника в точке `(x, y)`.
fn plane_height(triangle: &[Vec3; 3], point: Vec2) -> f64 {
    let normal = math3::cross(
        math3::sub(triangle[1], triangle[0]),
        math3::sub(triangle[2], triangle[0]),
    );
    triangle[0][2]
        - (normal[0] * (point[0] - triangle[0][0]) + normal[1] * (point[1] - triangle[0][1]))
            / normal[2]
}

/// Плитка слоя `ground`: клетка — восемь треугольников рельефа этой клетки, картинка растянута по
/// ним. Без файла высот клетка ровная, два треугольника.
fn push_tile(terrain: &Terrain, paint: &Paint<'_>, out: &mut Vec<SurfaceVertex>) {
    let cell = [
        f64::from(paint.rect.position[0]),
        f64::from(paint.rect.position[1]),
    ];
    let squares = terrain.squares();
    if squares[0] == 0 {
        paint.push_level_quad(0.0, out);
        return;
    }
    for dy in 0..2 {
        for dx in 0..2 {
            let column = 2 * cell[0] as usize + dx;
            let row = 2 * cell[1] as usize + dy;
            if column >= squares[0] || row >= squares[1] {
                continue;
            }
            for which in 0..2 {
                let triangle = terrain.triangle(column, row, which);
                let normal = Terrain::normal(&triangle);
                for corner in triangle {
                    out.push(paint.vertex(
                        corner,
                        normal,
                        [corner[0] - cell[0], corner[1] - cell[1]],
                    ));
                }
            }
        }
    }
}

/// Плоский объект на рельефе: его прямоугольник, обрезанный по треугольникам рельефа под ним, с
/// высотами вершин по плоскости каждого треугольника. Часть за краем сцены не рисуется.
fn push_on_terrain(terrain: &Terrain, paint: &Paint<'_>, out: &mut Vec<SurfaceVertex>) {
    let quad = paint.corners();
    let squares = terrain.squares();
    let low = quad
        .iter()
        .fold([f64::INFINITY; 2], |m, c| [m[0].min(c[0]), m[1].min(c[1])]);
    let high = quad.iter().fold([f64::NEG_INFINITY; 2], |m, c| {
        [m[0].max(c[0]), m[1].max(c[1])]
    });
    let range = |low: f64, high: f64, count: usize| {
        let first = ((low * 2.0).floor().max(0.0) as usize).min(count);
        let last = ((high * 2.0).ceil().max(0.0) as usize).min(count);
        first..last
    };
    for row in range(low[1], high[1], squares[1]) {
        for column in range(low[0], high[0], squares[0]) {
            for which in 0..2 {
                let triangle = terrain.triangle(column, row, which);
                let footprint = triangle.map(|corner| [corner[0], corner[1]]);
                let piece = clip_to_triangle(&quad, &footprint);
                if piece.len() < 3 {
                    continue;
                }
                let normal = Terrain::normal(&triangle);
                let vertex = |point: Vec2| {
                    let position = [point[0], point[1], plane_height(&triangle, point)];
                    paint.vertex(position, normal, paint.unit_of(point))
                };
                for at in 1..piece.len() - 1 {
                    out.extend([vertex(piece[0]), vertex(piece[at]), vertex(piece[at + 1])]);
                }
            }
        }
    }
}

fn push_flat_object(world: &World, id: u32, paint: &Paint<'_>, out: &mut Vec<SurfaceVertex>) {
    if paint.rect.size[0] <= 0.0 || paint.rect.size[1] <= 0.0 {
        return;
    }
    let terrain = world.terrain();
    match surface::lies_on(world, id) {
        Some(Lies::Deck { top }) => paint.push_level_quad(top, out),
        Some(Lies::Terrain { low, .. }) if terrain.is_flat() => paint.push_level_quad(low, out),
        Some(Lies::Terrain { .. }) => push_on_terrain(terrain, paint, out),
        None => paint.push_level_quad(world.base_z(id), out),
    }
}

/// Треугольники кадра: плитки земли и плоские объекты `rects` в их порядке, каждый лежит на
/// рельефе или на верху настила.
pub fn surface_triangles(world: &World, rects: &[RectPaint]) -> Vec<SurfaceVertex> {
    let terrain = world.terrain();
    let mut out = Vec::new();
    for rect in rects {
        let paint = Paint::new(rect);
        match rect.object {
            None => push_tile(terrain, &paint, &mut out),
            Some(id) => push_flat_object(world, id, &paint, &mut out),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(low: f64, high: f64) -> Vec<Vec2> {
        vec![[low, low], [high, low], [high, high], [low, high]]
    }

    fn area(polygon: &[Vec2]) -> f64 {
        (0..polygon.len())
            .map(|i| {
                let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn a_square_clipped_by_a_triangle_keeps_the_part_inside_it() {
        let triangle = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let inside = clip_to_triangle(&square(-1.0, 2.0), &triangle);
        assert!((area(&inside) - 0.5).abs() < 1e-12);
        let half = clip_to_triangle(&square(0.5, 2.0), &triangle);
        assert!((area(&half) - 0.125).abs() < 1e-12, "{half:?}");
        assert!(clip_to_triangle(&square(3.0, 4.0), &triangle).is_empty());
    }

    #[test]
    fn the_plane_height_reproduces_the_corners_and_interpolates_between_them() {
        let triangle = [[0.0, 0.0, 1.0], [1.0, 0.0, 3.0], [1.0, 1.0, 5.0]];
        for corner in triangle {
            assert!((plane_height(&triangle, [corner[0], corner[1]]) - corner[2]).abs() < 1e-12);
        }
        assert!((plane_height(&triangle, [1.0, 0.5]) - 4.0).abs() < 1e-12);
    }
}

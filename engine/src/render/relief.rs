//! «Рельеф» → «Вид»: поверхность земли трёхмерной сцены в треугольниках, без видеокарты. Сетка
//! рельефа и воды строится один раз при загрузке (`TerrainMesh`); плитки слоёв `ground` и плоские
//! объекты каждый кадр ложатся на неё треугольниками (`surface_triangles`), так что склон, настил и
//! ровная земля рисуются одним путём.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::footprint::Footprint;
use crate::core::math3::{self, Vec3};
use crate::core::scene::ground_footprint;
use crate::core::surface::{self, Lies};
use crate::core::terrain::Terrain;
use crate::core::value::Vec2;
use crate::core::world::World;

use super::atlas::{RectPaint, Turn};

static NEXT_MESH: AtomicU64 = AtomicU64::new(1);

const UP: Vec3 = [0.0, 0.0, 1.0];

/// Вершина поверхности рельефа или воды: место, нормаль освещения и цвет.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

/// Сетка рельефа: треугольники земли, залитые фоном сцены, затем, если в файле есть вода, её гладь
/// только над землёй ниже уровня воды: под землёй воды нет, иначе она просвечивает под краем
/// сцены, у которой нет боковой стенки. Вершины у каждого треугольника свои; у земли нормаль — своей
/// точки сетки, одна у всех треугольников в этой точке, так что свет по склону идёт плавно.
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
                    let normals = terrain.triangle_normals(column, row, which);
                    vertices.extend(triangle.into_iter().zip(normals).map(|(corner, normal)| {
                        TerrainVertex {
                            position: to_f32(corner),
                            normal: to_f32(normal),
                            color,
                        }
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
            let scene = terrain.point_place(columns, rows);
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
    /// 1 — картинка светится: свет не освещается солнцем и не затеняется, а прибавляется как есть.
    pub glow: f32,
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
            glow: f32::from(u8::from(rect.glow)),
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

    /// Прямоугольник целиком на высоте `z`: вершины его треугольников отдаются `put` — место, нормаль
    /// (её даёт `normal_at` по месту на земле), место в прямоугольнике.
    fn level_quad(
        &self,
        z: f64,
        normal_at: impl Fn(Vec2) -> Vec3,
        mut put: impl FnMut(Vec3, Vec3, Vec2),
    ) {
        let units = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for index in [0, 1, 2, 0, 2, 3] {
            let point = self.ground_point(units[index]);
            put([point[0], point[1], z], normal_at(point), units[index]);
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

/// Плитка слоя `ground`: клетка — треугольники рельефа этой клетки, по два на квадрат сетки,
/// картинка растянута по ним. Без файла высот клетка ровная, два треугольника.
fn push_tile(terrain: &Terrain, paint: &Paint<'_>, out: &mut Vec<SurfaceVertex>) {
    let cell = [
        f64::from(paint.rect.position[0]),
        f64::from(paint.rect.position[1]),
    ];
    let squares = terrain.squares();
    if squares[0] == 0 {
        paint.level_quad(
            0.0,
            |_| UP,
            |position, normal, unit| out.push(paint.vertex(position, normal, unit)),
        );
        return;
    }
    let density = terrain.density();
    for dy in 0..density {
        for dx in 0..density {
            let column = density * cell[0] as usize + dx;
            let row = density * cell[1] as usize + dy;
            if column >= squares[0] || row >= squares[1] {
                continue;
            }
            for which in 0..2 {
                let triangle = terrain.triangle(column, row, which);
                let normals = terrain.triangle_normals(column, row, which);
                for (corner, normal) in triangle.into_iter().zip(normals) {
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

/// Квадраты сетки по столбцам и по строкам, которые задевает четырёхугольник `quad`.
fn touched_squares(terrain: &Terrain, quad: &[Vec2; 4]) -> [Range<usize>; 2] {
    let squares = terrain.squares();
    let low = quad
        .iter()
        .fold([f64::INFINITY; 2], |m, c| [m[0].min(c[0]), m[1].min(c[1])]);
    let high = quad.iter().fold([f64::NEG_INFINITY; 2], |m, c| {
        [m[0].max(c[0]), m[1].max(c[1])]
    });
    [
        terrain.square_span(low[0], high[0], squares[0]),
        terrain.square_span(low[1], high[1], squares[1]),
    ]
}

/// Плоский объект на рельефе: его прямоугольник, обрезанный по треугольникам рельефа под ним, с
/// высотами вершин по плоскости каждого треугольника и нормалями освещения земли в этих местах. Часть
/// за краем сцены не рисуется.
fn on_terrain(terrain: &Terrain, paint: &Paint<'_>, mut put: impl FnMut(Vec3, Vec3, Vec2)) {
    let quad = paint.corners();
    let [columns, rows] = touched_squares(terrain, &quad);
    for row in rows {
        for column in columns.clone() {
            for which in 0..2 {
                let triangle = terrain.triangle(column, row, which);
                let footprint = triangle.map(|corner| [corner[0], corner[1]]);
                let piece = clip_to_triangle(&quad, &footprint);
                if piece.len() < 3 {
                    continue;
                }
                let mut vertex = |point: Vec2| {
                    let position = [point[0], point[1], plane_height(&triangle, point)];
                    let normal = terrain.normal_at(point[0], point[1]);
                    put(position, normal, paint.unit_of(point));
                };
                for at in 1..piece.len() - 1 {
                    for corner in [piece[0], piece[at], piece[at + 1]] {
                        vertex(corner);
                    }
                }
            }
        }
    }
}

/// Перепад земли под объектом не больше этого — земля под ним ровная.
const LEVEL_TOLERANCE: f64 = 1e-9;

/// Прямоугольник целиком на сцене и нормаль освещения земли одна во всех точках сетки под ним и в его
/// углах. Торчащий за край сцены объект режется, как на склоне: часть за краем не рисуется.
fn lit_evenly(terrain: &Terrain, paint: &Paint<'_>) -> bool {
    let quad = paint.corners();
    let squares = terrain.squares();
    let density = terrain.density().max(1) as f64;
    let extent = [squares[0] as f64 / density, squares[1] as f64 / density];
    // Без файла рельефа сетки нет: земля ровная везде, резать нечем.
    let on_scene = squares[0] == 0
        || quad.iter().all(|corner| {
            (0.0..=extent[0]).contains(&corner[0]) && (0.0..=extent[1]).contains(&corner[1])
        });
    if !on_scene {
        return false;
    }
    let reference = terrain.normal_at(quad[0][0], quad[0][1]);
    let [columns, rows] = touched_squares(terrain, &quad);
    quad.iter()
        .all(|corner| terrain.normal_at(corner[0], corner[1]) == reference)
        && rows
            .flat_map(|row| columns.clone().map(move |column| (column, row)))
            .flat_map(|(column, row)| (0..2).map(move |which| (column, row, which)))
            .all(|(column, row, which)| {
                terrain
                    .triangle_normals(column, row, which)
                    .iter()
                    .all(|normal| *normal == reference)
            })
}

/// Плоский объект над локально ровной землёй на высоте `low`: одним прямоугольником, если свет земли под
/// ним везде один, иначе кусками по треугольникам рельефа, как на склоне.
fn on_level_ground(
    terrain: &Terrain,
    paint: &Paint<'_>,
    low: f64,
    mut put: impl FnMut(Vec3, Vec3, Vec2),
) {
    if lit_evenly(terrain, paint) {
        paint.level_quad(low, |point| terrain.normal_at(point[0], point[1]), put);
    } else {
        on_terrain(terrain, paint, &mut put);
    }
}

/// Вершина готовой формы плоского объекта: место, нормаль и положение в прямоугольнике от 0 до 1.
#[derive(Debug, Clone, Copy)]
struct Corner {
    position: Vec3,
    normal: Vec3,
    unit: Vec2,
}

/// Всё, от чего зависит форма плоского объекта: его прямоугольник и поворот в кадре, место в мире,
/// высота основания и состояние земли с настилами.
#[derive(Debug, Clone, PartialEq)]
struct ShapeKey {
    position: [f32; 2],
    size: [f32; 2],
    turn: Turn,
    place: Option<Footprint>,
    base_z: f64,
    revision: u64,
}

impl ShapeKey {
    fn of(world: &World, id: u32, rect: &RectPaint) -> ShapeKey {
        ShapeKey {
            position: rect.position,
            size: rect.size,
            turn: rect.turn,
            place: ground_footprint(world, id),
            base_z: world.base_z(id),
            revision: world.surface_revision(),
        }
    }
}

#[derive(Debug, Default)]
struct Slot {
    key: Option<ShapeKey>,
    corners: Vec<Corner>,
}

/// Готовые формы плоских объектов на земле, по номеру объекта. Форма считается заново, только когда
/// сменилось то, от чего она зависит (`ShapeKey`); цвет, картинка, кадр, `opacity` и `layer` в нём не
/// участвуют.
#[derive(Debug, Default)]
pub struct FlatShapes {
    slots: Vec<Slot>,
    builds: usize,
}

impl FlatShapes {
    /// Сколько форм посчитано с самого начала.
    pub fn builds(&self) -> usize {
        self.builds
    }

    /// Готовая форма объекта `id` для прямоугольника `paint`; считается, если ключ сменился.
    fn shape(&mut self, world: &World, id: u32, paint: &Paint<'_>) -> &[Corner] {
        let key = ShapeKey::of(world, id, paint.rect);
        let index = id as usize;
        if self.slots.len() <= index {
            self.slots.resize_with(index + 1, Slot::default);
        }
        let slot = &mut self.slots[index];
        if slot.key.as_ref() != Some(&key) {
            slot.corners.clear();
            build_flat_shape(world, id, paint, &mut slot.corners);
            slot.key = Some(key);
            self.builds += 1;
        }
        &slot.corners
    }
}

/// Форма плоского объекта: прямоугольник на высоте верха настила под ним или ровной земли, иначе куски по
/// треугольникам рельефа.
fn build_flat_shape(world: &World, id: u32, paint: &Paint<'_>, corners: &mut Vec<Corner>) {
    let mut put = |position, normal, unit| {
        corners.push(Corner {
            position,
            normal,
            unit,
        });
    };
    match surface::lies_on(world, id) {
        Some(Lies::Deck { top }) => paint.level_quad(top, |_| UP, put),
        Some(Lies::Terrain { low, high }) if high - low <= LEVEL_TOLERANCE => {
            on_level_ground(world.terrain(), paint, low, put);
        }
        Some(Lies::Terrain { .. }) => on_terrain(world.terrain(), paint, &mut put),
        None => paint.level_quad(world.base_z(id), |_| UP, put),
    }
}

fn push_flat_object(
    world: &World,
    id: u32,
    paint: &Paint<'_>,
    shapes: &mut FlatShapes,
    out: &mut Vec<SurfaceVertex>,
) {
    if paint.rect.size[0] <= 0.0 || paint.rect.size[1] <= 0.0 {
        return;
    }
    out.extend(
        shapes
            .shape(world, id, paint)
            .iter()
            .map(|corner| paint.vertex(corner.position, corner.normal, corner.unit)),
    );
}

/// Треугольники кадра: плитки земли и плоские объекты `rects` в их порядке, каждый лежит на
/// рельефе или на верху настила. Форму плоского объекта берёт из `shapes` и считает заново, только
/// если она устарела.
pub fn surface_triangles(
    world: &World,
    rects: &[RectPaint],
    shapes: &mut FlatShapes,
) -> Vec<SurfaceVertex> {
    let terrain = world.terrain();
    let mut out = Vec::new();
    for rect in rects {
        let paint = Paint::new(rect);
        match rect.object {
            None => push_tile(terrain, &paint, &mut out),
            Some(id) => push_flat_object(world, id, &paint, shapes, &mut out),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::atlas::AtlasRect;

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

    type Piece = (Vec3, Vec3, Vec2);

    /// Плато высотой 2 в клетках 8–12 по x и 4–8 по y, склон до клетки 13,5, вокруг земля на нуле.
    fn hill() -> Terrain {
        let height = |x: f64, y: f64| {
            let d = (x - 10.0).abs().max((y - 6.0).abs());
            if d <= 2.0 {
                2.0
            } else if d <= 3.5 {
                2.0 * (3.5 - d) / 1.5
            } else {
                0.0
            }
        };
        let rows = (0..=24)
            .map(|row| {
                (0..=40)
                    .map(|column| height(f64::from(column) / 2.0, f64::from(row) / 2.0))
                    .collect()
            })
            .collect::<Vec<Vec<f64>>>();
        Terrain::from_rows([20, 12], &rows, None).expect("рельеф")
    }

    fn rect(position: [f32; 2], size: [f32; 2]) -> RectPaint {
        let atlas = AtlasRect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            sheet: 0,
        };
        RectPaint::flat(position, size, [1.0; 4], atlas, false, false)
    }

    fn collect(fill: impl FnOnce(&mut dyn FnMut(Vec3, Vec3, Vec2))) -> Vec<Piece> {
        let mut pieces = Vec::new();
        fill(&mut |position, normal, unit| pieces.push((position, normal, unit)));
        pieces
    }

    #[test]
    fn an_object_over_several_cells_at_a_plateau_edge_is_cut_like_on_a_slope() {
        let terrain = hill();
        let rect = rect([10.5, 5.0], [3.0, 1.0]);
        let paint = Paint::new(&rect);
        let cut = collect(|put| on_level_ground(&terrain, &paint, 2.0, put));
        let before = collect(|put| on_terrain(&terrain, &paint, put));
        let single = collect(|put| {
            paint.level_quad(2.0, |point| terrain.normal_at(point[0], point[1]), put);
        });
        assert!(before.len() > 6);
        assert_eq!(cut, before);
        assert_ne!(cut, single);
    }

    #[test]
    fn an_object_in_the_middle_of_a_plain_is_one_rectangle() {
        let terrain = hill();
        let rect = rect([1.0, 1.0], [3.0, 2.0]);
        let paint = Paint::new(&rect);
        let quad = collect(|put| on_level_ground(&terrain, &paint, 0.0, put));
        assert_eq!(quad.len(), 6);
        assert!(
            quad.iter()
                .all(|(position, normal, _)| position[2] == 0.0 && *normal == UP)
        );
    }

    #[test]
    fn an_object_sticking_out_of_the_scene_is_cut_at_its_edge() {
        let terrain = hill();
        let rect = rect([-1.0, 1.0], [3.0, 2.0]);
        let paint = Paint::new(&rect);
        let cut = collect(|put| on_level_ground(&terrain, &paint, 0.0, put));
        assert_eq!(cut, collect(|put| on_terrain(&terrain, &paint, put)));
        assert!(
            cut.iter().all(|(position, _, _)| position[0] >= 0.0),
            "часть за краем сцены не рисуется"
        );
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

//! «Трёхмерная сцена» → «Вид дороги»: дорога выглядит дорогой — прямые куски своего вида вдоль оси и
//! перекрёстки готовыми моделями на стыках, изломах и в тупиках. Модуль не знает о `World` и видеокарте: дороги
//! ему приносят готовыми прямоугольниками (`RoadShape`) с видом, оси и связи он берёт у `roads`, куски кладёт по
//! осям, а модель перекрёстка выбирает и изгибает `junction`.

pub mod junction;

#[cfg(test)]
mod fixtures;

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::rc::Rc;

use junction::SpotArm;

use super::model::{ModelTable, Part};
use super::roads::{Link, Road, RoadShape, add_scaled, cross, distance, dot, find_links, sub};
use super::value::Vec2;

/// Излом двух дорог одного вида и ширины, с которого на их стыке стоит перекрёсток, в градусах: меньше —
/// раскладка каждой продолжена за точку связи.
pub const JUNCTION_MIN_TURN_DEGREES: f64 = 30.0;
/// Точки связей разных дорог ближе этого друг к другу — одна точка, клеток.
pub const MERGE_DISTANCE: f64 = 0.05;
/// Два соседних рукава перекрёстка ближе этого друг к другу, в градусах, — модели у перекрёстка нет: такие дороги почти
/// лежат друг на друге.
const MIN_ARMS_APART_DEGREES: f64 = 30.0;
/// Оси двух дорог сходятся под меньшим углом, в градусах, — точка их стыка берётся у самой связи, а не на
/// пересечении осей: у почти параллельных осей пересечение уезжает от места, где дороги касаются, на поперечный
/// сдвиг конца, делённый на синус угла, — 0,002 клетки округления `scene.json` при 0,8° дают 0,14 клетки.
const NEAR_PARALLEL_DEGREES: f64 = 5.0;

const EPS: f64 = 1e-9;

/// Модель прямого куска: номер в таблице моделей игры и её размеры в файле — ширина по X и длина по Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PieceModel {
    pub model: usize,
    pub width: f64,
    pub length: f64,
}

/// Вид дороги `scene.road_kinds`: сменные варианты прямого куска.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadKind {
    pub name: String,
    pub pieces: Vec<PieceModel>,
}

impl RoadKind {
    /// Ширина вида в единицах файла — ширина первого прямого куска по X.
    pub fn width(&self) -> f64 {
        self.pieces[0].width
    }
}

/// Рукав модели перекрёстка `scene.road_junctions`: направление в радианах от +X glTF к −Z glTF против часовой,
/// если смотреть сверху (в пределах от 0 до полного круга), номер вида, расстояние от середины до устья, длина
/// гаснущей полосы перед устьем и ширина рукава — ширина его вида — в единицах файла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JunctionArm {
    pub dir: f64,
    pub kind: usize,
    pub reach: f64,
    pub fade: f64,
    pub width: f64,
}

/// Модель перекрёстка: номер в таблице моделей игры и её рукава в порядке записи.
#[derive(Debug, Clone, PartialEq)]
pub struct JunctionModel {
    pub model: usize,
    pub arms: Vec<JunctionArm>,
}

/// Виды дорог игры; у вида, чьё имя в `road_kind` дороги, номер в этой таблице. К ним же — модели перекрёстков
/// `scene.road_junctions` в порядке записи.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadKindTable {
    kinds: Vec<RoadKind>,
    junctions: Vec<JunctionModel>,
}

impl RoadKindTable {
    pub fn new(kinds: Vec<RoadKind>, junctions: Vec<JunctionModel>) -> RoadKindTable {
        RoadKindTable { kinds, junctions }
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&RoadKind> {
        self.kinds.get(index)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.kinds.iter().position(|kind| kind.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.kinds.iter().map(|kind| kind.name.as_str())
    }

    pub fn junctions(&self) -> &[JunctionModel] {
        &self.junctions
    }
}

/// Дорога раскладки: прямоугольник и номер вида в `RoadKindTable`, если у дороги есть `road_kind`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRoad {
    pub shape: RoadShape,
    pub kind: Option<usize>,
}

/// Прямой кусок на месте: модель, середина, высота основания, косинус и синус поворота (локальная ось +Y
/// модели — вдоль оси дороги, от начала к концу), увеличение модели и растяжение вдоль.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedPiece {
    pub model: usize,
    pub center: Vec2,
    pub z: f64,
    pub cos: f64,
    pub sin: f64,
    pub scale: f64,
    pub stretch: f64,
    /// Длина куска на земле после растяжения.
    pub length: f64,
}

/// Рукав перекрёстка на месте: угол его оси на карте (в радианах от +X против часовой, если смотреть сверху), его
/// удлинение и расстояние от середины до конца выреза раскладки — `(reach − fade) × s + удлинение`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedArm {
    pub angle: f64,
    pub extension: f64,
    pub mouth: f64,
}

/// Перекрёсток на месте: модель из `scene.road_junctions`, середина, высота основания и изогнутая копия её
/// частей — вершины лежат в клетках мира относительно середины, у каждой части тот же материал, что в модели.
#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    /// Номер модели в таблице моделей игры.
    pub model: usize,
    pub center: Vec2,
    /// Ширина самой широкой дороги перекрёстка.
    pub width: f64,
    pub z: f64,
    /// Во сколько раз модель увеличена: наибольшее из отношений ширины дороги к ширине её рукава в модели.
    pub scale: f64,
    pub mirrored: bool,
    /// Рукава по возрастанию угла.
    pub arms: Vec<PlacedArm>,
    pub parts: Vec<Part>,
}

/// Точки на плоскости, которые ближе `MERGE_DISTANCE` друг к другу, — одна: сетка с клеткой в это
/// расстояние ищет соседа в девяти клетках вокруг точки.
#[derive(Default)]
struct PointGrid {
    cells: HashMap<(i64, i64), Vec<(usize, Vec2)>>,
}

impl PointGrid {
    fn cell(point: Vec2) -> (i64, i64) {
        (
            (point[0] / MERGE_DISTANCE).floor() as i64,
            (point[1] / MERGE_DISTANCE).floor() as i64,
        )
    }

    fn find(&self, point: Vec2) -> Option<usize> {
        let (cx, cy) = PointGrid::cell(point);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let near = self.cells.get(&(cx + dx, cy + dy));
                let found = near.and_then(|list| {
                    list.iter()
                        .find(|(_, known)| distance(*known, point) < MERGE_DISTANCE)
                });
                if let Some((index, _)) = found {
                    return Some(*index);
                }
            }
        }
        None
    }

    fn insert(&mut self, index: usize, point: Vec2) {
        self.cells
            .entry(PointGrid::cell(point))
            .or_default()
            .push((index, point));
    }
}

/// Точка стыка и дороги, чьи связи сошлись в ней.
struct Cluster {
    point: Vec2,
    roads: Vec<usize>,
}

/// Точка стыка пары дорог, к которой относится связь `first`: пересечение их осевых прямых, если оси сходятся не
/// меньше чем под `NEAR_PARALLEL_DEGREES` и пересечение не дальше ширины каждой дороги от её оси-отрезка; иначе —
/// точка самой связи (на оси несущей дороги).
fn joint_of_pair(first: &Link, shapes: &[Road]) -> Vec2 {
    let (a, b) = (&shapes[first.a], &shapes[first.b]);
    let turn = cross(a.dir, b.dir);
    if turn.abs() >= NEAR_PARALLEL_DEGREES.to_radians().sin() {
        let point = add_scaled(
            a.end(0),
            a.dir,
            cross(sub(b.end(0), a.end(0)), b.dir) / turn,
        );
        let near_axis = |road: &Road| {
            let along = dot(sub(point, road.end(0)), road.dir);
            (-along).max(along - road.length()).max(0.0) <= width_of(road)
        };
        if near_axis(a) && near_axis(b) {
            return point;
        }
    }
    first.point_b
}

/// Точки связей, слитые по `MERGE_DISTANCE`, и номер точки у каждой связи. Все связи одной пары дорог — одна
/// точка (`joint_of_pair`): две прямые дороги стыкуются один раз.
fn cluster_links(links: &[Link], shapes: &[Road]) -> (Vec<Cluster>, Vec<usize>) {
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut grid = PointGrid::default();
    let mut of_link = Vec::with_capacity(links.len());
    let mut pairs: HashMap<(usize, usize), Vec2> = HashMap::new();
    for link in links {
        let point = *pairs
            .entry((link.a.min(link.b), link.a.max(link.b)))
            .or_insert_with(|| joint_of_pair(link, shapes));
        let index = grid.find(point).unwrap_or_else(|| {
            clusters.push(Cluster {
                point,
                roads: Vec::new(),
            });
            grid.insert(clusters.len() - 1, point);
            clusters.len() - 1
        });
        for road in [link.a, link.b] {
            if !clusters[index].roads.contains(&road) {
                clusters[index].roads.push(road);
            }
        }
        of_link.push(index);
    }
    (clusters, of_link)
}

fn width_of(road: &Road) -> f64 {
    road.half_width * 2.0
}

/// Рукав перекрёстка: дорога вида, направление, в котором она уходит от точки стыка, и начало рукава — ближайшая
/// к точке стыка точка оси дороги.
#[derive(Debug, Clone, Copy)]
struct Branch {
    road: usize,
    dir: Vec2,
    origin: Vec2,
}

/// Что стоит в точке стыка.
enum Joint {
    /// Перекрёстка нет: дорог вида нет, дорога вида одна — сквозь точку или кончается в ней.
    Nothing,
    /// Перекрёсток по рукавам; `lead` — самая широкая дорога вида в точке, от неё вид перекрёстка.
    Junction { lead: usize, branches: Vec<Branch> },
    /// Две дороги одного вида и ширины сходятся концами под малым изломом: перекрёстка нет, раскладка каждой
    /// продолжена за точку. Излом в радианах.
    Bend(f64),
}

/// Самая широкая дорога среди рукавов; при равной ширине — та, что раньше в сцене.
fn widest(branches: &[Branch], shapes: &[Road]) -> usize {
    branches
        .iter()
        .map(|branch| branch.road)
        .max_by(|&a, &b| {
            width_of(&shapes[a])
                .total_cmp(&width_of(&shapes[b]))
                .then(shapes[b].id.cmp(&shapes[a].id))
        })
        .unwrap_or_default()
}

/// Рукава дорог вида в точке `index`: дорога кончается в ней, если конец её раскладки — эта точка, и даёт один
/// рукав внутрь дороги; иначе идёт сквозь точку и даёт два.
fn branches_at(
    index: usize,
    cluster: &Cluster,
    shapes: &[Road],
    ends: &[Option<[LayoutEnd; 2]>],
) -> Vec<Branch> {
    let mut branches = Vec::new();
    for &road in &cluster.roads {
        let Some(road_ends) = &ends[road] else {
            continue;
        };
        let shape = &shapes[road];
        let origin = add_scaled(
            shape.end(0),
            shape.dir,
            dot(sub(cluster.point, shape.end(0)), shape.dir),
        );
        let backward = [-shape.dir[0], -shape.dir[1]];
        let ends_here = road_ends.each_ref().map(|end| end.cluster == Some(index));
        if ends_here == [true, true] {
            continue;
        }
        if ends_here != [false, true] {
            branches.push(Branch {
                road,
                dir: shape.dir,
                origin,
            });
        }
        if ends_here != [true, false] {
            branches.push(Branch {
                road,
                dir: backward,
                origin,
            });
        }
    }
    branches
}

fn joint_at(
    index: usize,
    cluster: &Cluster,
    roads: &[LayoutRoad],
    shapes: &[Road],
    ends: &[Option<[LayoutEnd; 2]>],
) -> Joint {
    let branches = branches_at(index, cluster, shapes, ends);
    if branches.len() < 2 {
        return Joint::Nothing;
    }
    if let [first, second] = branches[..] {
        if first.road == second.road {
            return Joint::Nothing;
        }
        let turn = PI
            - libm::atan2(
                cross(first.dir, second.dir).abs(),
                dot(first.dir, second.dir),
            );
        let alike = roads[first.road].kind == roads[second.road].kind
            && (width_of(&shapes[first.road]) - width_of(&shapes[second.road])).abs() <= EPS;
        if alike && turn < JUNCTION_MIN_TURN_DEGREES.to_radians() - EPS {
            return Joint::Bend(turn);
        }
    }
    Joint::Junction {
        lead: widest(&branches, shapes),
        branches,
    }
}

/// Конец раскладки у одного конца оси: расстояние от начала оси и точка связи, что его задала; без
/// точки — тупик.
struct LayoutEnd {
    along: f64,
    cluster: Option<usize>,
}

/// Конец раскладки: ближайшая к концу оси точка связи не дальше ширины дороги от него, даже если она чуть
/// за концом оси; нет такой — тупик, половина ширины от конца оси.
fn layout_end(
    road: &Road,
    end: usize,
    own_links: &[usize],
    of_link: &[usize],
    clusters: &[Cluster],
) -> LayoutEnd {
    let width = width_of(road);
    let target = if end == 0 { 0.0 } else { road.length() };
    let mut best: Option<(f64, LayoutEnd)> = None;
    for &link in own_links {
        let cluster = of_link[link];
        let along = dot(sub(clusters[cluster].point, road.end(0)), road.dir);
        let gap = (along - target).abs();
        if gap <= width + EPS && best.as_ref().is_none_or(|(known, _)| gap < known - EPS) {
            best = Some((
                gap,
                LayoutEnd {
                    along,
                    cluster: Some(cluster),
                },
            ));
        }
    }
    best.map(|(_, found)| found).unwrap_or(LayoutEnd {
        along: if end == 0 {
            width / 2.0
        } else {
            road.length() - width / 2.0
        },
        cluster: None,
    })
}

/// Поворот модели, чья локальная ось +Y идёт вдоль `dir`: косинус и синус для `ModelInstance`.
fn rotation_along(dir: Vec2) -> (f64, f64) {
    (dir[1], -dir[0])
}

/// Прямые куски дороги `road` на отрезке раскладки `span` — расстояниях от начала оси до начала и до конца
/// отрезка: `n` кусков подряд, каждый растянут вдоль до `L / n`. `first` — номер первого куска от начала
/// раскладки дороги; возвращает, сколько кусков положено.
fn pieces_of(
    road: &Road,
    kind_pieces: &[PieceModel],
    span: [f64; 2],
    first: usize,
    out: &mut Vec<PlacedPiece>,
) -> usize {
    let length = span[1] - span[0];
    if length <= EPS {
        return 0;
    }
    let width = width_of(road);
    let lead = &kind_pieces[0];
    let unit_length = lead.length * width / lead.width;
    let count = ((length / unit_length + 0.5 + EPS).floor() as usize).max(1);
    let each = length / count as f64;
    let (cos, sin) = rotation_along(road.dir);
    for k in 0..count {
        let variant = &kind_pieces[(first + k + road.id as usize) % kind_pieces.len()];
        let scale = width / variant.width;
        let along = span[0] + (k as f64 + 0.5) * each;
        out.push(PlacedPiece {
            model: variant.model,
            center: add_scaled(road.end(0), road.dir, along),
            z: road.z,
            cos,
            sin,
            scale,
            stretch: each / (variant.length * scale),
            length: each,
        });
    }
    count
}

/// Отрезки раскладки `span`, что остались после выреза `cuts` — отрезков, занятых перекрёстками. Вырезы
/// могут перекрываться и выступать за раскладку.
fn remaining(span: [f64; 2], cuts: &mut [[f64; 2]]) -> Vec<[f64; 2]> {
    cuts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut out = Vec::new();
    let mut cursor = span[0];
    for cut in cuts.iter() {
        if cut[0].min(span[1]) - cursor > EPS {
            out.push([cursor, cut[0].min(span[1])]);
        }
        cursor = cursor.max(cut[1]);
    }
    if span[1] - cursor > EPS {
        out.push([cursor, span[1]]);
    }
    out
}

/// Вырез раскладки: дорога и отрезок её оси, занятый перекрёстком.
type Cut = (usize, [f64; 2]);

/// Место перекрёстка: точка, самая широкая дорога (по ней высота и ширина) и рукава.
struct Spot {
    point: Vec2,
    lead: usize,
    branches: Vec<Branch>,
}

/// Угол направления на карте, где +Y идёт к игроку: от +X против часовой, если смотреть сверху.
fn angle_of(dir: Vec2) -> f64 {
    libm::atan2(-dir[1], dir[0]).rem_euclid(TAU)
}

/// Перекрёсток в месте `spot` с моделью из набора и вырезы раскладки у его рукавов: для каждого рукава — дорога
/// и отрезок оси от точки стыка до конца гаснущей полосы. `None`, если модели с таким числом рукавов нет.
fn junction_at(
    spot: &Spot,
    roads: &[LayoutRoad],
    shapes: &[Road],
    kinds: &RoadKindTable,
    models: &ModelTable,
) -> Option<(Junction, Vec<Cut>)> {
    let mut arms: Vec<(Branch, SpotArm)> = spot
        .branches
        .iter()
        .map(|&branch| {
            let arm = SpotArm {
                angle: angle_of(branch.dir),
                kind: roads[branch.road].kind?,
                width: width_of(&shapes[branch.road]),
                offset: cross(sub(branch.origin, spot.point), branch.dir),
            };
            Some((branch, arm))
        })
        .collect::<Option<_>>()?;
    arms.sort_by(|a, b| a.1.angle.total_cmp(&b.1.angle));
    let spot_arms: Vec<SpotArm> = arms.iter().map(|(_, arm)| *arm).collect();
    let crowded = spot_arms.len() > 1
        && spot_arms.iter().enumerate().any(|(index, arm)| {
            let next = spot_arms[(index + 1) % spot_arms.len()];
            (next.angle - arm.angle).rem_euclid(TAU) < MIN_ARMS_APART_DEGREES.to_radians() - EPS
        });
    if crowded {
        return None;
    }
    let fit = junction::select(kinds, models, &spot_arms)?;
    let placed = junction::place(kinds, models, &fit, &spot_arms)?;
    let cuts = arms
        .iter()
        .zip(&placed.mouths)
        .map(|((branch, _), mouth)| {
            let road = &shapes[branch.road];
            let at = dot(sub(branch.origin, road.end(0)), road.dir);
            let far = at + dot(branch.dir, road.dir).signum() * mouth;
            (branch.road, [at.min(far), at.max(far)])
        })
        .collect();
    let junction = Junction {
        model: kinds.junctions()[fit.model].model,
        center: spot.point,
        width: width_of(&shapes[spot.lead]),
        z: shapes[spot.lead].z,
        scale: placed.scale,
        mirrored: fit.mirrored,
        arms: spot_arms
            .iter()
            .zip(&placed.mouths)
            .zip(&placed.extensions)
            .map(|((arm, &mouth), &extension)| PlacedArm {
                angle: arm.angle,
                extension,
                mouth,
            })
            .collect(),
        parts: placed.parts,
    };
    Some((junction, cuts))
}

/// Раскладка видов дорог по дорогам мира: куски вдоль осей дорог с видом, вырезанные у рукавов перекрёстков, и
/// сами перекрёстки — готовые модели на стыках, изломах и в тупиках. Связи берутся со всех дорог, в том числе
/// закрытых и без вида.
pub fn lay_out(
    roads: &[LayoutRoad],
    kinds: &RoadKindTable,
    models: &ModelTable,
) -> (Vec<PlacedPiece>, Vec<Junction>) {
    let (pieces, junctions, _) = lay_out_counting_bare(roads, kinds, models);
    (pieces, junctions)
}

/// `lay_out` и сколько мест стыка осталось без модели (требование 7 фазы 46: нет модели с таким числом рукавов или
/// два соседних рукава теснее 30°): куски их дорог идут до самой точки стыка.
fn lay_out_counting_bare(
    roads: &[LayoutRoad],
    kinds: &RoadKindTable,
    models: &ModelTable,
) -> (Vec<PlacedPiece>, Vec<Junction>, usize) {
    if roads.iter().all(|road| road.kind.is_none()) {
        return (Vec::new(), Vec::new(), 0);
    }
    let shapes: Vec<Road> = roads
        .iter()
        .map(|road| Road::from_shape(&road.shape))
        .collect();
    let links = find_links(&shapes);
    let (clusters, of_link) = cluster_links(&links, &shapes);
    let mut links_of: Vec<Vec<usize>> = vec![Vec::new(); roads.len()];
    for (index, link) in links.iter().enumerate() {
        links_of[link.a].push(index);
        links_of[link.b].push(index);
    }
    let ends: Vec<Option<[LayoutEnd; 2]>> = roads
        .iter()
        .enumerate()
        .map(|(index, road)| {
            let shape = &shapes[index];
            (road.kind.and_then(|kind| kinds.get(kind)).is_some() && width_of(shape) > EPS).then(
                || [0, 1].map(|end| layout_end(shape, end, &links_of[index], &of_link, &clusters)),
            )
        })
        .collect();
    let joints: Vec<Joint> = clusters
        .iter()
        .enumerate()
        .map(|(index, cluster)| joint_at(index, cluster, roads, &shapes, &ends))
        .collect();

    let mut spots: Vec<Spot> = Vec::new();
    for (cluster, joint) in clusters.iter().zip(&joints) {
        if let Joint::Junction { lead, branches } = joint {
            spots.push(Spot {
                point: cluster.point,
                lead: *lead,
                branches: branches.clone(),
            });
        }
    }
    let mut spans: Vec<Option<[f64; 2]>> = vec![None; roads.len()];
    for (index, road_ends) in ends.iter().enumerate() {
        let Some(ends) = road_ends else {
            continue;
        };
        let shape = &shapes[index];
        let reach = |end: &LayoutEnd| match end.cluster.map(|cluster| &joints[cluster]) {
            Some(Joint::Bend(turn)) => width_of(shape) / 2.0 * libm::tan(turn / 2.0),
            _ => 0.0,
        };
        spans[index] = Some([
            ends[0].along - reach(&ends[0]),
            ends[1].along + reach(&ends[1]),
        ]);
        for (end, found) in ends.iter().enumerate() {
            if found.cluster.is_some() {
                continue;
            }
            let inward = if end == 0 {
                shape.dir
            } else {
                [-shape.dir[0], -shape.dir[1]]
            };
            spots.push(Spot {
                point: shape.point_at(found.along),
                lead: index,
                branches: vec![Branch {
                    road: index,
                    dir: inward,
                    origin: shape.point_at(found.along),
                }],
            });
        }
    }

    let mut junctions = Vec::with_capacity(spots.len());
    let mut bare = 0;
    let mut cuts: Vec<Vec<[f64; 2]>> = vec![Vec::new(); roads.len()];
    for spot in &spots {
        let Some((junction, spot_cuts)) = junction_at(spot, roads, &shapes, kinds, models) else {
            bare += 1;
            continue;
        };
        for (road, cut) in spot_cuts {
            cuts[road].push(cut);
        }
        junctions.push(junction);
    }

    let mut pieces = Vec::new();
    for (index, road) in roads.iter().enumerate() {
        let (Some(span), Some(kind)) = (spans[index], road.kind.and_then(|kind| kinds.get(kind)))
        else {
            continue;
        };
        let mut placed = 0;
        for segment in remaining(span, &mut cuts[index]) {
            placed += pieces_of(&shapes[index], &kind.pieces, segment, placed, &mut pieces);
        }
    }
    (pieces, junctions, bare)
}

/// Раскладка, что держится между кадрами: пересобирается, только когда сменились дороги мира —
/// появились, исчезли, сдвинулись, сменили размер, поворот, высоту, признак `road` или вид. Скорость на
/// дороге в `LayoutRoad` не входит, поэтому её смена раскладку не трогает.
#[derive(Debug, Default)]
pub struct RoadKindLayout {
    roads: Vec<LayoutRoad>,
    pieces: Vec<PlacedPiece>,
    junctions: Rc<Vec<Junction>>,
    bare_spots: usize,
    version: u64,
    fresh: Vec<LayoutRoad>,
}

impl RoadKindLayout {
    /// Сколько раз раскладка пересобрана.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn pieces(&self) -> &[PlacedPiece] {
        &self.pieces
    }

    /// Сколько мест стыка осталось без модели перекрёстка: куски их дорог идут до самой точки стыка.
    pub fn bare_spots(&self) -> usize {
        self.bare_spots
    }

    /// Перекрёстки с изогнутыми копиями моделей; новый общий список — при каждой пересборке, так что видеокарта
    /// заливает их заново, только когда он сменился.
    pub fn junctions(&self) -> &Rc<Vec<Junction>> {
        &self.junctions
    }

    /// Приводит раскладку к дорогам, которые `fill` кладёт в переданный буфер; `true`, если пересобрана.
    /// Буфер живёт между вызовами: кадр без перемен ничего не выделяет.
    pub fn sync_with(
        &mut self,
        kinds: &RoadKindTable,
        models: &ModelTable,
        fill: impl FnOnce(&mut Vec<LayoutRoad>),
    ) -> bool {
        let mut fresh = std::mem::take(&mut self.fresh);
        fill(&mut fresh);
        let changed = fresh != self.roads;
        if changed {
            std::mem::swap(&mut self.roads, &mut fresh);
            let (pieces, junctions, bare) = lay_out_counting_bare(&self.roads, kinds, models);
            self.pieces = pieces;
            self.junctions = Rc::new(junctions);
            self.bare_spots = bare;
            self.version += 1;
        }
        self.fresh = fresh;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{self, MAIN, STREET};
    use super::*;
    use crate::core::model::Model;

    /// Виды и модели `fixtures`: кусок улицы 2 × 4 и главной 3 × 6, рукава на `REACH` = 4 с полосой `FADE` = 2 —
    /// на дороге шириной 1 (улица) или 1,5 (главная) масштаб модели 0,5, а куски кончаются в 1 от точки стыка.
    fn world() -> (RoadKindTable, ModelTable) {
        fixtures::tables()
    }

    fn road(id: u32, position: Vec2, size: Vec2, kind: Option<usize>) -> LayoutRoad {
        LayoutRoad {
            shape: RoadShape {
                id,
                position,
                size,
                sin_cos: (0.0, 1.0),
                z: 0.0,
            },
            kind,
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-5
    }

    fn near_point(a: Vec2, b: Vec2) -> bool {
        near(a[0], b[0]) && near(a[1], b[1])
    }

    fn lay(roads: &[LayoutRoad]) -> (Vec<PlacedPiece>, Vec<Junction>) {
        let (kinds, models) = world();
        lay_out(roads, &kinds, &models)
    }

    fn at(junctions: &[Junction], point: Vec2) -> Option<&Junction> {
        junctions
            .iter()
            .find(|junction| near_point(junction.center, point))
    }

    fn model_name(junction: &Junction) -> String {
        world()
            .1
            .get(junction.model)
            .map(|model| model.name.clone())
            .unwrap_or_default()
    }

    /// Дорога по концам оси, ширине и виду.
    fn segment(id: u32, from: Vec2, to: Vec2, width: f64, kind: usize) -> LayoutRoad {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length = dx.hypot(dy);
        let center = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
        let mut slanted = road(
            id,
            [center[0] - length / 2.0, center[1] - width / 2.0],
            [length, width],
            Some(kind),
        );
        slanted.shape.sin_cos = (dy / length, dx / length);
        slanted
    }

    fn width_of_kind(kind: usize) -> f64 {
        if kind == MAIN { 1.5 } else { 1.0 }
    }

    fn start_of(pieces: &[PlacedPiece], from: Vec2, dir: Vec2) -> f64 {
        pieces
            .iter()
            .map(|p| dot(sub(p.center, from), dir) - p.length / 2.0)
            .fold(f64::INFINITY, f64::min)
    }

    /// Главная улица переходит в дорогу с изломом меньше градуса, в точке перехода сбоку подходит улица — одно
    /// место стыка с тройником. Дороги заходят друг на друга на 0,02, начало второй сдвинуто поперёк на 0,002, как
    /// после округления в `scene.json`: пересечение их осей отстоит от стыка на 0,002 / sin 0,8° ≈ 0,14 — дальше
    /// `MERGE_DISTANCE`.
    #[test]
    fn a_main_road_going_on_under_a_degree_with_a_street_at_the_joint_is_one_tee() {
        let (kinds, models) = world();
        let (sin, cos) = 0.8_f64.to_radians().sin_cos();
        let start = [19.99, 10.002];
        let roads = [
            segment(0, [0.0, 10.0], [20.01, 10.0], 1.5, MAIN),
            segment(
                1,
                start,
                [start[0] + 20.0 * cos, start[1] + 20.0 * sin],
                1.5,
                MAIN,
            ),
            segment(2, [20.0, 0.0], [20.0, 10.0], 1.0, STREET),
        ];
        let mut layout = RoadKindLayout::default();
        layout.sync_with(&kinds, &models, |buffer| buffer.extend(roads));
        assert_eq!(layout.bare_spots(), 0, "место без модели");
        let (_, junctions) = lay_out(&roads, &kinds, &models);
        let at_joint: Vec<&Junction> = junctions
            .iter()
            .filter(|junction| distance(junction.center, [20.0, 10.0]) < 0.5)
            .collect();
        assert_eq!(
            at_joint.len(),
            1,
            "у стыка не один перекрёсток: {at_joint:?}"
        );
        assert_eq!(model_name(at_joint[0]), "tee_main_street");
    }

    /// Крест из двух улиц, пример 13: куски обрываются в 1 от точки стыка.
    #[test]
    fn two_streets_crossing_are_cut_back_to_the_ends_of_the_fading_strips_and_end_in_dead_ends() {
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET)),
        ];
        let (pieces, junctions) = lay(&roads);
        assert_eq!(pieces.len(), 4 + 5 + 4 + 4);

        // Устья рукавов — в (5 − 2) × 0,5 = 1,5 от точки стыка.
        // А: от 0,5 до 8,5 — длина 8, четыре куска по 2; от 11,5 до 20,5 — длина 9, пять по 1,8.
        for (k, piece) in pieces[..4].iter().enumerate() {
            assert!(near(piece.center[0], 0.5 + 2.0 * (k as f64 + 0.5)), "{k}");
        }
        for (k, piece) in pieces[4..9].iter().enumerate() {
            assert!(near(piece.length, 1.8), "{piece:?}");
            assert!(near(piece.center[0], 11.5 + 1.8 * (k as f64 + 0.5)));
        }
        // Б: от 0,5 до 8,5 и от 11,5 до 19,5 — по четыре куска по 2.
        assert!(pieces[9..].iter().all(|p| near(p.length, 2.0)));
        assert!(near(pieces[9].center[1], 0.5 + 1.0));
        assert!(near(pieces[13].center[1], 11.5 + 1.0));

        assert_eq!(junctions.len(), 5, "крест и четыре тупика");
        let cross = at(&junctions, [10.0, 10.0]).expect("крест");
        assert_eq!(model_name(cross), "cross_street_street");
        assert!(near(cross.scale, 0.5) && near(cross.width, 1.0) && !cross.mirrored);
        for end in [[0.5, 10.0], [20.5, 10.0], [10.0, 0.5], [10.0, 19.5]] {
            let dead = at(&junctions, end).expect("тупик");
            assert_eq!(model_name(dead), "end_street");
        }
    }

    /// Пример 13 про масштаб: главные крестом, куски начинаются в `(reach − fade) × s` от точки стыка.
    #[test]
    fn crossing_main_roads_scale_the_model_to_the_road_width_and_start_the_pieces_at_reach_minus_fade()
     {
        let roads = [
            segment(0, [0.0, 10.0], [20.0, 10.0], 1.5, MAIN),
            segment(1, [10.0, 0.0], [10.0, 20.0], 1.5, MAIN),
        ];
        let (pieces, junctions) = lay(&roads);
        let cross = at(&junctions, [10.0, 10.0]).expect("крест");
        assert_eq!(model_name(cross), "cross_main_main");
        assert!(near(cross.scale, 0.5), "ширина дороги 1,5, рукав модели 3");
        // Модель в таком положении не изогнута: куски каждой дороги начинаются в (5 − 2) × 0,5 = 1,5 от точки.
        for (from, dir) in [
            ([10.0, 10.0], [1.0, 0.0]),
            ([10.0, 10.0], [-1.0, 0.0]),
            ([10.0, 10.0], [0.0, 1.0]),
            ([10.0, 10.0], [0.0, -1.0]),
        ] {
            let ahead: Vec<PlacedPiece> = pieces
                .iter()
                .filter(|p| dot(sub(p.center, from), dir) > 0.0 && cross_axis(p, from, dir) < 0.1)
                .copied()
                .collect();
            assert!(near(start_of(&ahead, from, dir), 1.5), "{dir:?}");
        }
    }

    fn cross_axis(piece: &PlacedPiece, from: Vec2, dir: Vec2) -> f64 {
        (cross(sub(piece.center, from), dir)).abs()
    }

    /// Т-образный стык: главная насквозь, улица кончается на ней.
    #[test]
    fn a_street_ending_on_a_main_road_makes_a_tee_of_the_main_road_scale() {
        let roads = [
            segment(0, [30.0, 10.0], [40.0, 10.0], 1.5, MAIN),
            segment(1, [35.0, 10.0], [35.0, 20.0], 1.0, STREET),
        ];
        let (pieces, junctions) = lay(&roads);
        let joint = at(&junctions, [35.0, 10.0]).expect("тройник");
        assert_eq!(model_name(joint), "tee_main_street");
        assert!(near(joint.width, 1.5) && near(joint.scale, 0.5));
        assert!(joint.z.abs() < 1e-9 && !joint.mirrored);
        // Главная: от 30,75 до 33,5 и от 36,5 до 39,25 — по одному куску длины 2,75; улица: от y = 11,5 до тупика 19,5.
        let main: Vec<&PlacedPiece> = pieces.iter().filter(|p| p.model == 2).collect();
        assert_eq!(main.len(), 2, "{main:?}");
        assert!(near_point(main[0].center, [32.125, 10.0]) && near(main[0].length, 2.75));
        assert!(near_point(main[1].center, [37.875, 10.0]));
        let street: Vec<&PlacedPiece> = pieces.iter().filter(|p| p.model < 2).collect();
        assert!(near(
            street
                .iter()
                .map(|p| p.center[1] - p.length / 2.0)
                .fold(f64::INFINITY, f64::min),
            11.5
        ));
        assert_eq!(
            model_name(at(&junctions, [35.0, 19.5]).expect("тупик")),
            "end_street"
        );
    }

    /// Пример 15: из главной в улицу по часовой — отражённая модель.
    #[test]
    fn a_bend_from_main_into_a_street_is_mirrored_only_when_it_turns_clockwise() {
        let counter = [
            segment(0, [50.0, 10.0], [60.0, 10.0], 1.5, MAIN),
            segment(1, [60.0, 10.0], [60.0, 20.0], 1.0, STREET),
        ];
        let (_, junctions) = lay(&counter);
        let joint = at(&junctions, [60.0, 10.0]).expect("поворот");
        assert_eq!(model_name(joint), "bend_main_street");
        assert!(
            !joint.mirrored,
            "от главной налево по экрану вниз — против часовой"
        );
        let clockwise = [
            segment(0, [50.0, 10.0], [60.0, 10.0], 1.5, MAIN),
            segment(1, [60.0, 10.0], [60.0, 0.0], 1.0, STREET),
        ];
        let (_, junctions) = lay(&clockwise);
        let joint = at(&junctions, [60.0, 10.0]).expect("поворот");
        assert_eq!(model_name(joint), "bend_main_street");
        assert!(joint.mirrored, "по часовой модель отражена");
    }

    /// Две улицы шириной 1, сходящиеся концами в (60; 10): вторая отходит от первой на `degrees`.
    fn bend(degrees: f64) -> [LayoutRoad; 2] {
        let (sin, cos) = degrees.to_radians().sin_cos();
        [
            segment(0, [50.0, 10.0], [60.0, 10.0], 1.0, STREET),
            segment(
                1,
                [60.0, 10.0],
                [60.0 + cos * 10.0, 10.0 + sin * 10.0],
                1.0,
                STREET,
            ),
        ]
    }

    /// Излом меньше 30°: перекрёстка нет, раскладка каждой продолжена на `w / 2 × tg(угол / 2)`.
    #[test]
    fn a_small_bend_has_no_junction_and_both_layouts_run_past_the_joint() {
        let (pieces, junctions) = lay(&bend(20.0));
        let reach = 0.5 * 10.0_f64.to_radians().tan();
        assert!((reach - 0.088).abs() < 1e-3);
        assert_eq!(junctions.len(), 2, "только тупики на дальних концах");
        assert!(
            junctions
                .iter()
                .all(|junction| distance(junction.center, [60.0, 10.0]) > 1.0)
        );
        assert_eq!(pieces.len(), 10);
        let covered: f64 = pieces[..5].iter().map(|p| p.length).sum();
        assert!(near(covered, 9.5 + reach), "{covered}");
    }

    #[test]
    fn a_right_angle_bend_is_a_bend_model_and_cuts_both_streets_back_to_the_end_of_the_strip() {
        let (pieces, junctions) = lay(&bend(90.0));
        let joint = at(&junctions, [60.0, 10.0]).expect("поворот");
        assert_eq!(model_name(joint), "bend_street_street");
        // Первая улица: от 50,5 до 58,5; вторая: от 61,5 до 69,5.
        let covered: f64 = pieces.iter().map(|p| p.length).sum();
        assert!(near(covered, 8.0 + 8.0), "{covered}");
    }

    /// Улица переходит в главную по прямой: тот же стык, модель «переход».
    #[test]
    fn a_street_running_straight_into_a_main_road_makes_a_line_junction_of_the_main_scale() {
        let roads = [
            segment(0, [0.0, 10.0], [10.0, 10.0], 1.0, STREET),
            segment(1, [10.0, 10.0], [20.0, 10.0], 1.5, MAIN),
        ];
        let (pieces, junctions) = lay(&roads);
        let joint = at(&junctions, [10.0, 10.0]).expect("переход");
        assert_eq!(model_name(joint), "line_main_street");
        assert!(near(joint.width, 1.5) && near(joint.scale, 0.5));
        let narrow_end = pieces
            .iter()
            .filter(|p| p.model < 2)
            .map(|p| p.center[0] + p.length / 2.0)
            .fold(0.0, f64::max);
        assert!(near(narrow_end, 8.5), "{narrow_end}");
        let wide_start = pieces
            .iter()
            .filter(|p| p.model == 2)
            .map(|p| p.center[0] - p.length / 2.0)
            .fold(f64::INFINITY, f64::min);
        assert!(near(wide_start, 11.5), "{wide_start}");
    }

    #[test]
    fn two_roads_of_different_kind_or_width_get_a_junction_even_in_a_straight_line() {
        let straight = |second_kind: Option<usize>, second_width: f64| {
            [
                segment(0, [50.0, 10.0], [60.0, 10.0], 1.0, STREET),
                LayoutRoad {
                    kind: second_kind,
                    ..segment(1, [60.0, 10.0], [70.0, 10.0], second_width, STREET)
                },
            ]
        };
        let junction_at_joint = |roads: &[LayoutRoad]| {
            lay(roads)
                .1
                .into_iter()
                .find(|j| near_point(j.center, [60.0, 10.0]))
        };
        assert!(
            junction_at_joint(&straight(Some(STREET), 1.0)).is_none(),
            "одного вида и ширины — перекрёстка нет"
        );
        assert!(junction_at_joint(&straight(Some(MAIN), 1.0)).is_some());
        let wider = junction_at_joint(&straight(Some(STREET), 1.5)).expect("перекрёсток");
        assert!(near(wider.width, 1.5), "по самой широкой");
        assert!(
            junction_at_joint(&straight(None, 1.0)).is_none(),
            "дорога без вида рукавов не даёт: у дороги вида один рукав"
        );
    }

    #[test]
    fn a_road_without_a_kind_gives_no_arms_and_a_kinded_road_ending_on_it_is_neither_junction_nor_dead_end()
     {
        let (pieces, junctions) = lay(&[road(0, [0.0, 0.0], [9.0, 1.0], None)]);
        assert!(pieces.is_empty() && junctions.is_empty());
        // Улица вида кончается на улице без вида: куски доходят до точки связи, на этом конце ничего нет.
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], None),
            road(1, [9.5, 10.0], [1.0, 10.0], Some(STREET)),
        ];
        let (pieces, junctions) = lay(&roads);
        assert_eq!(junctions.len(), 1, "только тупик на дальнем конце");
        assert!(near_point(junctions[0].center, [10.0, 19.5]));
        let start = pieces
            .iter()
            .map(|p| p.center[1] - p.length / 2.0)
            .fold(f64::INFINITY, f64::min);
        assert!(near(start, 10.0), "{start}");
    }

    #[test]
    fn a_through_road_crossed_by_a_road_without_a_kind_gets_no_junction() {
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 20.0], None),
        ];
        let (pieces, junctions) = lay(&roads);
        assert_eq!(junctions.len(), 2, "тупики улицы вида");
        assert_eq!(pieces.len(), 10, "улица цела от тупика до тупика");
    }

    /// Пример 17: пять улиц в одной точке — моделей с пятью рукавами нет, куски доходят до точки стыка.
    #[test]
    fn five_streets_in_one_point_have_no_junction_and_the_pieces_run_up_to_the_point() {
        let spoke = |id: u32, degrees: f64| {
            let (sin, cos) = degrees.to_radians().sin_cos();
            segment(
                id,
                [30.0, 20.0],
                [30.0 + 8.0 * cos, 20.0 + 8.0 * sin],
                1.0,
                STREET,
            )
        };
        let roads: Vec<LayoutRoad> = (0..5).map(|k| spoke(k, 72.0 * f64::from(k))).collect();
        let (pieces, junctions) = lay(&roads);
        assert!(at(&junctions, [30.0, 20.0]).is_none());
        assert_eq!(junctions.len(), 5, "только тупики на дальних концах");
        for road in &roads {
            let dir = [road.shape.sin_cos.1, road.shape.sin_cos.0];
            let mine: Vec<PlacedPiece> = pieces
                .iter()
                .filter(|p| {
                    cross_axis(p, [30.0, 20.0], dir) < 1e-6
                        && dot(sub(p.center, [30.0, 20.0]), dir) > 0.0
                })
                .copied()
                .collect();
            assert!(near(start_of(&mine, [30.0, 20.0], dir), 0.0), "{dir:?}");
        }
    }

    #[test]
    fn a_junction_with_no_model_for_its_arm_count_cuts_nothing_but_the_others_still_do() {
        let (kinds, models) = fixtures::tables_with(&["end_street", "cross_street_street"]);
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 9.0], [1.0, 11.0], Some(STREET)),
        ];
        let (pieces, junctions) = lay_out(&roads, &kinds, &models);
        // Тройник (улица Б кончается на А) моделей не имеет: перекрёстка в стыке нет и А цела, а Б доходит до А.
        assert!(at(&junctions, [10.0, 10.0]).is_none());
        let along_a: usize = pieces.iter().filter(|p| near(p.center[1], 10.0)).count();
        assert_eq!(along_a, 10, "улица А цела от тупика до тупика");
        let start = pieces
            .iter()
            .filter(|p| near(p.center[0], 10.0))
            .map(|p| p.center[1] - p.length / 2.0)
            .fold(f64::INFINITY, f64::min);
        assert!(near(start, 10.0), "{start}");
    }

    #[test]
    fn the_junction_takes_the_scale_of_the_widest_road_and_the_first_in_the_scene_wins_a_tie() {
        let roads = [
            segment(7, [0.0, 10.0], [21.0, 10.0], 1.0, STREET),
            segment(3, [10.0, 0.0], [10.0, 20.0], 1.5, MAIN),
        ];
        let (_, junctions) = lay(&roads);
        let cross = at(&junctions, [10.0, 10.0]).expect("крест");
        assert!(near(cross.width, 1.5) && near(cross.scale, 0.5));
        assert_eq!(model_name(cross), "cross_main_street");
        // Главная насквозь по x, улица по y: подходит `cross_main_street`, повёрнутая на 90°, или на 270°.
        let wide = [
            segment(0, [0.0, 10.0], [21.0, 10.0], 3.0, STREET),
            segment(1, [10.0, 0.0], [10.0, 20.0], 3.0, STREET),
        ];
        let (_, junctions) = lay(&wide);
        let cross = at(&junctions, [10.0, 10.0]).expect("крест");
        assert!(
            near(cross.width, 3.0) && near(cross.scale, 1.5),
            "{}",
            cross.scale
        );
    }

    #[test]
    fn a_square_road_has_a_zero_length_layout_so_no_pieces_and_two_dead_ends_over_the_middle() {
        let (pieces, junctions) = lay(&[road(0, [10.0, 10.0], [1.0, 1.0], Some(STREET))]);
        assert!(pieces.is_empty());
        assert_eq!(junctions.len(), 2);
        assert!(
            junctions
                .iter()
                .all(|junction| model_name(junction) == "end_street")
        );
    }

    #[test]
    fn a_layout_shorter_than_half_a_piece_is_one_piece_squeezed_along() {
        // Ось 1,5 вдоль x при ширине 1: раскладка от 0,5 до 1,0 — длина 0,5; 0,5 / 2 даёт 0, но кусок один.
        let (pieces, _) = lay(&[road(0, [0.0, 0.0], [1.5, 1.0], Some(STREET))]);
        assert_eq!(pieces.len(), 1);
        assert!(
            near(pieces[0].length, 0.5) && near(pieces[0].stretch, 0.25),
            "{:?}",
            pieces[0]
        );
    }

    #[test]
    fn piece_variants_go_round_by_the_piece_number_plus_the_road_number() {
        let roads = [
            road(0, [0.0, 0.0], [9.0, 1.0], Some(STREET)),
            road(1, [0.0, 5.0], [9.0, 1.0], Some(STREET)),
        ];
        let (pieces, _) = lay(&roads);
        let models: Vec<usize> = pieces.iter().map(|p| p.model).collect();
        // У каждой: от 0,5 до 8,5 — длина 8, четыре куска.
        assert_eq!(models, [0, 1, 0, 1, 1, 0, 1, 0]);
    }

    #[test]
    fn the_piece_number_runs_on_through_the_segments_cut_by_a_junction() {
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET)),
        ];
        let (pieces, _) = lay(&roads);
        let models: Vec<usize> = pieces[..9].iter().map(|p| p.model).collect();
        assert_eq!(models, [0, 1, 0, 1, 0, 1, 0, 1, 0]);
    }

    #[test]
    fn two_junctions_closer_than_their_strips_leave_no_pieces_between_them() {
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET)),
            road(2, [11.0, 0.0], [1.0, 20.0], Some(STREET)),
        ];
        let (pieces, junctions) = lay(&roads);
        assert!(at(&junctions, [10.0, 10.0]).is_some() && at(&junctions, [11.5, 10.0]).is_some());
        // Вырез от 8,5 до 13: на горизонтальной улице в нём куски не лежат.
        let between = pieces.iter().filter(|p| {
            near(p.center[1], 10.0) && p.center[0] > 8.5 + 1e-9 && p.center[0] < 13.0 - 1e-9
        });
        assert_eq!(between.count(), 0);
    }

    #[test]
    fn a_dead_end_nearer_to_a_junction_than_its_strip_leaves_no_pieces_between() {
        // Улица Б пересекает А и кончается в 1,2 за её осью: тупик в (10; 10,7) ближе конца полосы креста в (10; 11).
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 8.0], [1.0, 3.2], Some(STREET)),
        ];
        let (pieces, junctions) = lay(&roads);
        assert!(at(&junctions, [10.0, 10.7]).is_some());
        assert!(
            pieces
                .iter()
                .all(|p| !near(p.center[0], 10.0) || p.center[1] < 10.0),
            "за перекрёстком куски Б не лежат"
        );
    }

    #[test]
    fn a_road_whose_layout_ends_in_the_joint_gives_one_arm_even_if_its_axis_runs_on() {
        // Улица Б идёт от (10; 0) до (10; 10,3) и пересекает А по y = 10, но её раскладка кончается в (10; 10): рукав
        // Б один, назад, и перекрёсток — тройник, а не крест.
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 10.3], Some(STREET)),
        ];
        let (pieces, junctions) = lay(&roads);
        let joint = at(&junctions, [10.0, 10.0]).expect("Т-образный стык");
        assert_eq!(model_name(joint), "tee_street_street");
        let end = pieces
            .iter()
            .filter(|p| near(p.center[0], 10.0))
            .map(|p| p.center[1] + p.length / 2.0)
            .fold(0.0, f64::max);
        assert!(near(end, 8.5), "куски Б кончаются у полосы: {end}");
        assert!(
            junctions
                .iter()
                .all(|j| !near(j.center[0], 10.0) || j.center[1] < 10.1),
            "тупика за стыком нет"
        );
    }

    /// Улица S длиной 30 (острые углы удлиняют рукава, куски должны остаться) под `degrees` начинается в (10; `start_y`) внутри прямоугольника А: стык — пересечение
    /// осей, а куски S лежат на её оси.
    fn assert_oblique_joint(degrees: f64, start_y: f64, joint: Vec2) {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let mut slanted = road(
            1,
            [10.0 + cos * 15.0 - 15.0, start_y + sin * 15.0 - 0.5],
            [30.0, 1.0],
            Some(STREET),
        );
        slanted.shape.sin_cos = (sin, cos);
        let roads = [road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)), slanted];
        let (pieces, junctions) = lay(&roads);
        assert!(at(&junctions, joint).is_some(), "{junctions:?}");
        let first = pieces
            .iter()
            .filter(|p| p.cos > 0.1)
            .min_by(|a, b| a.center[1].total_cmp(&b.center[1]))
            .expect("куски S");
        // Рукав начинается на оси S, а не в точке стыка: первый кусок лежит на её оси.
        let off_axis = cross(sub(first.center, [10.0, start_y]), [cos, sin]).abs();
        assert!(off_axis < 1e-6, "{off_axis}");

        // Устья рукавов модели ложатся на оси своих дорог в `(reach − fade) × s` от ближайшей к стыку точки оси: два
        // рукава на А, один на S, хотя ось S мимо точки стыка.
        let (kinds, models) = world();
        let crossing = at(&junctions, joint).expect("стык");
        let model = models.get(crossing.model).expect("модель");
        let described = kinds
            .junctions()
            .iter()
            .find(|j| j.model == crossing.model)
            .expect("описание модели");
        let axes = [([0.0, 10.0], [1.0, 0.0]), ([10.0, start_y], [cos, sin])];
        let mut on_roads = [0, 0];
        for arm in &described.arms {
            let index = on_axis(model, arm, arm.reach - arm.fade, 0.0).expect("вершина на оси");
            let image = crossing.parts[0].vertices[index].position;
            let point = [
                crossing.center[0] + f64::from(image[0]),
                crossing.center[1] + f64::from(image[1]),
            ];
            let road = axes
                .iter()
                .position(|&(from, dir)| {
                    let origin = add_scaled(from, dir, dot(sub(crossing.center, from), dir));
                    let offset = sub(point, origin);
                    cross(offset, dir).abs() < 1e-4
                        && crossing
                            .arms
                            .iter()
                            .any(|placed| (dot(offset, dir).abs() - placed.mouth).abs() < 1e-4)
                })
                .unwrap_or_else(|| panic!("устье в {point:?} не лежит на оси дороги"));
            on_roads[road] += 1;
        }
        assert_eq!(on_roads, [2, 1], "{on_roads:?}");
    }

    #[test]
    fn an_oblique_road_joins_at_the_crossing_of_the_axes() {
        // S под 45° начинается в (10; 10,3), не дойдя до оси А на 0,3; её ось пересекает ось А в (9,7; 10).
        assert_oblique_joint(45.0, 10.3, [9.7, 10.0]);
    }

    #[test]
    fn a_near_parallel_road_has_no_model_and_its_pieces_run_on_its_own_axis_up_to_the_joint() {
        // S под 10° начинается в (10; 10,45) на краю прямоугольника А: оси пересекаются далеко позади начала S, стык —
        // точка связи (10; 10). Рукава А и S теснее 30°, модели нет, куски S лежат на её оси и идут до стыка.
        let (sin, cos) = 10.0_f64.to_radians().sin_cos();
        let mut slanted = road(
            1,
            [10.0 + cos * 15.0 - 15.0, 10.45 + sin * 15.0 - 0.5],
            [30.0, 1.0],
            Some(STREET),
        );
        slanted.shape.sin_cos = (sin, cos);
        let roads = [road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)), slanted];
        let (pieces, junctions) = lay(&roads);
        assert!(at(&junctions, [10.0, 10.0]).is_none(), "{junctions:?}");
        let slanted_pieces: Vec<&PlacedPiece> = pieces.iter().filter(|p| p.cos > 0.1).collect();
        assert!(!slanted_pieces.is_empty());
        for piece in &slanted_pieces {
            assert!(cross(sub(piece.center, [10.0, 10.45]), [cos, sin]).abs() < 1e-6);
        }
    }

    #[test]
    fn all_links_of_a_pair_of_roads_are_one_joint_and_no_ground_lies_past_the_road_ends() {
        // Дорога 0 — ось (50, 10)–(60, 10); дорога 1 под 60° начинается в (59,7; 9,7). Связей три, стык один.
        let (sin, cos) = 60.0_f64.to_radians().sin_cos();
        let center = [59.7 + cos * 4.0, 9.7 + sin * 4.0];
        let mut second = road(
            1,
            [center[0] - 4.0, center[1] - 0.5],
            [8.0, 1.0],
            Some(STREET),
        );
        second.shape.sin_cos = (sin, cos);
        let roads = [road(0, [50.0, 9.5], [10.0, 1.0], Some(STREET)), second];
        let (_, junctions) = lay(&roads);
        let joints: Vec<&Junction> = junctions
            .iter()
            .filter(|j| distance(j.center, [59.873, 10.0]) < 0.3)
            .collect();
        assert_eq!(joints.len(), 1, "{junctions:?}");
        assert!(distance(joints[0].center, [59.7 + 0.3 / 3.0_f64.sqrt(), 10.0]) < 1e-6);
        assert_eq!(junctions.len(), 3, "стык и два тупика на дальних концах");
    }

    #[test]
    fn a_road_whose_two_layout_ends_are_in_one_point_gives_no_arms_there() {
        // Улица Б длиной 1,6 пересекает А посередине: оба конца её раскладки — в точке пересечения. Рукавов у Б нет, у А
        // остаются два: перекрёстка нет, куски Б не кладутся.
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 9.2], [1.0, 1.6], Some(STREET)),
        ];
        let (pieces, junctions) = lay(&roads);
        assert!(at(&junctions, [10.0, 10.0]).is_none(), "{junctions:?}");
        assert!(pieces.iter().all(|p| !near(p.center[0], 10.0)));
    }

    #[test]
    fn a_layout_is_rebuilt_only_when_a_road_changes() {
        let (kinds, models) = world();
        let mut layout = RoadKindLayout::default();
        let roads = vec![road(0, [0.0, 0.0], [9.0, 1.0], Some(STREET))];
        assert!(layout.sync_with(&kinds, &models, |out| out.clone_from(&roads)));
        let built = layout.version();
        let junctions = Rc::clone(layout.junctions());
        assert!(!layout.sync_with(&kinds, &models, |out| out.clone_from(&roads)));
        assert_eq!(layout.version(), built, "те же дороги — раскладка та же");
        assert!(
            Rc::ptr_eq(&junctions, layout.junctions()),
            "перекрёстки те же"
        );
        let moved = vec![road(0, [0.0, 1.0], [9.0, 1.0], Some(STREET))];
        assert!(layout.sync_with(&kinds, &models, |out| out.clone_from(&moved)));
        assert!(layout.version() > built);
        assert!(!Rc::ptr_eq(&junctions, layout.junctions()));
        assert!(near(layout.pieces()[0].center[1], 1.5));
    }

    #[test]
    fn roads_at_different_heights_do_not_join() {
        let mut upper = road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET));
        upper.shape.z = 1.2;
        let roads = [road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)), upper];
        let (_, junctions) = lay(&roads);
        assert!(
            junctions
                .iter()
                .all(|j| !near_point(j.center, [10.0, 10.0])),
            "дорога на мосту с дорогой под ним не связана"
        );
        assert!(
            junctions.iter().any(|j| near(j.z, 1.2)),
            "мост — на высоте настила"
        );
    }

    #[test]
    fn link_points_closer_than_the_merge_distance_are_one_point() {
        // Две узкие улицы с двух сторон главной кончаются на ней в двух точках в 0,03 друг от друга: перекрёсток один,
        // крест.
        let roads = [
            road(0, [0.0, 9.25], [21.0, 1.5], Some(MAIN)),
            road(1, [9.5, 5.0], [1.0, 4.9], Some(STREET)),
            road(2, [9.53, 10.1], [1.0, 5.0], Some(STREET)),
        ];
        let (_, junctions) = lay(&roads);
        let at_joint: Vec<&Junction> = junctions
            .iter()
            .filter(|j| distance(j.center, [10.0, 10.0]) < 0.2)
            .collect();
        assert_eq!(at_joint.len(), 1);
        assert_eq!(model_name(at_joint[0]), "cross_main_street");
    }

    /// Дорога, чей вид не назван в наборе, не даёт рукавов: у пустой таблицы моделей перекрёстков нет вовсе.
    #[test]
    fn a_game_without_junction_models_lays_pieces_up_to_the_joints() {
        let (kinds, models) = fixtures::tables_with(&[]);
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET)),
        ];
        let (pieces, junctions) = lay_out(&roads, &kinds, &models);
        assert!(junctions.is_empty());
        assert_eq!(
            pieces.len(),
            10 + 10,
            "обе улицы цельные от тупика до тупика"
        );
        let hollow = ModelTable::new(models.iter().map(|m| Model::placeholder(&m.name)).collect());
        let (full, _) = fixtures::tables();
        assert!(lay_out(&roads, &full, &hollow).1.is_empty());
    }

    // -----------------------------------------------------------------------------------------------
    // Перебор углов: изгиб моделей не выворачивает треугольники и кладёт рукава на оси дорог
    // -----------------------------------------------------------------------------------------------

    /// Знаковая площадь треугольника в плоскости карты.
    fn signed_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f64 {
        f64::from((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
    }

    /// Сколько треугольников копии вывернуто: лежат не так, как у модели.
    fn folds(junction: &Junction, models: &ModelTable) -> usize {
        let model = models.get(junction.model).expect("модель");
        let mut count = 0;
        for (part, source) in junction.parts.iter().zip(&model.parts) {
            assert_eq!(part.indices.len(), source.indices.len());
            let area = |vertices: &[crate::core::model::ModelVertex], t: &[u32]| {
                signed_area(
                    vertices[t[0] as usize].position,
                    vertices[t[1] as usize].position,
                    vertices[t[2] as usize].position,
                )
            };
            for (copy, original) in part.indices.chunks(3).zip(source.indices.chunks(3)) {
                if area(&source.vertices, original) * area(&part.vertices, copy) <= 0.0 {
                    count += 1;
                }
            }
        }
        count
    }

    /// Случай перебора: как сходятся две дороги (`tee` — вторая кончается на первой, `cross` — обе насквозь, `bend` —
    /// обе кончаются), угол второй от оси x по карте, виды и множители ширины дорог против ширины их вида и на сколько
    /// начало второй дороги у тройника заходит за ось первой (стык тогда в пересечении осей, а не в её конце).
    #[derive(Debug, Clone, Copy)]
    struct Case {
        shape: &'static str,
        degrees: f64,
        kinds: [usize; 2],
        widths: [f64; 2],
        slide: f64,
    }

    const MEETING: Vec2 = [30.0, 20.0];

    impl Case {
        fn roads(&self) -> Vec<LayoutRoad> {
            let p = MEETING;
            let (sin, cos) = self.degrees.to_radians().sin_cos();
            let far = |sign: f64| [p[0] + sign * 12.0 * cos, p[1] + sign * 12.0 * sin];
            let width = |k: usize| width_of_kind(self.kinds[k]) * self.widths[k];
            let first = |from: Vec2, to: Vec2| segment(0, from, to, width(0), self.kinds[0]);
            let second = |from: Vec2, to: Vec2| segment(1, from, to, width(1), self.kinds[1]);
            let (west, east) = ([p[0] - 12.0, p[1]], [p[0] + 12.0, p[1]]);
            match self.shape {
                "tee" => vec![first(west, east), second(far(-self.slide / 12.0), far(1.0))],
                "cross" => vec![first(west, east), second(far(-1.0), far(1.0))],
                _ => vec![first(west, p), second(p, far(1.0))],
            }
        }

        /// Рукава на карте: угол против часовой, если смотреть сверху, и ширина дороги. Ось y карты идёт к игроку,
        /// поэтому угол по карте — минус угол по осям.
        fn arms(&self) -> Vec<(f64, f64)> {
            let width = |k: usize| width_of_kind(self.kinds[k]) * self.widths[k];
            let second = -self.degrees;
            match self.shape {
                "tee" => vec![(0.0, width(0)), (180.0, width(0)), (second, width(1))],
                "cross" => vec![
                    (0.0, width(0)),
                    (180.0, width(0)),
                    (second, width(1)),
                    (second + 180.0, width(1)),
                ],
                _ => vec![(180.0, width(0)), (second, width(1))],
            }
        }
    }

    /// Все случаи: три расположения, четыре пары видов, три пары ширин, у тройника ещё со сдвигом начала второй
    /// дороги, и углы от 30° до 150° в обе стороны.
    fn cases() -> Vec<Case> {
        let mut out = Vec::new();
        for shape in ["tee", "cross", "bend"] {
            for kinds in [
                [STREET, STREET],
                [MAIN, STREET],
                [MAIN, MAIN],
                [STREET, MAIN],
            ] {
                for widths in [[1.0, 1.0], [0.7, 1.0], [1.0, 1.3]] {
                    for slide in if shape == "tee" {
                        vec![0.0, 0.2]
                    } else {
                        vec![0.0]
                    } {
                        for degrees in (30..=150).step_by(5) {
                            for sign in [1.0, -1.0] {
                                out.push(Case {
                                    shape,
                                    degrees: sign * f64::from(degrees),
                                    kinds,
                                    widths,
                                    slide,
                                });
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn layout_of(
        case: &Case,
        kinds: &RoadKindTable,
        models: &ModelTable,
    ) -> (Vec<PlacedPiece>, Junction) {
        let (pieces, junctions) = lay_out(&case.roads(), kinds, models);
        let joint = at(&junctions, MEETING)
            .unwrap_or_else(|| panic!("{case:?}: перекрёстка нет"))
            .clone();
        (pieces, joint)
    }

    fn joint_of(case: &Case, kinds: &RoadKindTable, models: &ModelTable) -> Junction {
        layout_of(case, kinds, models).1
    }

    #[test]
    fn bent_copies_stay_unfolded_for_every_tee_cross_and_bend_from_30_to_150_degrees_on_both_sides()
    {
        let (kinds, models) = world();
        let all = cases();
        let folded: Vec<String> = all
            .iter()
            .filter_map(|case| {
                let count = folds(&joint_of(case, &kinds, &models), &models);
                (count > 0).then(|| format!("{case:?}: {count}"))
            })
            .collect();
        assert_eq!(all.len(), 3 * 4 * 3 * 25 * 2 + 4 * 3 * 25 * 2);
        assert!(folded.is_empty(), "вывернутые треугольники: {folded:#?}");
    }

    #[test]
    fn where_no_arm_is_stretched_every_point_keeps_its_distance_to_the_middle() {
        let (kinds, models) = world();
        let mut kept = 0;
        for case in cases() {
            let joint = joint_of(&case, &kinds, &models);
            if joint.arms.iter().any(|arm| arm.extension > 0.0) {
                continue;
            }
            let model = models.get(joint.model).expect("модель");
            for (part, source) in joint.parts.iter().zip(&model.parts) {
                for (copy, original) in part.vertices.iter().zip(&source.vertices) {
                    let before = f64::from(original.position[0] - model.origin[0])
                        .hypot(f64::from(original.position[1] - model.origin[1]));
                    let after = f64::from(copy.position[0]).hypot(f64::from(copy.position[1]));
                    assert!(
                        (after - before * joint.scale).abs() < 1e-4,
                        "{case:?}: было {before} × {}, стало {after}",
                        joint.scale
                    );
                }
            }
            kept += 1;
        }
        assert!(kept > 1000, "{kept}");
    }

    /// Вершина сетки `model` на оси рукава `arm` в `along` от середины файла и `lateral` сбоку от неё.
    fn on_axis(model: &Model, arm: &JunctionArm, along: f64, lateral: f64) -> Option<usize> {
        let (sin, cos) = arm.dir.sin_cos();
        let file = [along * cos - lateral * sin, along * sin + lateral * cos];
        let target = [
            file[0] + f64::from(model.origin[0]),
            -file[1] + f64::from(model.origin[1]),
        ];
        model.parts[0].vertices.iter().position(|v| {
            (f64::from(v.position[0]) - target[0]).abs() < 1e-4
                && (f64::from(v.position[1]) - target[1]).abs() < 1e-4
        })
    }

    /// Точка копии как вектор на карте «против часовой, если смотреть сверху».
    fn planar(junction: &Junction, index: usize) -> Vec2 {
        let p = junction.parts[0].vertices[index].position;
        [f64::from(p[0]), -f64::from(p[1])]
    }

    /// Номер рукава дорог `roads`, на чьём луче лежит угол `angle`.
    fn road_at(roads: &[(f64, f64)], angle: f64) -> Option<usize> {
        roads.iter().position(|&(road, _)| {
            let apart = (angle - road + 180.0).rem_euclid(360.0) - 180.0;
            apart.abs() < 1e-3
        })
    }

    /// Рукава ближе 30° друг к другу перекрёстка не получают (требование 7): ни выреза, ни модели, куски дорог идут до
    /// самой точки стыка. Ровно на 30° и шире модель есть.
    #[test]
    fn two_adjacent_arms_closer_than_30_degrees_have_no_model_and_the_pieces_run_up_to_the_joint() {
        let (kinds, models) = world();
        for degrees in [20.0, 29.0, -20.0, -29.0, 160.0, 151.0, -160.0, -151.0] {
            let case = Case {
                shape: "tee",
                degrees,
                kinds: [STREET, STREET],
                widths: [1.0, 1.0],
                slide: 0.0,
            };
            let (pieces, junctions) = lay_out(&case.roads(), &kinds, &models);
            assert!(
                at(&junctions, MEETING).is_none(),
                "{degrees}°: у тесных рукавов нет модели"
            );
            let mut layout = RoadKindLayout::default();
            layout.sync_with(&kinds, &models, |buffer| buffer.extend(case.roads()));
            assert_eq!(layout.bare_spots(), 1, "{degrees}°: одно место без модели");
            // Вторая дорога доходит до точки стыка: ближайший конец её кусков — в ней.
            let (sin, cos) = degrees.to_radians().sin_cos();
            let dir = [cos, sin];
            let reach = pieces
                .iter()
                .filter(|p| (p.cos - dir[1]).abs() < 1e-9 && (p.sin + dir[0]).abs() < 1e-9)
                .map(|p| dot(sub(p.center, MEETING), dir) - p.length / 2.0)
                .fold(f64::INFINITY, f64::min);
            assert!(
                reach.abs() < 1e-6,
                "{degrees}°: куски кончаются в {reach} от точки стыка"
            );
            // Первая дорога идёт сквозь точку стыка: к ней примыкает кусок.
            assert!(
                pieces.iter().any(|p| {
                    (p.center[1] - MEETING[1]).abs() < 1e-6
                        && (p.center[0] - MEETING[0]).abs() <= p.length / 2.0 + 1e-6
                }),
                "{degrees}°: первая дорога не доходит до точки стыка"
            );
        }
        for degrees in [30.0, 31.0, -30.0, 150.0, 149.0, -150.0] {
            let case = Case {
                shape: "tee",
                degrees,
                kinds: [STREET, STREET],
                widths: [1.0, 1.0],
                slide: 0.0,
            };
            assert!(
                at(&lay_out(&case.roads(), &kinds, &models).1, MEETING).is_some(),
                "{degrees}°: на 30° и шире модель есть"
            );
            let mut layout = RoadKindLayout::default();
            layout.sync_with(&kinds, &models, |buffer| buffer.extend(case.roads()));
            assert_eq!(
                layout.bare_spots(),
                0,
                "{degrees}°: у каждого места своя модель"
            );
        }
    }

    #[test]
    fn the_mouth_of_every_model_arm_lands_on_the_axis_of_a_road_at_reach_minus_fade_times_the_scale_plus_the_extension()
     {
        let (kinds, models) = world();
        for case in cases() {
            let joint = joint_of(&case, &kinds, &models);
            let model = models.get(joint.model).expect("модель");
            let described = kinds
                .junctions()
                .iter()
                .find(|j| j.model == joint.model)
                .expect("описание модели");
            let roads = case.arms();
            let mut taken = vec![false; roads.len()];
            for arm in &described.arms {
                let index = on_axis(model, arm, arm.reach - arm.fade, 0.0)
                    .unwrap_or_else(|| panic!("{case:?}: в сетке нет вершины на оси рукава"));
                let [x, y] = planar(&joint, index);
                let angle = libm::atan2(y, x).to_degrees();
                let found = road_at(&roads, angle).unwrap_or_else(|| {
                    panic!("{case:?}: устье на {angle}° не лежит на оси дорог {roads:?}")
                });
                assert!(!taken[found], "{case:?}: два рукава модели на одной дороге");
                taken[found] = true;
                // Рукав перекрёстка на этой дороге: устье на его `mouth`, то есть на `(reach − fade) × s + Δ`.
                let placed = joint
                    .arms
                    .iter()
                    .find(|placed| {
                        let apart =
                            (placed.angle.to_degrees() - angle + 180.0).rem_euclid(360.0) - 180.0;
                        apart.abs() < 1e-3
                    })
                    .expect("рукав на этой дороге");
                let base = (arm.reach - arm.fade) * joint.scale;
                assert!(
                    (placed.mouth - base - placed.extension).abs() < 1e-9
                        && (x.hypot(y) - placed.mouth).abs() < 1e-4,
                    "{case:?}: устье на расстоянии {} вместо {}",
                    x.hypot(y),
                    placed.mouth
                );
            }
        }
    }

    #[test]
    fn the_pieces_of_every_road_begin_at_reach_minus_fade_times_the_scale_plus_the_extension() {
        let (kinds, models) = world();
        let mut stretched = 0;
        for case in cases() {
            let (pieces, joint) = layout_of(&case, &kinds, &models);
            for arm in &joint.arms {
                // Рукав на карте смотрит по углу `angle` против часовой, то есть по оси x и к меньшим y.
                let dir = [arm.angle.cos(), -arm.angle.sin()];
                let start = pieces
                    .iter()
                    .filter(|p| {
                        let from = sub(p.center, MEETING);
                        cross(from, dir).abs() < 1e-6 && dot(from, dir) > 0.0
                    })
                    .map(|p| dot(sub(p.center, MEETING), dir) - p.length / 2.0)
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    (start - arm.mouth).abs() < 1e-6,
                    "{case:?}: куски начинаются в {start} вместо {}",
                    arm.mouth
                );
                stretched += usize::from(arm.extension > 0.0);
            }
        }
        assert!(stretched > 1000, "острые углы удлиняют рукава: {stretched}");
    }

    #[test]
    fn at_every_angle_the_mouth_edges_lie_at_the_road_width_across_within_two_percent() {
        let (kinds, models) = world();
        let mut checked = 0;
        for case in cases() {
            let joint = joint_of(&case, &kinds, &models);
            let model = models.get(joint.model).expect("модель");
            let described = kinds
                .junctions()
                .iter()
                .find(|j| j.model == joint.model)
                .expect("описание модели");
            let roads = case.arms();
            for arm in &described.arms {
                let start = arm.reach - arm.fade;
                let half = arm.width / 2.0;
                let axis = planar(&joint, on_axis(model, arm, start, 0.0).expect("ось"));
                let angle = libm::atan2(axis[1], axis[0]).to_degrees();
                let road_width = roads[road_at(&roads, angle).expect("дорога")].1;
                for side in [-1.0, 1.0] {
                    let edge = planar(
                        &joint,
                        on_axis(model, arm, start, side * half).expect("край"),
                    );
                    let (ex, ey) = (edge[0] - axis[0], edge[1] - axis[1]);
                    let length = axis[0].hypot(axis[1]);
                    let across = (ey * axis[0] - ex * axis[1]) / length;
                    let want = half * road_width / arm.width;
                    assert!(
                        (across.abs() - want).abs() < 0.02 * want,
                        "{case:?}: край в {across} от оси вместо {want}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 5000, "{checked}");
    }
}

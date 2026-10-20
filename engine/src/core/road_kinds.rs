//! «Трёхмерная сцена» → «Вид дороги»: дорога выглядит дорогой — прямые куски своего вида вдоль оси и узлы на
//! стыках, тупиках и изломах. Модуль не знает о `World` и видеокарте: дороги ему приносят готовыми
//! прямоугольниками (`RoadShape`) с видом, оси и связи он берёт у `roads`, а кладёт куски и узлы по ним.

use std::collections::HashMap;
use std::f64::consts::PI;

use super::roads::{Link, Road, RoadShape, add_scaled, cross, distance, dot, find_links, sub};
use super::value::Vec2;

/// Излом двух дорог одного вида и ширины, с которого на их стыке стоит узел, в градусах: меньше —
/// раскладка каждой продолжена за точку связи.
pub const NODE_MIN_TURN_DEGREES: f64 = 30.0;
/// Точки связей разных дорог ближе этого друг к другу — одна точка, клеток.
pub const MERGE_DISTANCE: f64 = 0.05;

const EPS: f64 = 1e-9;

/// Модель прямого куска: номер в таблице моделей игры и её размеры в файле — ширина по X и длина по Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PieceModel {
    pub model: usize,
    pub width: f64,
    pub length: f64,
}

/// Модель узла: номер в таблице моделей игры и её ширина по X.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeModel {
    pub model: usize,
    pub width: f64,
}

/// Вид дороги `scene.road_kinds`: сменные варианты прямого куска и узел.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadKind {
    pub name: String,
    pub pieces: Vec<PieceModel>,
    pub node: NodeModel,
}

/// Виды дорог игры; у вида, чьё имя в `road_kind` дороги, номер в этой таблице.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadKindTable {
    kinds: Vec<RoadKind>,
}

impl RoadKindTable {
    pub fn new(kinds: Vec<RoadKind>) -> RoadKindTable {
        RoadKindTable { kinds }
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

/// Узел на месте: то же, что у куска, без растяжения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedNode {
    pub model: usize,
    pub center: Vec2,
    pub z: f64,
    pub cos: f64,
    pub sin: f64,
    pub scale: f64,
    /// Ширина узла на земле после увеличения.
    pub width: f64,
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

/// Точки связей, слитые по `MERGE_DISTANCE`, и номер точки у каждой связи. Точка связи — место на оси
/// несущей дороги (`point_b`): у пересечения осей оно общее, у конца внутри чужого прямоугольника — на оси
/// той, в чей прямоугольник конец зашёл.
fn cluster_links(links: &[Link]) -> (Vec<Cluster>, Vec<usize>) {
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut grid = PointGrid::default();
    let mut of_link = Vec::with_capacity(links.len());
    for link in links {
        let index = grid.find(link.point_b).unwrap_or_else(|| {
            clusters.push(Cluster {
                point: link.point_b,
                roads: Vec::new(),
            });
            grid.insert(clusters.len() - 1, link.point_b);
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

/// Направления, в которых дорога уходит от точки: сквозь точку — два, кончающаяся в ней — одно, внутрь
/// дороги. Точка не дальше `MERGE_DISTANCE` от конца оси или за ним — конец: кончается дорога.
fn directions_at(road: &Road, point: Vec2) -> Vec<Vec2> {
    let along = dot(sub(point, road.end(0)), road.dir);
    let backward = [-road.dir[0], -road.dir[1]];
    if along <= MERGE_DISTANCE {
        vec![road.dir]
    } else if along >= road.length() - MERGE_DISTANCE {
        vec![backward]
    } else {
        vec![road.dir, backward]
    }
}

fn width_of(road: &Road) -> f64 {
    road.half_width * 2.0
}

/// Что стоит в точке стыка.
enum Joint {
    /// Нет дорог вида: класть нечего.
    Nothing,
    /// Узел по самой широкой дороге вида в точке.
    Node(usize),
    /// Две дороги одного вида и ширины сходятся концами под малым изломом: узла нет, раскладка каждой
    /// продолжена за точку. Излом в радианах.
    Bend(f64),
}

fn joint_at(cluster: &Cluster, roads: &[LayoutRoad], shapes: &[Road]) -> Joint {
    let kinded: Vec<usize> = cluster
        .roads
        .iter()
        .copied()
        .filter(|&road| roads[road].kind.is_some())
        .collect();
    let Some(widest) = kinded.iter().copied().max_by(|&a, &b| {
        width_of(&shapes[a])
            .total_cmp(&width_of(&shapes[b]))
            .then(shapes[b].id.cmp(&shapes[a].id))
    }) else {
        return Joint::Nothing;
    };
    let directions: Vec<Vec2> = cluster
        .roads
        .iter()
        .flat_map(|&road| directions_at(&shapes[road], cluster.point))
        .collect();
    if directions.len() != 2 {
        return Joint::Node(widest);
    }
    let [first, second] = [directions[0], directions[1]];
    let turn = PI - libm::atan2(cross(first, second).abs(), dot(first, second));
    let [a, b] = [cluster.roads[0], cluster.roads[1]];
    let alike = roads[a].kind == roads[b].kind
        && (width_of(&shapes[a]) - width_of(&shapes[b])).abs() <= EPS;
    if alike && turn < NODE_MIN_TURN_DEGREES.to_radians() - EPS {
        Joint::Bend(turn)
    } else {
        Joint::Node(widest)
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

/// Место узла и дорога, по которой он выбран: самая широкая дорога вида в точке.
struct Site {
    point: Vec2,
    road: usize,
}

/// Узлы раскладки по местам; места ближе `MERGE_DISTANCE` — одно, и узел берёт дорога пошире.
#[derive(Default)]
struct Sites {
    sites: Vec<Site>,
    grid: PointGrid,
}

impl Sites {
    fn add(&mut self, point: Vec2, road: usize, shapes: &[Road]) {
        let Some(known) = self.grid.find(point) else {
            self.grid.insert(self.sites.len(), point);
            self.sites.push(Site { point, road });
            return;
        };
        let (held, new) = (&shapes[self.sites[known].road], &shapes[road]);
        let wider = width_of(new)
            .total_cmp(&width_of(held))
            .then(held.id.cmp(&new.id));
        if wider.is_gt() {
            self.sites[known].road = road;
        }
    }
}

/// Поворот модели, чья локальная ось +Y идёт вдоль `dir`: косинус и синус для `ModelInstance`.
fn rotation_along(dir: Vec2) -> (f64, f64) {
    (dir[1], -dir[0])
}

/// Прямые куски дороги `road` на раскладке `span` — расстояниях от начала оси до начала и до конца раскладки:
/// `n` кусков подряд, каждый растянут вдоль до `L / n`.
fn pieces_of(road: &Road, kind_pieces: &[PieceModel], span: [f64; 2], out: &mut Vec<PlacedPiece>) {
    let length = span[1] - span[0];
    if length <= EPS {
        return;
    }
    let width = width_of(road);
    let first = &kind_pieces[0];
    let unit_length = first.length * width / first.width;
    let count = ((length / unit_length + 0.5 + EPS).floor() as usize).max(1);
    let each = length / count as f64;
    let (cos, sin) = rotation_along(road.dir);
    for k in 0..count {
        let variant = &kind_pieces[(k + road.id as usize) % kind_pieces.len()];
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
}

fn node_of(
    site: &Site,
    roads: &[LayoutRoad],
    shapes: &[Road],
    kinds: &RoadKindTable,
) -> Option<PlacedNode> {
    let road = &shapes[site.road];
    let node = kinds.get(roads[site.road].kind?)?.node;
    let (cos, sin) = rotation_along(road.dir);
    let scale = width_of(road) / node.width;
    Some(PlacedNode {
        model: node.model,
        center: site.point,
        z: road.z,
        cos,
        sin,
        scale,
        width: node.width * scale,
    })
}

/// Раскладка видов дорог по дорогам мира (требования «Вида дороги» 9–17): куски вдоль осей дорог с видом
/// и узлы на стыках, изломах и в тупиках. Связи берутся со всех дорог, в том числе закрытых и без вида.
pub fn lay_out(roads: &[LayoutRoad], kinds: &RoadKindTable) -> (Vec<PlacedPiece>, Vec<PlacedNode>) {
    if roads.iter().all(|road| road.kind.is_none()) {
        return (Vec::new(), Vec::new());
    }
    let shapes: Vec<Road> = roads
        .iter()
        .map(|road| Road::from_shape(&road.shape))
        .collect();
    let links = find_links(&shapes);
    let (clusters, of_link) = cluster_links(&links);
    let mut links_of: Vec<Vec<usize>> = vec![Vec::new(); roads.len()];
    for (index, link) in links.iter().enumerate() {
        links_of[link.a].push(index);
        links_of[link.b].push(index);
    }
    let joints: Vec<Joint> = clusters
        .iter()
        .map(|cluster| joint_at(cluster, roads, &shapes))
        .collect();

    let mut sites = Sites::default();
    for (cluster, joint) in clusters.iter().zip(&joints) {
        if let Joint::Node(road) = joint {
            sites.add(cluster.point, *road, &shapes);
        }
    }
    let mut pieces = Vec::new();
    for (index, road) in roads.iter().enumerate() {
        let (Some(kind), shape) = (road.kind, &shapes[index]) else {
            continue;
        };
        let Some(kind) = kinds.get(kind) else {
            continue;
        };
        if width_of(shape) <= EPS {
            continue;
        }
        let ends = [0, 1].map(|end| layout_end(shape, end, &links_of[index], &of_link, &clusters));
        let reach = |end: &LayoutEnd| match end.cluster.map(|cluster| &joints[cluster]) {
            Some(Joint::Bend(turn)) => width_of(shape) / 2.0 * libm::tan(turn / 2.0),
            _ => 0.0,
        };
        let span = [
            ends[0].along - reach(&ends[0]),
            ends[1].along + reach(&ends[1]),
        ];
        pieces_of(shape, &kind.pieces, span, &mut pieces);
        let dead: Vec<&LayoutEnd> = ends.iter().filter(|end| end.cluster.is_none()).collect();
        if dead.len() == 2 && span[1] - span[0] <= EPS {
            sites.add(shape.point_at(shape.half_length), index, &shapes);
        } else {
            for end in dead {
                sites.add(shape.point_at(end.along), index, &shapes);
            }
        }
    }
    let nodes = sites
        .sites
        .iter()
        .filter_map(|site| node_of(site, roads, &shapes, kinds))
        .collect();
    (pieces, nodes)
}

/// Раскладка, что держится между кадрами: пересобирается, только когда сменились дороги мира —
/// появились, исчезли, сдвинулись, сменили размер, поворот, высоту, признак `road` или вид. Скорость на
/// дороге в `LayoutRoad` не входит, поэтому её смена раскладку не трогает.
#[derive(Debug, Default)]
pub struct RoadKindLayout {
    roads: Vec<LayoutRoad>,
    pieces: Vec<PlacedPiece>,
    nodes: Vec<PlacedNode>,
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

    pub fn nodes(&self) -> &[PlacedNode] {
        &self.nodes
    }

    /// Приводит раскладку к дорогам, которые `fill` кладёт в переданный буфер; `true`, если пересобрана.
    /// Буфер живёт между вызовами: кадр без перемен ничего не выделяет.
    pub fn sync_with(
        &mut self,
        kinds: &RoadKindTable,
        fill: impl FnOnce(&mut Vec<LayoutRoad>),
    ) -> bool {
        let mut fresh = std::mem::take(&mut self.fresh);
        fill(&mut fresh);
        let changed = fresh != self.roads;
        if changed {
            std::mem::swap(&mut self.roads, &mut fresh);
            (self.pieces, self.nodes) = lay_out(&self.roads, kinds);
            self.version += 1;
        }
        self.fresh = fresh;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREET: usize = 0;
    const MAIN: usize = 1;

    /// Виды: «street» — кусок 1 × 2 (модели 0 и 1, второй вариант той же формы) и узел 1 (модель 2); «main» —
    /// кусок 1 × 2 (модель 3) и узел 1 (модель 4).
    fn kinds() -> RoadKindTable {
        let piece = |model| PieceModel {
            model,
            width: 1.0,
            length: 2.0,
        };
        RoadKindTable::new(vec![
            RoadKind {
                name: "street".to_string(),
                pieces: vec![piece(0), piece(1)],
                node: NodeModel {
                    model: 2,
                    width: 1.0,
                },
            },
            RoadKind {
                name: "main".to_string(),
                pieces: vec![piece(3)],
                node: NodeModel {
                    model: 4,
                    width: 1.0,
                },
            },
        ])
    }

    /// Только «street» с одним вариантом куска.
    fn single_street() -> RoadKindTable {
        let mut street = kinds().get(STREET).cloned().expect("вид есть");
        street.pieces.truncate(1);
        RoadKindTable::new(vec![street])
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
        (a - b).abs() < 1e-9
    }

    fn near_point(a: Vec2, b: Vec2) -> bool {
        near(a[0], b[0]) && near(a[1], b[1])
    }

    /// «Вид дороги», пример требования 18: перекрёсток крестом двух улиц.
    #[test]
    fn two_streets_crossing_lie_in_pieces_with_a_node_in_the_cross_and_in_each_dead_end() {
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET)),
        ];
        let (pieces, nodes) = lay_out(&roads, &single_street());
        assert_eq!(pieces.len(), 20);

        // А: раскладка от 0,5 до 20,5 — длина 20, десять кусков по 2 без растяжения.
        for (k, piece) in pieces[..10].iter().enumerate() {
            assert!(
                near(piece.length, 2.0) && near(piece.stretch, 1.0),
                "{piece:?}"
            );
            assert!(
                near_point(piece.center, [1.5 + 2.0 * k as f64, 10.0]),
                "{piece:?}"
            );
            assert!(near(piece.scale, 1.0));
        }
        // Б: раскладка от 0,5 до 19,5 — длина 19, `n` = 9,5 → 10 кусков по 1,9, сжатых вдоль до 0,95.
        for (k, piece) in pieces[10..].iter().enumerate() {
            assert!(near(piece.length, 1.9), "{piece:?}");
            assert!(near(piece.stretch, 0.95), "{piece:?}");
            assert!(
                near_point(piece.center, [10.0, 0.5 + 0.95 + 1.9 * k as f64]),
                "{piece:?}"
            );
        }

        let centers: Vec<Vec2> = nodes.iter().map(|node| node.center).collect();
        assert_eq!(centers.len(), 5, "{centers:?}");
        for expected in [
            [10.0, 10.0],
            [0.5, 10.0],
            [20.5, 10.0],
            [10.0, 0.5],
            [10.0, 19.5],
        ] {
            assert!(
                centers.iter().any(|&center| near_point(center, expected)),
                "нет узла в {expected:?}: {centers:?}"
            );
        }
        assert!(nodes.iter().all(|node| near(node.scale, 1.0)));
    }

    /// Пример требования 19: Т-образный стык узкой улицы с широкой.
    #[test]
    fn a_narrow_street_ending_on_a_wide_one_stops_on_its_axis_and_the_node_takes_the_wide_kind() {
        let roads = [
            road(0, [30.0, 9.25], [10.0, 1.5], Some(MAIN)),
            road(1, [34.5, 10.5], [1.0, 9.5], Some(STREET)),
        ];
        let table = RoadKindTable::new(vec![
            kinds().get(STREET).cloned().expect("вид"),
            kinds().get(MAIN).cloned().expect("вид"),
        ]);
        let (pieces, nodes) = lay_out(&roads, &table);
        // Широкая: от 0,75 до 9,25 вдоль оси; 8,5 / (2 × 1,5) = 2,83 → 3 куска.
        assert_eq!(pieces.iter().filter(|p| p.model == 3).count(), 3);
        // Узкая: от (35; 10) до тупика (35; 19,5) — длина 9,5, пять кусков по 1,9.
        let narrow: Vec<&PlacedPiece> = pieces.iter().filter(|p| p.model != 3).collect();
        assert_eq!(narrow.len(), 5);
        let (first, last) = (narrow[0], narrow[4]);
        assert!(near_point(first.center, [35.0, 10.0 + 0.95]), "{first:?}");
        assert!(near_point(last.center, [35.0, 19.5 - 0.95]), "{last:?}");
        assert!(
            narrow
                .iter()
                .all(|p| p.center[1] - p.length / 2.0 >= 10.0 - 1e-9),
            "кусок узкой не выходит за ось широкой"
        );
        // Узел в стыке — вида широкой, шириной 1,5.
        let joint = nodes
            .iter()
            .find(|node| near_point(node.center, [35.0, 10.0]))
            .expect("узел в стыке");
        assert_eq!(joint.model, 4);
        assert!(
            near(joint.scale, 1.5) && near(joint.width, 1.5),
            "{joint:?}"
        );
        // Тупик узкой — узел узкой улицы.
        let dead_end = nodes
            .iter()
            .find(|node| near_point(node.center, [35.0, 19.5]))
            .expect("узел в тупике");
        assert_eq!(dead_end.model, 2);
    }

    /// Две улицы шириной 1, сходящиеся концами в (60; 10): вторая отходит от первой на `degrees`.
    fn bend(degrees: f64) -> [LayoutRoad; 2] {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let center = [60.0 + cos * 5.0, 10.0 + sin * 5.0];
        let mut second = road(
            1,
            [center[0] - 5.0, center[1] - 0.5],
            [10.0, 1.0],
            Some(STREET),
        );
        second.shape.sin_cos = (sin, cos);
        [road(0, [50.0, 9.5], [10.0, 1.0], Some(STREET)), second]
    }

    /// Пример требования 20: излом меньше 30° — узла нет, раскладка каждой продолжена на `w / 2 × tg(угол / 2)`.
    #[test]
    fn a_small_bend_has_no_node_and_both_layouts_run_past_the_joint() {
        let (pieces, nodes) = lay_out(&bend(20.0), &single_street());
        let reach = 0.5 * 10.0_f64.to_radians().tan();
        assert!((reach - 0.088).abs() < 1e-3);
        // Только тупики на дальних концах: узла на изломе нет.
        assert_eq!(nodes.len(), 2, "{nodes:?}");
        assert!(
            nodes
                .iter()
                .all(|node| distance(node.center, [60.0, 10.0]) > 1.0)
        );
        // По пять кусков на дорогу, вместе длиной 9,5 и продолжение.
        assert_eq!(pieces.len(), 10);
        for road_pieces in [&pieces[..5], &pieces[5..]] {
            let covered: f64 = road_pieces.iter().map(|p| p.length).sum();
            assert!(near(covered, 9.5 + reach), "{covered}");
        }
    }

    #[test]
    fn a_right_angle_bend_gets_a_node_of_the_road_width_and_no_extension() {
        let roads = [
            road(0, [50.0, 9.5], [10.0, 1.0], Some(STREET)),
            road(1, [59.5, 10.0], [1.0, 10.0], Some(STREET)),
        ];
        let (pieces, nodes) = lay_out(&roads, &single_street());
        let joint = nodes
            .iter()
            .find(|node| near_point(node.center, [60.0, 10.0]))
            .expect("узел на изломе");
        assert!(near(joint.width, 1.0));
        let covered: f64 = pieces.iter().map(|p| p.length).sum();
        assert!(
            near(covered, 9.5 + 9.5),
            "продолжения за точку нет: {covered}"
        );
    }

    #[test]
    fn two_roads_of_different_kind_or_width_get_a_node_even_in_a_straight_line() {
        let straight = |second_kind: Option<usize>, second_width: f64| {
            [
                road(0, [50.0, 9.5], [10.0, 1.0], Some(STREET)),
                road(
                    1,
                    [60.0, 10.0 - second_width / 2.0],
                    [10.0, second_width],
                    second_kind,
                ),
            ]
        };
        let table = kinds();
        let at_joint = |nodes: &[PlacedNode]| {
            nodes
                .iter()
                .find(|n| near_point(n.center, [60.0, 10.0]))
                .copied()
        };
        let (_, same) = lay_out(&straight(Some(STREET), 1.0), &table);
        assert!(at_joint(&same).is_none(), "одного вида и ширины — узла нет");
        let (_, other_kind) = lay_out(&straight(Some(MAIN), 1.0), &table);
        assert!(at_joint(&other_kind).is_some());
        let (_, wider) = lay_out(&straight(Some(STREET), 1.5), &table);
        assert!(
            near(at_joint(&wider).expect("узел").width, 1.5),
            "узел по самой широкой"
        );
        let (_, unkinded) = lay_out(&straight(None, 1.0), &table);
        assert!(
            at_joint(&unkinded).is_some(),
            "у дороги без вида в стыке тоже узел"
        );
    }

    #[test]
    fn a_node_is_rotated_along_the_widest_road_and_the_first_in_the_scene_wins_a_tie() {
        let roads = [
            road(7, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(3, [9.5, 0.0], [1.0, 20.0], Some(STREET)),
        ];
        let (_, nodes) = lay_out(&roads, &kinds());
        let cross = nodes
            .iter()
            .find(|node| near_point(node.center, [10.0, 10.0]))
            .expect("узел");
        // Равная ширина: берётся дорога с меньшим номером — вертикальная, её ось вдоль +y.
        assert!(near(cross.cos, 1.0) && near(cross.sin, 0.0), "{cross:?}");
        let wide = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.25, 0.0], [1.5, 20.0], Some(STREET)),
        ];
        let (_, nodes) = lay_out(&wide, &kinds());
        let cross = nodes
            .iter()
            .find(|node| near_point(node.center, [10.0, 10.0]))
            .expect("узел");
        assert!(near(cross.width, 1.5), "{cross:?}");
    }

    #[test]
    fn a_square_road_has_a_zero_length_layout_so_no_pieces_and_one_node_in_the_middle() {
        let (pieces, nodes) = lay_out(&[road(0, [10.0, 10.0], [1.0, 1.0], Some(STREET))], &kinds());
        assert!(pieces.is_empty());
        assert_eq!(nodes.len(), 1, "{nodes:?}");
        assert!(near_point(nodes[0].center, [10.5, 10.5]), "{:?}", nodes[0]);
    }

    #[test]
    fn a_layout_shorter_than_half_a_piece_is_one_piece_squeezed_along() {
        // Ось 1,5 вдоль x при ширине 1: раскладка от 0,5 до 1,0 — длина 0,5; 0,5 / 2 даёт 0, но кусок один.
        let roads = [road(0, [0.0, 0.0], [1.5, 1.0], Some(STREET))];
        let (pieces, _) = lay_out(&roads, &single_street());
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
        let (pieces, _) = lay_out(&roads, &kinds());
        let models: Vec<usize> = pieces.iter().map(|p| p.model).collect();
        // У каждой: от 0,5 до 8,5 — длина 8, четыре куска.
        assert_eq!(models, [0, 1, 0, 1, 1, 0, 1, 0]);
    }

    #[test]
    fn a_layout_is_rebuilt_only_when_a_road_changes() {
        let table = kinds();
        let mut layout = RoadKindLayout::default();
        let roads = vec![road(0, [0.0, 0.0], [9.0, 1.0], Some(STREET))];
        assert!(layout.sync_with(&table, |out| out.clone_from(&roads)));
        let built = layout.version();
        assert!(!layout.sync_with(&table, |out| out.clone_from(&roads)));
        assert_eq!(layout.version(), built, "те же дороги — раскладка та же");
        let moved = vec![road(0, [0.0, 1.0], [9.0, 1.0], Some(STREET))];
        assert!(layout.sync_with(&table, |out| out.clone_from(&moved)));
        assert!(layout.version() > built);
        assert!(near(layout.pieces()[0].center[1], 1.5));
    }

    #[test]
    fn roads_without_a_kind_lay_nothing_but_still_join_the_roads_that_have_one() {
        let table = kinds();
        let (pieces, nodes) = lay_out(&[road(0, [0.0, 0.0], [9.0, 1.0], None)], &table);
        assert!(pieces.is_empty() && nodes.is_empty());
        // Улица без вида кончается на улице с видом: в стыке три направления — узел.
        let roads = [
            road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)),
            road(1, [9.5, 10.0], [1.0, 10.0], None),
        ];
        let (_, nodes) = lay_out(&roads, &table);
        assert!(
            nodes.iter().any(|n| near_point(n.center, [10.0, 10.0])),
            "{nodes:?}"
        );
    }

    #[test]
    fn roads_at_different_heights_do_not_join() {
        let mut upper = road(1, [9.5, 0.0], [1.0, 20.0], Some(STREET));
        upper.shape.z = 1.2;
        let roads = [road(0, [0.0, 9.5], [21.0, 1.0], Some(STREET)), upper];
        let (_, nodes) = lay_out(&roads, &kinds());
        assert!(
            nodes.iter().all(|n| !near_point(n.center, [10.0, 10.0])),
            "дорога на мосту с дорогой под ним не связана"
        );
        assert!(
            nodes.iter().any(|n| near(n.z, 1.2)),
            "мост — на высоте настила"
        );
    }

    #[test]
    fn link_points_closer_than_the_merge_distance_are_one_point() {
        // Две узкие улицы кончаются на широкой в двух точках в 0,03 друг от друга: узел один.
        let roads = [
            road(0, [0.0, 9.25], [21.0, 1.5], Some(MAIN)),
            road(1, [9.5, 10.5], [1.0, 5.0], Some(STREET)),
            road(2, [9.53, 10.5], [1.0, 5.0], Some(STREET)),
        ];
        let (_, nodes) = lay_out(&roads, &kinds());
        let at_joint = nodes
            .iter()
            .filter(|n| distance(n.center, [10.0, 10.0]) < 0.2)
            .count();
        assert_eq!(at_joint, 1, "{nodes:?}");
    }
}

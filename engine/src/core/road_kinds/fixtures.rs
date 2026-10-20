//! Общее для проверок раскладки и перекрёстков: виды дорог, модели перекрёстков из плоских сеток и таблицы моделей,
//! собранные в коде. Сетка — объединение полос рукавов вдоль осей (кратных 90°) с квадратом посередине, как в
//! настоящих моделях, но обычная квадратная: без тонких треугольников.

use std::collections::HashMap;

use super::super::model::{Material, Model, ModelTable, ModelVertex, Part};
use super::{JunctionArm, JunctionModel, PieceModel, RoadKind, RoadKindTable};

pub(super) const STREET: usize = 0;
pub(super) const MAIN: usize = 1;

/// Ширина куска улицы и главной улицы в единицах файла.
pub(super) const STREET_WIDTH: f64 = 2.0;
pub(super) const MAIN_WIDTH: f64 = 3.0;
/// Шаг квадратной сетки.
const STEP: f64 = 0.25;
/// Расстояние до устья и гаснущая полоса рукава обычной модели: полоса начинается на `REACH − FADE` от середины.
pub(super) const REACH: f64 = 5.0;
pub(super) const FADE: f64 = 2.0;

/// Рукав описания: угол в градусах, вид.
pub(super) type ArmSpec = (f64, usize);

fn kind_width(kind: usize) -> f64 {
    if kind == MAIN {
        MAIN_WIDTH
    } else {
        STREET_WIDTH
    }
}

/// Сетка из полос `arms` (угол, полуширина, длина от середины), каждая начинается позади середины на свою
/// полуширину; вершины в осях файла (`y` сцены — вниз), потом середина коробки ставится в начало, как это делает
/// загрузка модели.
pub(super) fn strips(name: &str, arms: &[(f64, f64, f64)]) -> Model {
    let inside = |x: f64, y: f64| {
        let (px, py) = (x, -y);
        arms.iter().any(|&(degrees, half, reach)| {
            let (sin, cos) = degrees.to_radians().sin_cos();
            let along = px * cos + py * sin;
            let lateral = py * cos - px * sin;
            along >= -half && along <= reach && lateral.abs() <= half
        })
    };
    let extent = arms
        .iter()
        .map(|&(_, half, reach)| reach.max(half))
        .fold(0.0, f64::max);
    let cells = (extent / STEP).ceil() as i64;
    let mut index: HashMap<(i64, i64), u32> = HashMap::new();
    let mut vertices: Vec<[f64; 2]> = Vec::new();
    let mut at = |i: i64, j: i64, vertices: &mut Vec<[f64; 2]>| {
        *index.entry((i, j)).or_insert_with(|| {
            vertices.push([i as f64 * STEP, j as f64 * STEP]);
            vertices.len() as u32 - 1
        })
    };
    let mut indices = Vec::new();
    for i in -cells..cells {
        for j in -cells..cells {
            let (x, y) = ((i as f64 + 0.5) * STEP, (j as f64 + 0.5) * STEP);
            if !inside(x, y) {
                continue;
            }
            let a = at(i, j, &mut vertices);
            let b = at(i, j + 1, &mut vertices);
            let c = at(i + 1, j, &mut vertices);
            let d = at(i + 1, j + 1, &mut vertices);
            indices.extend([a, b, c, b, d, c]);
        }
    }
    let low = vertices.iter().fold([f64::INFINITY; 2], |low, v| {
        [low[0].min(v[0]), low[1].min(v[1])]
    });
    let high = vertices.iter().fold([f64::NEG_INFINITY; 2], |high, v| {
        [high[0].max(v[0]), high[1].max(v[1])]
    });
    let shift = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
    let part = Part {
        vertices: vertices
            .iter()
            .map(|v| ModelVertex {
                position: [(v[0] - shift[0]) as f32, (v[1] - shift[1]) as f32, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [
                    ((v[0] - low[0]) / (high[0] - low[0])) as f32,
                    ((v[1] - low[1]) / (high[1] - low[1])) as f32,
                ],
                tangent: [1.0, 0.0, 0.0, -1.0],
                color: [1.0; 4],
            })
            .collect(),
        indices,
        material: 0,
    };
    Model {
        name: name.to_string(),
        parts: vec![part],
        materials: vec![Material::plain()],
        images: Vec::new(),
        size: [high[0] - low[0], high[1] - low[1], 0.0],
        origin: [-shift[0] as f32, -shift[1] as f32],
    }
}

/// Модель перекрёстка по описанию рукавов: рукава обычной длины `REACH` с гаснущей полосой `FADE`; тупик — с
/// полосой на всю длину рукава (его кусок дороги начинается прямо в точке тупика).
pub(super) fn junction_model(name: &str, arms: &[ArmSpec]) -> (Model, Vec<JunctionArm>) {
    let reach = |arms: &[ArmSpec]| if arms.len() == 1 { 2.0 } else { REACH };
    let fade = |arms: &[ArmSpec]| if arms.len() == 1 { 2.0 } else { FADE };
    let strips_of: Vec<(f64, f64, f64)> = arms
        .iter()
        .map(|&(degrees, kind)| (degrees, kind_width(kind) / 2.0, reach(arms)))
        .collect();
    let described = arms
        .iter()
        .map(|&(degrees, kind)| JunctionArm {
            dir: degrees.to_radians(),
            kind,
            reach: reach(arms),
            fade: fade(arms),
            width: kind_width(kind),
        })
        .collect();
    (strips(name, &strips_of), described)
}

/// Все модели перекрёстков набора: тупики, переходы, повороты, тройники и кресты улиц и главной улицы.
pub(super) fn junction_specs() -> Vec<(&'static str, Vec<ArmSpec>)> {
    let line = |kinds: [usize; 2]| vec![(0.0, kinds[0]), (180.0, kinds[1])];
    let bend = |kinds: [usize; 2]| vec![(0.0, kinds[0]), (90.0, kinds[1])];
    let tee = |kinds: [usize; 3]| vec![(0.0, kinds[0]), (90.0, kinds[1]), (180.0, kinds[2])];
    let cross = |kinds: [usize; 4]| {
        vec![
            (0.0, kinds[0]),
            (90.0, kinds[1]),
            (180.0, kinds[2]),
            (270.0, kinds[3]),
        ]
    };
    vec![
        ("end_street", vec![(0.0, STREET)]),
        ("end_main", vec![(0.0, MAIN)]),
        ("line_main_street", line([MAIN, STREET])),
        ("line_main_main", line([MAIN, MAIN])),
        ("bend_street_street", bend([STREET, STREET])),
        ("bend_main_street", bend([MAIN, STREET])),
        ("bend_main_main", bend([MAIN, MAIN])),
        ("tee_street_street", tee([STREET, STREET, STREET])),
        ("tee_main_street", tee([MAIN, STREET, MAIN])),
        ("tee_street_main", tee([STREET, MAIN, STREET])),
        ("tee_main_main", tee([MAIN, MAIN, MAIN])),
        ("cross_street_street", cross([STREET; 4])),
        ("cross_main_street", cross([MAIN, STREET, MAIN, STREET])),
        ("cross_main_main", cross([MAIN; 4])),
    ]
}

/// Виды и модели: «street» — куски 2 × 4 (модели 0 и 1, два варианта), «main» — кусок 3 × 6 (модель 2); дальше —
/// модели перекрёстков `junction_specs` в порядке записи, у каждой сетка из полос.
pub(super) fn tables() -> (RoadKindTable, ModelTable) {
    let piece = |model: usize, width: f64| PieceModel {
        model,
        width,
        length: width * 2.0,
    };
    let kinds = vec![
        RoadKind {
            name: "street".to_string(),
            pieces: vec![piece(0, STREET_WIDTH), piece(1, STREET_WIDTH)],
        },
        RoadKind {
            name: "main".to_string(),
            pieces: vec![piece(2, MAIN_WIDTH)],
        },
    ];
    let mut models: Vec<Model> = ["street_a", "street_b", "main_a"]
        .iter()
        .map(|name| Model::placeholder(name))
        .collect();
    let mut junctions = Vec::new();
    for (name, arms) in junction_specs() {
        let (model, described) = junction_model(name, &arms);
        junctions.push(JunctionModel {
            model: models.len(),
            arms: described,
        });
        models.push(model);
    }
    (
        RoadKindTable::new(kinds, junctions),
        ModelTable::new(models),
    )
}

/// Те же виды и модели, но с перекрёстками из  в этом порядке; имя можно назвать дважды.
pub(super) fn tables_with(keep: &[&str]) -> (RoadKindTable, ModelTable) {
    let (all, models) = tables();
    let specs = junction_specs();
    let junctions = keep
        .iter()
        .filter_map(|name| {
            let at = specs.iter().position(|(known, _)| known == name)?;
            all.junctions().get(at).cloned()
        })
        .collect();
    let kinds = (0..2).filter_map(|index| all.get(index).cloned()).collect();
    (RoadKindTable::new(kinds, junctions), models)
}

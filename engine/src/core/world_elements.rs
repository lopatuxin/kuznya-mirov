//! «Надписи, полоски и картинки в мире», требование 17: computes every world element's place, fill,
//! color, text and image — in scene cells, in drawing order (требование 14) — from the world, its
//! properties and the resolved `screens.json` list. Pure function of its three inputs: no browser, no GPU, so
//! it is tested without a videocard. `wasm::mod` only converts these cells to pixels and draws.

use super::camera::Camera3d;
use super::property::{self, PropertyTable};
use super::scene::{self, LayerView, SceneConfig};
use super::screens::{
    self, Align, Anchor, FontId, MaxSpec, WorldColor, WorldElement, WorldElementKind, WorldTextPart,
};
use super::shapes::Body;
use super::step::selector_matches;
use super::value::{ImageId, Vec2};
use super::world::World;

/// One colored rectangle in scene cells — a bar's backing or fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

/// «Надписи и полоски в мире», требования 10–11: `back`, only when the element names
/// `back_color` (`value`/`max` having resolved at all — a missing property drops the whole
/// element, подложка included, before a `BarDraw` is ever built); `fill`'s width already carries
/// the `value / max` fraction, clamped to `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarDraw {
    pub back: Option<WorldRect>,
    pub fill: WorldRect,
    /// Номер элемента в `world_elements`, как у `LabelDraw`.
    pub element: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelDraw {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub text: String,
    pub font: FontId,
    pub font_size: f32,
    pub align: Align,
    pub color: [f32; 4],
    /// Номер элемента в `world_elements`: общий порядок надписей и картинок — порядок списка.
    pub element: usize,
}

/// «Надписи, полоски и картинки в мире», требования 1–5: картинка над объектом `object` — какая
/// (`image`) и где (`position`, `size`; клетки сцены или точки окна, как у надписи).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageDraw {
    pub object: u32,
    pub image: ImageId,
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// Номер элемента в `world_elements`, как у `LabelDraw`.
    pub element: usize,
}

impl ImageDraw {
    /// Левый и верхний края входят, правый и нижний нет — как у прямоугольника объекта.
    fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.position[0]
            && point[0] < self.position[0] + self.size[0]
            && point[1] >= self.position[1]
            && point[1] < self.position[1] + self.size[1]
    }
}

/// Полоски, надписи и картинки мира: каждый список уже в порядке рисования.
pub type WorldDraws = (Vec<BarDraw>, Vec<LabelDraw>, Vec<ImageDraw>);

/// «Надписи, полоски и картинки в мире», требование 6: объект, чья картинка стоит под точкой `point`
/// (в тех же единицах, что `ImageDraw`). Из нескольких — верхняя по порядку рисования, то есть
/// последняя в списке.
pub fn image_object_at(images: &[ImageDraw], point: [f32; 2]) -> Option<u32> {
    images
        .iter()
        .rev()
        .find(|draw| draw.contains(point))
        .map(|draw| draw.object)
}

/// Подряд идущие полоски, надписи или картинки слоя мира: сколько их, считая с того места, где
/// кончился прошлый отрезок того же списка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayRun {
    Bars(usize),
    Labels(usize),
    Images(usize),
}

/// «Надписи, полоски и картинки в мире», требование 5: полоски, надписи и картинки идут между собой
/// в порядке списка `world_elements` — отрезки всех трёх видов, чередуясь, в порядке рисования, —
/// кроме одного: текст поверх полосок. Полоска, стоящая в списке после первой надписи, ложится
/// прямо под неё. Три списка уже в порядке элементов, так что это слияние по `element`.
pub fn overlay_runs(
    bars: &[BarDraw],
    labels: &[LabelDraw],
    images: &[ImageDraw],
) -> Vec<OverlayRun> {
    let first_label = labels.first().map_or(usize::MAX, |label| label.element);
    let mut runs: Vec<OverlayRun> = Vec::new();
    let (mut next_bar, mut next_label, mut next_image) = (0, 0, 0);
    loop {
        // (место в порядке рисования, 0 у полоски и 1 у остальных — полоска впереди надписи).
        let candidates = [
            bars.get(next_bar)
                .map(|bar| ((bar.element.min(first_label), 0), OverlayRun::Bars(1))),
            labels
                .get(next_label)
                .map(|label| ((label.element, 1), OverlayRun::Labels(1))),
            images
                .get(next_image)
                .map(|image| ((image.element, 1), OverlayRun::Images(1))),
        ];
        let Some((_, next)) = candidates.into_iter().flatten().min_by_key(|(key, _)| *key) else {
            break;
        };
        match (runs.last_mut(), next) {
            (Some(OverlayRun::Bars(count)), OverlayRun::Bars(_))
            | (Some(OverlayRun::Labels(count)), OverlayRun::Labels(_))
            | (Some(OverlayRun::Images(count)), OverlayRun::Images(_)) => *count += 1,
            _ => runs.push(next),
        }
        match next {
            OverlayRun::Bars(_) => next_bar += 1,
            OverlayRun::Labels(_) => next_label += 1,
            OverlayRun::Images(_) => next_image += 1,
        }
    }
    runs
}

fn resolve_world_color(color: &WorldColor, world: &World, id: u32) -> Option<[f32; 4]> {
    match color {
        WorldColor::Solid(c) => Some(*c),
        // «Цвет из списка», требование 12: округление вниз, прижатое к первому и последнему —
        // тот же приём, что у числа из таблицы в правилах (`core::step::resolve_number_expr`),
        // повторённый здесь напрямую: типы разные (список цветов, а не чисел), общей функции нет.
        WorldColor::Table { colors, by } => {
            let v = world.number_like(id, *by)?;
            let idx = v.floor();
            let i = if idx < 0.0 {
                0
            } else {
                (idx as usize).min(colors.len().saturating_sub(1))
            };
            colors.get(i).copied()
        }
    }
}

/// «Надписи и полоски в мире», требование 8: the element's top-left corner in scene cells — its
/// *center* sits at the anchor point on the object's own rectangle (`position`/`size`), shifted by
/// `offset`. Reuses `Anchor::fractions` (the same nine points a screen element's corner anchors to
/// a window edge), read here as a point on the object instead.
fn element_top_left(
    anchor: Anchor,
    offset: [f32; 2],
    size: [f32; 2],
    position: Vec2,
    obj_size: Vec2,
) -> [f32; 2] {
    let anchor_point = anchor.point_on(
        [position[0] as f32, position[1] as f32],
        [obj_size[0] as f32, obj_size[1] as f32],
    );
    let center = [anchor_point[0] + offset[0], anchor_point[1] + offset[1]];
    [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0]
}

pub(crate) fn format_world_text(
    parts: &[WorldTextPart],
    world: &World,
    id: u32,
    properties: &PropertyTable,
) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            WorldTextPart::Literal(s) => out.push_str(s),
            WorldTextPart::Value(prop) => {
                out.push_str(&screens::format_property(world, id, *prop, properties))
            }
        }
    }
    out
}

fn resolve_max(max: &MaxSpec, world: &World, id: u32) -> Option<f64> {
    match max {
        MaxSpec::Const(n) => Some(*n as f64),
        MaxSpec::Property(p) => world.number_like(id, *p),
    }
}

/// «Надписи и полоски в мире», требование 17: `elements` в порядке `screens.json` → «Отрисовка»,
/// требование 14 — между собой элементы идут в порядке списка, а над разными объектами одного
/// элемента — в порядке их рисования (`scene::draw_order`). Возвращает полоски, надписи и картинки
/// отдельными списками, каждый уже в этом самом порядке; общий порядок трёх видов между собой даёт
/// `overlay_runs` по `element`. «Слои глубины»,
/// требование 18: над объектом слоя — над местом, где он нарисован, с тем же сдвигом, и один раз,
/// без копий `repeat_x`.
pub fn compute_world_draws(
    world: &World,
    scene: &SceneConfig,
    properties: &PropertyTable,
    elements: &[WorldElement],
    layers: &LayerView,
) -> WorldDraws {
    compute_draws(world, scene, properties, elements, None, layers, 1.0)
}

/// То же при клетке надписей в `cell` клеток сцены — с камерой игрока она не растёт с приближением,
/// `SceneConfig::label_cell`.
pub fn compute_world_draws_scaled(
    world: &World,
    scene: &SceneConfig,
    properties: &PropertyTable,
    elements: &[WorldElement],
    layers: &LayerView,
    cell: f64,
) -> WorldDraws {
    compute_draws(world, scene, properties, elements, None, layers, cell)
}

/// «Трёхмерная сцена» → «Мышь и надписи», требования 24–25: то же для трёхмерной сцены, но
/// результат — в точках окна, а не в клетках сцены. `anchor` встаёт на прямоугольник, который фигура
/// (или, без фигуры, её место на земле) занимает на экране; `size`, `offset` и `font_size` переведены
/// в точки одной клеткой — высота окна, делённая на `view_height`, — так что над ближним и дальним
/// объектом они одного размера. Объект, которого на экране нет (за камерой), элемента не получает.
pub fn compute_world_draws_3d(
    world: &World,
    scene: &SceneConfig,
    properties: &PropertyTable,
    elements: &[WorldElement],
    camera: &Camera3d,
) -> WorldDraws {
    compute_draws(
        world,
        scene,
        properties,
        elements,
        Some(camera),
        &LayerView::default(),
        1.0,
    )
}

/// Прямоугольник объекта на экране в точках окна — `[x, y]` и `[ширина, высота]`.
fn screen_rectangle(world: &World, id: u32, camera: &Camera3d) -> Option<([f32; 2], [f32; 2])> {
    let [x0, y0, x1, y1] = match Body::of_object(world, id).or_else(|| Body::of_model(world, id)) {
        Some(body) => body.screen_rect(camera)?,
        None => {
            let position = world.vec2(id, property::POSITION)?;
            let size = world.vec2(id, property::SIZE)?;
            let footprint = super::footprint::Footprint::rotated(
                position,
                size,
                world.rotation(id, property::ROTATION),
            );
            let mut rect = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            let base = world.base_z(id);
            for corner in footprint.corners() {
                let point = camera.project([corner[0], corner[1], base])?;
                rect = [
                    rect[0].min(point[0]),
                    rect[1].min(point[1]),
                    rect[2].max(point[0]),
                    rect[3].max(point[1]),
                ];
            }
            rect
        }
    };
    Some(([x0 as f32, y0 as f32], [(x1 - x0) as f32, (y1 - y0) as f32]))
}

fn compute_draws(
    world: &World,
    scene: &SceneConfig,
    properties: &PropertyTable,
    elements: &[WorldElement],
    camera: Option<&Camera3d>,
    layers: &LayerView,
    flat_cell: f64,
) -> WorldDraws {
    let mut bars = Vec::new();
    let mut labels = Vec::new();
    let mut images = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        let mut ids: Vec<u32> = world
            .ids()
            .filter(|&id| selector_matches(&element.for_, world, id))
            .filter(|&id| {
                world.vec2(id, property::POSITION).is_some()
                    && world.vec2(id, property::SIZE).is_some()
            })
            .collect();
        ids.sort_by(|&a, &b| scene::draw_order(world, scene, a, b));
        for id in ids {
            let Some(color) = resolve_world_color(&element.color, world, id) else {
                continue;
            };
            let (position, obj_size, cell) = match camera {
                None => {
                    let position = world.vec2(id, property::POSITION).expect("filtered above");
                    let shift = layers.displacement(scene::parallax(world, id));
                    (
                        [position[0] + shift[0], position[1] + shift[1]],
                        world.vec2(id, property::SIZE).expect("filtered above"),
                        flat_cell as f32,
                    )
                }
                Some(camera) => {
                    let Some((position, size)) = screen_rectangle(world, id, camera) else {
                        continue;
                    };
                    (
                        [position[0] as f64, position[1] as f64],
                        [size[0] as f64, size[1] as f64],
                        camera.cell_points() as f32,
                    )
                }
            };
            let placement_size = element.placement.size.map(|c| c * cell);
            let placement_offset = element.placement.offset.map(|c| c * cell);
            let top_left = element_top_left(
                element.placement.anchor,
                placement_offset,
                placement_size,
                position,
                obj_size,
            );
            match &element.kind {
                WorldElementKind::Bar {
                    value,
                    max,
                    back_color,
                } => {
                    let Some(value_n) = world.number_like(id, *value) else {
                        continue;
                    };
                    let Some(max_n) = resolve_max(max, world, id) else {
                        continue;
                    };
                    let fraction = if max_n <= 0.0 {
                        0.0
                    } else {
                        (value_n / max_n).clamp(0.0, 1.0) as f32
                    };
                    let size = placement_size;
                    bars.push(BarDraw {
                        back: back_color.map(|c| WorldRect {
                            position: top_left,
                            size,
                            color: c,
                        }),
                        fill: WorldRect {
                            position: top_left,
                            size: [size[0] * fraction, size[1]],
                            color,
                        },
                        element: index,
                    });
                }
                WorldElementKind::Label {
                    text,
                    font,
                    font_size,
                    align,
                } => {
                    labels.push(LabelDraw {
                        position: top_left,
                        size: placement_size,
                        text: format_world_text(text, world, id, properties),
                        font: *font,
                        font_size: *font_size * cell,
                        align: *align,
                        color,
                        element: index,
                    });
                }
                WorldElementKind::Image { image } => {
                    let Some(image) = image.resolve(world, id, properties) else {
                        continue;
                    };
                    images.push(ImageDraw {
                        object: id,
                        image,
                        position: top_left,
                        size: placement_size,
                        element: index,
                    });
                }
            }
        }
    }
    (bars, labels, images)
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;
    use crate::core::property::PropertyId;
    use crate::core::rules::Selector;
    use crate::core::screens::{ImageRef, Placement};
    use crate::core::value::PropKind;

    fn scene_config() -> SceneConfig {
        SceneConfig {
            width: 20,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
            cell_pixels: None,
            player_camera: None,
        }
    }

    fn placement(anchor: Anchor, offset: [f32; 2], size: [f32; 2]) -> Placement {
        Placement {
            anchor,
            offset,
            size,
        }
    }

    fn object_with_rect(world: &mut World, pos: Vec2, size: Vec2) -> u32 {
        let id = world.create();
        world.set_vec2(id, property::POSITION, pos);
        world.set_vec2(id, property::SIZE, size);
        id
    }

    fn selector_has(props: &[PropertyId]) -> Selector {
        Selector {
            has: props.to_vec(),
            without: Vec::new(),
        }
    }

    fn bar_element(value: PropertyId, max: MaxSpec, back_color: Option<[f32; 4]>) -> WorldElement {
        WorldElement {
            for_: selector_has(&[value]),
            placement: placement(Anchor::Center, [0.0, 0.0], [2.0, 0.5]),
            color: WorldColor::Solid([1.0, 0.0, 0.0, 1.0]),
            kind: WorldElementKind::Bar {
                value,
                max,
                back_color,
            },
        }
    }

    #[test]
    fn element_center_sits_at_each_of_nine_anchors_with_offset() {
        let position: Vec2 = [10.0, 10.0];
        let obj_size: Vec2 = [4.0, 2.0];
        let offset = [0.5, -0.5];
        let size = [1.0, 1.0];

        let anchors = [
            (Anchor::TopLeft, [10.0, 10.0]),
            (Anchor::Top, [12.0, 10.0]),
            (Anchor::TopRight, [14.0, 10.0]),
            (Anchor::Left, [10.0, 11.0]),
            (Anchor::Center, [12.0, 11.0]),
            (Anchor::Right, [14.0, 11.0]),
            (Anchor::BottomLeft, [10.0, 12.0]),
            (Anchor::Bottom, [12.0, 12.0]),
            (Anchor::BottomRight, [14.0, 12.0]),
        ];
        for (anchor, anchor_point) in anchors {
            let top_left = element_top_left(anchor, offset, size, position, obj_size);
            let center = [top_left[0] + size[0] / 2.0, top_left[1] + size[1] / 2.0];
            let expected = [anchor_point[0] + offset[0], anchor_point[1] + offset[1]];
            assert!(
                (center[0] - expected[0]).abs() < 1e-4 && (center[1] - expected[1]).abs() < 1e-4,
                "{anchor:?}: center={center:?}, expected={expected:?}"
            );
        }
    }

    #[test]
    fn element_size_in_cells_is_independent_of_object_size() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let obj = object_with_rect(&mut world, [0.0, 0.0], [50.0, 50.0]);
        world.set_number(obj, hp, 1.0);
        let scene = scene_config();
        let mut element = bar_element(hp, MaxSpec::Const(1.0), None);
        element.placement = placement(Anchor::Top, [0.0, 0.0], [2.0, 0.4]);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars[0].fill.size, [2.0, 0.4]);
    }

    /// «Слои глубины», требование 18: полоска над объектом слоя — над местом, где он нарисован, с
    /// тем же сдвигом; у объекта на `parallax` 1 она остаётся на записанном месте.
    #[test]
    fn a_bar_over_a_depth_layer_object_is_shifted_with_it() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let layer = object_with_rect(&mut world, [10.0, 10.0], [4.0, 2.0]);
        world.set_number(layer, hp, 1.0);
        world.set_number(layer, property::PARALLAX, 0.25);
        let plain = object_with_rect(&mut world, [10.0, 10.0], [4.0, 2.0]);
        world.set_number(plain, hp, 1.0);
        let scene = scene_config();
        let layers = LayerView {
            shift: [20.0, 6.0],
            window_cells: 0.0,
        };
        let mut element = bar_element(hp, MaxSpec::Const(1.0), None);
        element.placement = placement(Anchor::TopLeft, [0.0, 0.0], [2.0, 0.5]);
        let (bars, _, _) = compute_world_draws(&world, &scene, &properties, &[element], &layers);
        assert_eq!(bars.len(), 2);
        // Середина полоски — в углу объекта, левый верхний угол полоски на полразмера левее и выше.
        assert_eq!(bars[0].fill.position, [24.0, 14.25], "сдвиг 15 и 4,5");
        assert_eq!(
            bars[1].fill.position,
            [9.0, 9.75],
            "parallax 1 — без сдвига"
        );
    }

    /// «Слои глубины», требование 18: у `repeat_x` надпись одна — над самим объектом, без копий.
    #[test]
    fn a_repeating_object_gets_one_element_not_one_per_copy() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let layer = object_with_rect(&mut world, [10.0, 10.0], [4.0, 2.0]);
        world.set_number(layer, hp, 1.0);
        world.set_number(layer, property::PARALLAX, 0.5);
        world.set_flag(layer, property::REPEAT_X, true);
        let scene = scene_config();
        let layers = LayerView {
            shift: [0.0, 0.0],
            window_cells: 20.0,
        };
        let element = bar_element(hp, MaxSpec::Const(1.0), None);
        let (bars, _, _) = compute_world_draws(&world, &scene, &properties, &[element], &layers);
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn object_without_position_or_size_gets_no_elements() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let obj = world.create(); // bare object: no position, no size
        world.set_number(obj, hp, 1.0);
        let scene = scene_config();
        let element = bar_element(hp, MaxSpec::Const(1.0), None);
        let (bars, labels, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert!(bars.is_empty());
        assert!(labels.is_empty());
    }

    #[test]
    fn invisible_object_still_gets_its_element() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        // No color, no image — invisible, but has position and size.
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 1.0);
        let scene = scene_config();
        let element = bar_element(hp, MaxSpec::Const(1.0), None);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn selector_picks_only_matching_objects() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let with_hp = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(with_hp, hp, 5.0);
        object_with_rect(&mut world, [5.0, 5.0], [1.0, 1.0]); // no hp: should be skipped

        let scene = scene_config();
        let element = bar_element(hp, MaxSpec::Const(10.0), None);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn created_object_gets_the_element_deleted_object_loses_it() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let element = bar_element(hp, MaxSpec::Const(10.0), None);

        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            std::slice::from_ref(&element),
            &LayerView::default(),
        );
        assert!(bars.is_empty(), "нет объекта — нет элемента");

        let id = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(id, hp, 5.0);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            std::slice::from_ref(&element),
            &LayerView::default(),
        );
        assert_eq!(bars.len(), 1, "объект создан — элемент появился");

        world.delete(id);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert!(bars.is_empty(), "объект удалён — элемента больше нет");
    }

    #[test]
    fn bar_fraction_full_at_or_above_max() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let max_hp = properties
            .declare_author("max_hp", PropKind::Number)
            .unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 45.0);
        world.set_number(obj, max_hp, 30.0);
        let element = bar_element(hp, MaxSpec::Property(max_hp), None);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars[0].fill.size[0], 2.0); // full width (element width is 2.0 cells)
    }

    #[test]
    fn bar_fraction_empty_below_zero() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let max_hp = properties
            .declare_author("max_hp", PropKind::Number)
            .unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, -5.0);
        world.set_number(obj, max_hp, 30.0);
        let element = bar_element(hp, MaxSpec::Property(max_hp), None);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars[0].fill.size[0], 0.0);
    }

    #[test]
    fn max_as_a_plain_number_works_like_a_max_property() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 5.0);
        let element = bar_element(hp, MaxSpec::Const(10.0), None);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars[0].fill.size[0], 1.0); // half of the 2.0-cell width
    }

    #[test]
    fn max_property_at_or_below_zero_is_an_empty_bar_but_still_shows_backing() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let max_hp = properties
            .declare_author("max_hp", PropKind::Number)
            .unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 5.0);
        world.set_number(obj, max_hp, 0.0);
        let back = [0.0, 0.0, 0.0, 0.5];
        let element = bar_element(hp, MaxSpec::Property(max_hp), Some(back));
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars[0].fill.size[0], 0.0);
        assert_eq!(bars[0].back.map(|b| b.color), Some(back));
    }

    #[test]
    fn missing_value_or_max_property_drops_the_whole_bar_backing_included() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let max_hp = properties
            .declare_author("max_hp", PropKind::Number)
            .unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]); // neither hp nor max_hp set
        let mut element = bar_element(hp, MaxSpec::Property(max_hp), Some([0.0; 4]));
        element.for_ = Selector::default();
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert!(bars.is_empty());
    }

    #[test]
    fn back_color_covers_the_whole_element() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 1.0);
        let element = bar_element(hp, MaxSpec::Const(1.0), Some([0.0, 0.0, 0.0, 0.5]));
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars[0].back.unwrap().size, [2.0, 0.5]);
    }

    fn label_element_with_color(color: WorldColor) -> WorldElement {
        WorldElement {
            for_: Selector::default(),
            placement: placement(Anchor::Center, [0.0, 0.0], [1.0, 1.0]),
            color,
            kind: WorldElementKind::Label {
                text: vec![],
                font: 0,
                font_size: 0.3,
                align: Align::Left,
            },
        }
    }

    #[test]
    fn color_table_picks_by_floor_clamped_to_first_and_last() {
        let colors = [
            [0.6, 0.6, 0.6, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
        ];
        let cases: [(f64, [f32; 4]); 4] = [
            (1.7, colors[1]),
            (-3.0, colors[0]),
            (99.0, colors[2]),
            (0.0, colors[0]),
        ];
        for (danger_value, expected) in cases {
            let mut properties = PropertyTable::new();
            let danger = properties
                .declare_author("danger", PropKind::Number)
                .unwrap();
            let mut world = World::new(&properties);
            let scene = scene_config();
            let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
            world.set_number(obj, danger, danger_value);
            let mut element = label_element_with_color(WorldColor::Table {
                colors: colors.to_vec(),
                by: danger,
            });
            element.for_ = selector_has(&[danger]);
            let (_, labels, _) = compute_world_draws(
                &world,
                &scene,
                &properties,
                &[element],
                &LayerView::default(),
            );
            assert_eq!(labels[0].color, expected, "danger={danger_value}");
        }
    }

    #[test]
    fn missing_by_property_drops_the_element_entirely() {
        let mut properties = PropertyTable::new();
        let danger = properties
            .declare_author("danger", PropKind::Number)
            .unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]); // no `danger` set
        let element = label_element_with_color(WorldColor::Table {
            colors: vec![[1.0, 1.0, 1.0, 1.0]],
            by: danger,
        });
        let (bars, labels, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert!(bars.is_empty());
        assert!(labels.is_empty());
    }

    #[test]
    fn label_substitutes_own_property_by_kind_and_falls_back_to_empty() {
        let mut properties = PropertyTable::new();
        let level = properties
            .declare_author("level", PropKind::Number)
            .unwrap();
        let elapsed = properties
            .declare_author("elapsed", PropKind::Time)
            .unwrap();
        let on = properties.declare_author("on", PropKind::Flag).unwrap();
        let title = properties.declare_author("title", PropKind::Text).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, level, 3.0);
        world.set_time(obj, elapsed, 90); // 1.5s at 60 steps/s
        world.set_flag(obj, on, true);
        world.set_text(obj, title, "boss".to_string());

        let element = WorldElement {
            for_: Selector::default(),
            placement: placement(Anchor::Center, [0.0, 0.0], [2.0, 0.4]),
            color: WorldColor::Solid([1.0; 4]),
            kind: WorldElementKind::Label {
                text: vec![
                    WorldTextPart::Literal("Ур. ".to_string()),
                    WorldTextPart::Value(level),
                    WorldTextPart::Literal("/".to_string()),
                    WorldTextPart::Value(elapsed),
                    WorldTextPart::Literal("/".to_string()),
                    WorldTextPart::Value(on),
                    WorldTextPart::Literal("/".to_string()),
                    WorldTextPart::Value(title),
                    WorldTextPart::Literal("/".to_string()),
                    WorldTextPart::Value(property::POSITION),
                ],
                font: 2,
                font_size: 0.35,
                align: Align::Center,
            },
        };
        let (_, labels, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(labels[0].text, "Ур. 3/1.5/да/boss/");
        assert_eq!(labels[0].font, 2);
        assert_eq!(labels[0].font_size, 0.35);
        assert_eq!(labels[0].align, Align::Center);
    }

    #[test]
    fn order_follows_list_order_then_object_draw_order() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = SceneConfig {
            y_sort: true,
            camera: None,
            light: Default::default(),
            cell_pixels: None,
            player_camera: None,
            ..scene_config()
        };
        // Two objects at the same layer, y_sort on: the lower bottom edge draws on top.
        let lower = object_with_rect(&mut world, [0.0, 5.0], [1.0, 1.0]); // bottom edge 6
        let upper = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]); // bottom edge 1
        world.set_number(lower, hp, 1.0);
        world.set_number(upper, hp, 1.0);

        let element = bar_element(hp, MaxSpec::Const(1.0), None);
        let (bars, _, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(bars.len(), 2);
        // The object drawn on top (`lower`, bigger bottom edge) is pushed last — «полоска
        // объекта, нарисованного поверх, лежит поверх». Center anchor, element size [2.0, 0.5]:
        // top-left y is the object's own center y minus half the element height (0.25).
        assert_eq!(bars[0].fill.position[1], 0.25, "upper (object 1) first");
        assert_eq!(
            bars[1].fill.position[1], 5.25,
            "lower (object 0) last, drawn on top"
        );
    }

    #[test]
    fn text_always_pushed_separately_from_bars_regardless_of_element_list_order() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 1.0);
        let bar = bar_element(hp, MaxSpec::Const(1.0), None);
        let label = WorldElement {
            for_: selector_has(&[hp]),
            placement: placement(Anchor::Center, [0.0, 0.0], [1.0, 1.0]),
            color: WorldColor::Solid([1.0; 4]),
            kind: WorldElementKind::Label {
                text: vec![],
                font: 0,
                font_size: 0.3,
                align: Align::Left,
            },
        };
        let (bars, labels, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[label, bar],
            &LayerView::default(),
        );
        assert_eq!(bars.len(), 1);
        assert_eq!(labels.len(), 1);
    }

    #[test]
    fn compute_world_draws_never_mutates_the_world() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let obj = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(obj, hp, 1.0);
        let before = format!("{world:?}");
        let element = bar_element(hp, MaxSpec::Const(1.0), None);
        let _ = compute_world_draws(
            &world,
            &scene,
            &properties,
            &[element],
            &LayerView::default(),
        );
        assert_eq!(format!("{world:?}"), before);
    }

    fn image_element(for_: Selector, placement: Placement, image: ImageRef) -> WorldElement {
        WorldElement {
            for_,
            placement,
            color: WorldColor::Solid([1.0; 4]),
            kind: WorldElementKind::Image { image },
        }
    }

    fn label_over(for_: Selector) -> WorldElement {
        WorldElement {
            for_,
            ..label_element_with_color(WorldColor::Solid([1.0; 4]))
        }
    }

    fn icon_names() -> Arc<[String]> {
        ["food", "bakery"].map(String::from).into_iter().collect()
    }

    /// Фаза 44, требования 1–4: картинка по `{icon}` — названная картинка над своим объектом в
    /// прямоугольнике `size` с серединой в точке `anchor` со сдвигом `offset`; у объекта без свойства,
    /// с пустым и с неизвестным именем картинки нет, и ошибки нет.
    #[test]
    fn an_image_by_substitution_is_drawn_only_for_a_known_name() {
        let mut properties = PropertyTable::new();
        let icon = properties.declare_author("icon", PropKind::Text).unwrap();
        let place = properties.declare_author("place", PropKind::Flag).unwrap();
        let mut world = World::new(&properties);
        let bakery = object_with_rect(&mut world, [10.0, 10.0], [4.0, 2.0]);
        let house = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        let blank = object_with_rect(&mut world, [2.0, 0.0], [1.0, 1.0]);
        let stranger = object_with_rect(&mut world, [4.0, 0.0], [1.0, 1.0]);
        for id in [bakery, house, blank, stranger] {
            world.set_flag(id, place, true);
        }
        world.set_text(bakery, icon, "bakery".to_string());
        world.set_text(blank, icon, String::new());
        world.set_text(stranger, icon, "nothing".to_string());
        let element = image_element(
            selector_has(&[place]),
            placement(Anchor::Top, [0.0, -0.6], [0.8, 0.8]),
            ImageRef::Own {
                property: icon,
                names: icon_names(),
            },
        );

        let (bars, labels, images) = compute_world_draws(
            &world,
            &scene_config(),
            &properties,
            &[element],
            &LayerView::default(),
        );

        assert!(bars.is_empty() && labels.is_empty());
        assert_eq!(images.len(), 1, "{images:?}");
        let draw = images[0];
        assert_eq!(
            (draw.object, draw.image, draw.size),
            (bakery, 1, [0.8, 0.8])
        );
        assert!((draw.position[0] - 11.6).abs() < 1e-4, "{draw:?}");
        assert!((draw.position[1] - 9.0).abs() < 1e-4, "{draw:?}");
    }

    /// Фаза 44, требования 1, 4: картинка без подстановки — у всех объектов отбора.
    #[test]
    fn a_fixed_image_is_drawn_over_every_object_of_the_selection() {
        let mut properties = PropertyTable::new();
        let place = properties.declare_author("place", PropKind::Flag).unwrap();
        let mut world = World::new(&properties);
        for x in 0..3 {
            let id = object_with_rect(&mut world, [x as f64 * 2.0, 0.0], [1.0, 1.0]);
            world.set_flag(id, place, true);
        }
        object_with_rect(&mut world, [9.0, 9.0], [1.0, 1.0]);
        let element = image_element(
            selector_has(&[place]),
            placement(Anchor::Center, [0.0, 0.0], [1.0, 1.0]),
            ImageRef::Fixed(0),
        );

        let (_, _, images) = compute_world_draws(
            &world,
            &scene_config(),
            &properties,
            &[element],
            &LayerView::default(),
        );

        let objects: Vec<u32> = images.iter().map(|draw| draw.object).collect();
        assert_eq!(objects, [0, 1, 2]);
        assert!(images.iter().all(|draw| draw.image == 0));
    }

    /// Фаза 44, «Крайние случаи»: код сменил `icon` — сменился значок; объект создан с `icon` —
    /// значок при следующем расчёте.
    #[test]
    fn a_changed_or_created_icon_changes_the_drawn_image() {
        let mut properties = PropertyTable::new();
        let icon = properties.declare_author("icon", PropKind::Text).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let element = image_element(
            selector_has(&[icon]),
            placement(Anchor::Center, [0.0, 0.0], [1.0, 1.0]),
            ImageRef::Own {
                property: icon,
                names: icon_names(),
            },
        );
        let draw_images = |world: &World| {
            compute_world_draws(
                world,
                &scene,
                &properties,
                std::slice::from_ref(&element),
                &LayerView::default(),
            )
            .2
        };
        assert!(draw_images(&world).is_empty());

        let first = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_text(first, icon, "food".to_string());
        assert_eq!(draw_images(&world)[0].image, 0);

        world.set_text(first, icon, "bakery".to_string());
        assert_eq!(draw_images(&world)[0].image, 1, "код сменил icon");
    }

    /// Фаза 44, требование 5: надписи и картинки между собой — в порядке списка `world_elements`,
    /// отрезки тех и других чередуются; картинки одного элемента идут подряд.
    #[test]
    fn labels_and_images_alternate_in_the_order_of_the_element_list() {
        let mut properties = PropertyTable::new();
        let place = properties.declare_author("place", PropKind::Flag).unwrap();
        let mut world = World::new(&properties);
        for x in 0..2 {
            let id = object_with_rect(&mut world, [x as f64 * 3.0, 0.0], [1.0, 1.0]);
            world.set_flag(id, place, true);
        }
        let label = label_over(selector_has(&[place]));
        let image = image_element(
            selector_has(&[place]),
            placement(Anchor::Center, [0.0, 0.0], [1.0, 1.0]),
            ImageRef::Fixed(0),
        );
        let runs = |elements: &[WorldElement]| {
            let (bars, labels, images) = compute_world_draws(
                &world,
                &scene_config(),
                &properties,
                elements,
                &LayerView::default(),
            );
            overlay_runs(&bars, &labels, &images)
        };

        assert_eq!(
            runs(&[label.clone(), image.clone(), label.clone()]),
            [
                OverlayRun::Labels(2),
                OverlayRun::Images(2),
                OverlayRun::Labels(2)
            ]
        );
        assert_eq!(
            runs(&[image.clone(), label.clone()]),
            [OverlayRun::Images(2), OverlayRun::Labels(2)]
        );
        assert_eq!(runs(&[image.clone(), image]), [OverlayRun::Images(4)]);
        assert_eq!(runs(&[]), []);
    }

    /// Фаза 44, требование 6, «Крайние случаи»: под точкой — объект картинки; из двух внахлёст верхняя —
    /// нарисованная позже; левый и верхний края картинки входят, правый и нижний нет.
    #[test]
    fn a_point_belongs_to_the_topmost_image_under_it() {
        let mut properties = PropertyTable::new();
        let place = properties.declare_author("place", PropKind::Flag).unwrap();
        let mut world = World::new(&properties);
        let lower = object_with_rect(&mut world, [0.0, 0.0], [2.0, 2.0]);
        let upper = object_with_rect(&mut world, [1.0, 0.0], [2.0, 2.0]);
        world.set_flag(lower, place, true);
        world.set_flag(upper, place, true);
        let element = image_element(
            selector_has(&[place]),
            placement(Anchor::Center, [0.0, 0.0], [2.0, 2.0]),
            ImageRef::Fixed(0),
        );
        let (_, _, images) = compute_world_draws(
            &world,
            &scene_config(),
            &properties,
            &[element],
            &LayerView::default(),
        );

        assert_eq!(image_object_at(&images, [0.5, 1.0]), Some(lower));
        assert_eq!(
            image_object_at(&images, [1.5, 1.0]),
            Some(upper),
            "внахлёст"
        );
        assert_eq!(
            image_object_at(&images, [0.0, 0.0]),
            Some(lower),
            "левый верхний угол"
        );
        assert_eq!(
            image_object_at(&images, [3.0, 1.0]),
            None,
            "правый край не входит"
        );
        assert_eq!(
            image_object_at(&images, [1.0, 2.0]),
            None,
            "нижний край не входит"
        );
        assert_eq!(image_object_at(&[], [1.0, 1.0]), None);
    }

    /// Нефункциональное требование: 200 объектов с одной полоской и одной надписью каждый — не
    /// дольше 1 мс в `cargo test --release`.
    #[test]
    fn two_hundred_objects_compute_within_a_millisecond_in_release() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        for i in 0..200 {
            let obj = object_with_rect(&mut world, [(i % 20) as f64, (i / 20) as f64], [1.0, 1.0]);
            world.set_number(obj, hp, 5.0);
        }
        let bar = bar_element(hp, MaxSpec::Const(10.0), Some([0.0, 0.0, 0.0, 0.5]));
        let label = WorldElement {
            for_: selector_has(&[hp]),
            placement: placement(Anchor::Top, [0.0, -0.6], [2.0, 0.4]),
            color: WorldColor::Solid([1.0, 1.0, 1.0, 1.0]),
            kind: WorldElementKind::Label {
                text: vec![WorldTextPart::Value(hp)],
                font: 0,
                font_size: 0.3,
                align: Align::Center,
            },
        };
        let elements = [bar, label];
        let (bars, labels, _) = compute_world_draws(
            &world,
            &scene,
            &properties,
            &elements,
            &LayerView::default(),
        );
        assert_eq!(bars.len(), 200);
        assert_eq!(labels.len(), 200);
        // Лучший из нескольких замеров после прогрева выше: соседние тесты, идущие параллельно,
        // не должны ронять проверку случайной задержкой одного прогона.
        let best = (0..10)
            .map(|_| {
                let start = Instant::now();
                black_box(compute_world_draws(
                    &world,
                    &scene,
                    &properties,
                    &elements,
                    &LayerView::default(),
                ));
                start.elapsed()
            })
            .min()
            .expect("замеров десять");
        if !cfg!(debug_assertions) {
            assert!(best.as_micros() <= 1000, "расчёт занял {best:?}");
        }
    }
}

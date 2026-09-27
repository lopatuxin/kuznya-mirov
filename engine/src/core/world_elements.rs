//! «Надписи и полоски в мире», требование 17: computes every world element's place, fill, color
//! and text — in scene cells, in drawing order (требование 14) — from the world, its properties
//! and the resolved `screens.json` list. Pure function of its three inputs: no browser, no GPU, so
//! it is tested without a videocard. `wasm::mod` only converts these cells to pixels and draws.

use super::property::{self, PropertyTable};
use super::scene::{self, SceneConfig};
use super::screens::{
    self, Align, Anchor, FontId, MaxSpec, WorldColor, WorldElement, WorldElementKind, WorldTextPart,
};
use super::step::selector_matches;
use super::value::Vec2;
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
    let (fx, fy) = anchor.fractions();
    let anchor_point = [
        position[0] as f32 + fx * obj_size[0] as f32,
        position[1] as f32 + fy * obj_size[1] as f32,
    ];
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
/// элемента — в порядке их рисования (`scene::draw_order`). Возвращает полоски и надписи отдельными
/// списками, каждый уже в этом самом порядке: вызывающий кладёт все полоски мира раньше всего
/// текста в мире и получает верный порядок без дополнительной сортировки.
pub fn compute_world_draws(
    world: &World,
    scene: &SceneConfig,
    properties: &PropertyTable,
    elements: &[WorldElement],
) -> (Vec<BarDraw>, Vec<LabelDraw>) {
    let mut bars = Vec::new();
    let mut labels = Vec::new();
    for element in elements {
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
            let position = world.vec2(id, property::POSITION).expect("filtered above");
            let obj_size = world.vec2(id, property::SIZE).expect("filtered above");
            let top_left = element_top_left(
                element.placement.anchor,
                element.placement.offset,
                element.placement.size,
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
                    let size = element.placement.size;
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
                        size: element.placement.size,
                        text: format_world_text(text, world, id, properties),
                        font: *font,
                        font_size: *font_size,
                        align: *align,
                        color,
                    });
                }
            }
        }
    }
    (bars, labels)
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;
    use std::time::Instant;

    use super::*;
    use crate::core::property::PropertyId;
    use crate::core::rules::Selector;
    use crate::core::screens::Placement;
    use crate::core::value::PropKind;

    fn scene_config() -> SceneConfig {
        SceneConfig {
            width: 20,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
        assert_eq!(bars[0].fill.size, [2.0, 0.4]);
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
        let (bars, labels) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
        assert_eq!(bars.len(), 1);
    }

    #[test]
    fn created_object_gets_the_element_deleted_object_loses_it() {
        let mut properties = PropertyTable::new();
        let hp = properties.declare_author("hp", PropKind::Number).unwrap();
        let mut world = World::new(&properties);
        let scene = scene_config();
        let element = bar_element(hp, MaxSpec::Const(10.0), None);

        let (bars, _) =
            compute_world_draws(&world, &scene, &properties, std::slice::from_ref(&element));
        assert!(bars.is_empty(), "нет объекта — нет элемента");

        let id = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]);
        world.set_number(id, hp, 5.0);
        let (bars, _) =
            compute_world_draws(&world, &scene, &properties, std::slice::from_ref(&element));
        assert_eq!(bars.len(), 1, "объект создан — элемент появился");

        world.delete(id);
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
            let (_, labels) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, labels) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (_, labels) = compute_world_draws(&world, &scene, &properties, &[element]);
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
            ..scene_config()
        };
        // Two objects at the same layer, y_sort on: the lower bottom edge draws on top.
        let lower = object_with_rect(&mut world, [0.0, 5.0], [1.0, 1.0]); // bottom edge 6
        let upper = object_with_rect(&mut world, [0.0, 0.0], [1.0, 1.0]); // bottom edge 1
        world.set_number(lower, hp, 1.0);
        world.set_number(upper, hp, 1.0);

        let element = bar_element(hp, MaxSpec::Const(1.0), None);
        let (bars, _) = compute_world_draws(&world, &scene, &properties, &[element]);
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
        let (bars, labels) = compute_world_draws(&world, &scene, &properties, &[label, bar]);
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
        let _ = compute_world_draws(&world, &scene, &properties, &[element]);
        assert_eq!(format!("{world:?}"), before);
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
        let (bars, labels) = compute_world_draws(&world, &scene, &properties, &elements);
        assert_eq!(bars.len(), 200);
        assert_eq!(labels.len(), 200);
        // Лучший из нескольких замеров после прогрева выше: соседние тесты, идущие параллельно,
        // не должны ронять проверку случайной задержкой одного прогона.
        let best = (0..10)
            .map(|_| {
                let start = Instant::now();
                black_box(compute_world_draws(&world, &scene, &properties, &elements));
                start.elapsed()
            })
            .min()
            .expect("замеров десять");
        if !cfg!(debug_assertions) {
            assert!(best.as_micros() <= 1000, "расчёт занял {best:?}");
        }
    }
}

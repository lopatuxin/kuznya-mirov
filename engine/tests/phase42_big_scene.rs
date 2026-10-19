//! Фаза 42 — большая сцена без задержек. Игры собираются в коде теста, как в `phase17_terrain.rs`:
//! список настилов мира, поиск настила только среди них, готовая форма плоского объекта на земле,
//! которая пересчитывается, только когда меняется то, от чего она зависит. Порогов времени нет.

use engine::core::footprint::Footprint;
use engine::core::game::Game;
use engine::core::property;
use engine::core::scene;
use engine::core::surface::{self, Lies};
use engine::data::error::LoadFailure;
use engine::data::load::load_game_from_texts_with_terrain;
use engine::render::relief::{self, FlatShapes, SurfaceVertex};
use engine::render::scene3d::{Frame3d, compose_frame3d};
use engine::render::wind::Motion;

const WINDOW: [f32; 2] = [1920.0, 1080.0];

const PROPS: &str = r#"{"properties":{}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;

const GAME: &str = r##"{"name":"T","scene":{"width":20,"height":12,"background":"#000000","view_height":12,"camera":{"pitch":55}},
"random_seed":1,"start_screen":"main","max_objects":300,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},"terrain":"terrain.json"}}"##;

fn load(terrain: &str, objects: &str) -> Game {
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    let loaded = load_game_from_texts_with_terrain(
        GAME,
        PROPS,
        &scene,
        NO_RULES,
        SCREENS,
        None,
        &[],
        Some(terrain),
    );
    match loaded {
        Ok((game, _screens, _warnings)) => game,
        Err(LoadFailure { errors, .. }) => panic!("игра не загрузилась: {errors:#?}"),
    }
}

/// Файл высот сцены 20 × 12 клеток; `height_at(x, y)` считается в каждой точке сетки.
fn terrain_text(water: Option<f64>, height_at: impl Fn(f64, f64) -> f64) -> String {
    let rows: Vec<String> = (0..=24)
        .map(|row| {
            let cells: Vec<String> = (0..=40)
                .map(|column| format!("{}", height_at(column as f64 / 2.0, row as f64 / 2.0)))
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();
    let water = water.map_or_else(String::new, |level| {
        format!(r##""water":{{"level":{level},"color":"#35607a"}},"##)
    });
    format!(r#"{{{water}"heights":[{}]}}"#, rows.join(","))
}

/// Плато высотой 2 в клетках 8–12 по x и 4–8 по y, склон до клетки 6,5 и 13,5, вокруг земля на нуле.
fn hill_terrain() -> String {
    terrain_text(None, |x, y| {
        let d = ((x - 10.0).abs()).max((y - 6.0).abs());
        if d <= 2.0 {
            2.0
        } else if d <= 3.5 {
            2.0 * (3.5 - d) / 1.5
        } else {
            0.0
        }
    })
}

/// Берег: слева вода глубже уровня, справа ровная земля на высоте 1; обрыв между точками x = 9,5 и 10.
fn shore_terrain() -> String {
    terrain_text(Some(-1.0), |x, _| if x < 10.0 { -2.0 } else { 1.0 })
}

const BRIDGE: &str = r##"{"name":"bridge","position":[8,5],"size":[4,2],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
const CRATE: &str = r##"{"name":"crate","position":[1,1],"size":[1,1],"shape":"box","height":1,"color":"#888888"}"##;

fn crate_at(name: &str, x: f64, y: f64) -> String {
    CRATE
        .replace("crate", name)
        .replace("[1,1],\"size\"", &format!("[{x},{y}],\"size\""))
}

fn flat(name: &str, x: f64, y: f64, w: f64, h: f64) -> String {
    format!(r##"{{"name":"{name}","position":[{x},{y}],"size":[{w},{h}],"color":"#aa8844"}}"##)
}

fn frame(game: &Game, shapes: &mut FlatShapes) -> Frame3d {
    let camera = game.camera_3d(WINDOW).expect("камера");
    compose_frame3d(game, &camera, &Motion::default(), &[], &[], shapes)
}

/// Вершины кадра, что лежат над прямоугольником объекта `id` без поворота: плоские объекты теста не
/// пересекаются, а плиток земли в сцене нет.
fn object_vertices(frame: &Frame3d, game: &Game, id: u32) -> Vec<SurfaceVertex> {
    let [x, y] = game.world.vec2(id, property::POSITION).expect("место");
    let [w, h] = game.world.vec2(id, property::SIZE).expect("размер");
    let inside =
        |value: f32, low: f64, high: f64| (low - 1e-4..=high + 1e-4).contains(&f64::from(value));
    frame
        .surface
        .iter()
        .copied()
        .filter(|v| inside(v.position[0], x, x + w) && inside(v.position[1], y, y + h))
        .collect()
}

/// Настилы так, как их находил перебор всех объектов мира.
fn decks_by_scanning(game: &Game) -> Vec<(u32, Footprint, f64)> {
    game.world
        .ids()
        .filter(|&id| game.world.flag(id, property::DECK))
        .filter_map(|id| {
            Some((
                id,
                scene::ground_footprint(&game.world, id)?,
                surface::deck_top(&game.world, id),
            ))
        })
        .collect()
}

fn decks_listed(game: &Game) -> Vec<(u32, Footprint, f64)> {
    surface::decks(&game.world).collect()
}

#[test]
fn the_deck_list_follows_creation_deletion_deck_writes_and_moves() {
    let objects = format!(
        "{},{BRIDGE},{},{}",
        crate_at("first", 1.0, 1.0),
        crate_at("second", 3.0, 1.0),
        BRIDGE.replace("bridge", "pier").replace("[8,5]", "[2,8]")
    );
    let mut game = load(&hill_terrain(), &objects);
    assert_eq!(game.world.deck_ids(), &[1, 3]);
    assert_eq!(decks_listed(&game), decks_by_scanning(&game));

    game.world.delete(1);
    assert_eq!(
        game.world.deck_ids(),
        &[3],
        "удалённый настил ушёл из списка"
    );

    game.world.set_flag(0, property::DECK, true);
    assert_eq!(
        game.world.deck_ids(),
        &[0, 3],
        "объект получил deck — в списке по возрастанию"
    );
    game.world.set_flag(0, property::DECK, true);
    assert_eq!(
        game.world.deck_ids(),
        &[0, 3],
        "повторная запись список не множит"
    );
    game.world.set_flag(0, property::DECK, false);
    assert_eq!(game.world.deck_ids(), &[3], "deck снят");

    game.world.clear_property(3, property::DECK);
    assert!(game.world.deck_ids().is_empty(), "deck очищен кодом");

    let fresh = game.world.create();
    assert_eq!(fresh, 1, "освободившийся номер занят заново");
    assert!(game.world.deck_ids().is_empty(), "новый объект не настил");
    game.world.set_flag(fresh, property::DECK, true);
    assert_eq!(game.world.deck_ids(), &[1]);
    assert_eq!(decks_listed(&game), decks_by_scanning(&game));
}

#[test]
fn a_flat_object_lies_on_the_top_of_a_deck_under_it_and_on_the_terrain_beside_it() {
    let objects = format!(
        "{BRIDGE},{},{},{}",
        flat("on", 9.0, 5.5, 1.0, 0.5),
        flat("beside", 4.0, 5.5, 1.0, 0.5),
        flat("edge", 12.0, 5.5, 1.0, 0.5),
    );
    let game = load(&terrain_text(None, |_, _| 0.0), &objects);
    let lies = |id| surface::lies_on(&game.world, id).expect("место есть");
    assert_eq!(lies(1), Lies::Deck { top: 0.3 });
    assert!(matches!(lies(2), Lies::Terrain { .. }));
    assert!(
        matches!(lies(3), Lies::Terrain { .. }),
        "касание краем — не настил"
    );
    let floor = surface::rest_height(
        &game.world,
        Some(1),
        &scene::ground_footprint(&game.world, 1).expect("место"),
        None,
    );
    assert_eq!(floor, 0.3, "поиск посадки идёт по тому же списку");
}

#[test]
fn the_shape_of_a_flat_object_is_not_rebuilt_between_frames_but_after_a_move() {
    let objects = format!(
        "{},{}",
        flat("trail", 6.6, 5.0, 1.0, 0.6),
        flat("plain", 1.0, 1.0, 2.0, 1.0)
    );
    let mut game = load(&hill_terrain(), &objects);
    let mut shapes = FlatShapes::default();
    let first = frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 2, "по форме на объект");
    let again = frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 2, "без изменений форма не считается");
    assert_eq!(first, again);

    game.world.set_vec2(0, property::POSITION, [7.6, 5.0]);
    let moved = frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 3, "сдвинут один — пересчитан один");
    assert_ne!(first.surface, moved.surface);
    assert_eq!(
        frame(&game, &mut FlatShapes::default()),
        moved,
        "кадр с готовыми формами равен кадру с нуля"
    );
}

#[test]
fn the_shape_follows_size_rotation_and_base_height() {
    let mut game = load(&hill_terrain(), &flat("trail", 6.6, 5.0, 1.0, 0.6));
    let mut shapes = FlatShapes::default();
    frame(&game, &mut shapes);
    game.world.set_vec2(0, property::SIZE, [1.5, 0.6]);
    frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 2, "размер");
    game.world.set_rotation(
        0,
        property::ROTATION,
        engine::core::value::Rotation::from_degrees(30.0).expect("угол"),
    );
    frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 3, "поворот");
    game.world.set_base_z(0, 1.0);
    frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 4, "высота основания");
}

#[test]
fn a_terrain_edit_and_a_removed_deck_rebuild_the_shape_in_the_next_frame() {
    let objects = format!("{BRIDGE},{}", flat("mat", 9.0, 5.5, 1.0, 0.5));
    let mut game = load(&hill_terrain(), &objects);
    let mut shapes = FlatShapes::default();
    let on_deck = frame(&game, &mut shapes);
    let top = surface::deck_top(&game.world, 0);
    assert!(
        object_vertices(&on_deck, &game, 1)
            .iter()
            .all(|v| (f64::from(v.position[2]) - top).abs() < 1e-5),
        "лежит на верху настила"
    );

    game.world.delete(0);
    let after_delete = frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 2, "настил удалён — форма пересчитана");
    assert!(
        object_vertices(&after_delete, &game, 1)
            .iter()
            .all(|v| f64::from(v.position[2]) < top),
        "лежит на рельефе"
    );

    let before = shapes.builds();
    game.world.replace_terrain(None);
    let flat_land = frame(&game, &mut shapes);
    assert_eq!(
        shapes.builds(),
        before + 1,
        "рельеф сменился — форма пересчитана"
    );
    assert!(
        object_vertices(&flat_land, &game, 1)
            .iter()
            .all(|v| v.position[2] == 0.0),
        "лежит по новой земле"
    );
}

#[test]
fn a_moved_deck_rebuilds_the_shapes_but_a_moved_ordinary_object_does_not() {
    let objects = format!(
        "{BRIDGE},{},{}",
        flat("mat", 9.0, 5.5, 1.0, 0.5),
        crate_at("box", 1.0, 1.0)
    );
    let mut game = load(&hill_terrain(), &objects);
    let mut shapes = FlatShapes::default();
    frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 1);

    game.world.set_vec2(2, property::POSITION, [3.0, 3.0]);
    frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 1, "ящик не настил — форма цела");

    game.world.set_vec2(0, property::POSITION, [8.0, 8.0]);
    let moved = frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 2, "настил ушёл из-под объекта");
    assert!(
        object_vertices(&moved, &game, 1)
            .iter()
            .all(|v| f64::from(v.position[2]) > 1.9),
        "на плато без настила"
    );
}

#[test]
fn a_change_of_color_and_opacity_changes_the_paint_but_not_the_shape() {
    let mut game = load(&hill_terrain(), &flat("trail", 6.6, 5.0, 1.0, 0.6));
    let mut shapes = FlatShapes::default();
    let before = frame(&game, &mut shapes);
    game.world
        .set_color(0, property::COLOR, [0.1, 0.2, 0.3, 1.0]);
    game.world.set_number(0, property::OPACITY, 0.5);
    let after = frame(&game, &mut shapes);
    assert_eq!(shapes.builds(), 1, "форма не пересчитана");
    let (old, new) = (
        object_vertices(&before, &game, 0),
        object_vertices(&after, &game, 0),
    );
    assert_eq!(old.len(), new.len());
    assert_ne!(old[0].color, new[0].color, "цвет сменился");
    for (a, b) in old.iter().zip(&new) {
        assert_eq!((a.position, a.normal, a.uv), (b.position, b.normal, b.uv));
    }
}

#[test]
fn level_ground_under_an_object_gives_one_rectangle_even_beside_a_slope_and_water() {
    let objects = [
        flat("plateau", 9.0, 5.0, 1.0, 1.0),
        flat("plain", 1.0, 1.0, 2.0, 1.0),
        flat("slope", 6.6, 5.0, 1.0, 0.6),
    ]
    .join(",");
    let game = load(&hill_terrain(), &objects);
    let frame_now = frame(&game, &mut FlatShapes::default());
    let plateau = object_vertices(&frame_now, &game, 0);
    let plain = object_vertices(&frame_now, &game, 1);
    let sloped = object_vertices(&frame_now, &game, 2);
    assert_eq!(plateau.len(), 6, "плато — один прямоугольник");
    assert_eq!(plain.len(), 6, "ровная земля — один прямоугольник");
    assert!(plateau.iter().all(|v| v.position[2] == 2.0));
    assert!(plain.iter().all(|v| v.position[2] == 0.0));
    assert!(sloped.len() > 6, "склон режется по треугольникам");

    let shore = load(
        &shore_terrain(),
        &[
            flat("land", 14.0, 3.0, 3.0, 2.0),
            flat("across", 9.0, 3.0, 2.0, 2.0),
        ]
        .join(","),
    );
    let frame_now = frame(&shore, &mut FlatShapes::default());
    let land = object_vertices(&frame_now, &shore, 0);
    assert_eq!(land.len(), 6, "один прямоугольник");
    assert!(
        land.iter().all(|v| v.position[2] == 1.0),
        "земля у воды ровная"
    );
    assert!(
        object_vertices(&frame_now, &shore, 1).len() > 6,
        "через обрыв режется"
    );
}

#[test]
fn a_changed_picture_frame_moves_the_place_on_the_atlas_but_not_the_shape() {
    let game = load(&hill_terrain(), &flat("trail", 6.6, 5.0, 1.0, 0.6));
    let mut shapes = FlatShapes::default();
    let first = frame(&game, &mut shapes);
    let mut rects = first.ground.clone();
    rects[0].atlas_rect.x += 16;
    rects[0].color = [0.5, 0.5, 0.5, 1.0];
    let again = relief::surface_triangles(&game.world, &rects, &mut shapes);
    assert_eq!(shapes.builds(), 1, "форма не пересчитана");
    assert_eq!(again.len(), first.surface.len());
    for (old, new) in first.surface.iter().zip(&again) {
        assert_eq!(old.position, new.position);
        assert_eq!(old.normal, new.normal);
        assert_eq!(new.uv[0], old.uv[0] + 16.0, "место на картинке сдвинулось");
        assert_ne!(old.color, new.color);
    }
}

#[test]
fn a_rectangle_on_level_ground_takes_the_light_of_the_terrain_under_it() {
    let game = load(&hill_terrain(), &flat("edge", 11.5, 5.0, 0.5, 1.0));
    let frame_now = frame(&game, &mut FlatShapes::default());
    let edge = object_vertices(&frame_now, &game, 0);
    assert!(
        edge.len() > 6,
        "у края плато свет разный — режется по треугольникам"
    );
    let mut tilted = false;
    for vertex in &edge {
        let normal = game
            .world
            .terrain()
            .normal_at(f64::from(vertex.position[0]), f64::from(vertex.position[1]));
        for axis in 0..3 {
            assert!(
                (f64::from(vertex.normal[axis]) - normal[axis]).abs() < 1e-5,
                "нормаль вершины {:?} — не нормаль земли {normal:?}",
                vertex.position
            );
        }
        tilted |= vertex.normal[2] < 0.9999;
    }
    assert!(tilted, "у края плато свет земли не строго сверху");
}

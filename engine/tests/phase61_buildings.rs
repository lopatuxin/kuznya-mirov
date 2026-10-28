//! Фаза 61, шаг 1 — здания из объёма и срез: ключи моделей `solid` и `cutaway`, узлы `inside`, препятствия и пол из
//! геометрии твёрдой модели для `move`, `shift`, ходьбы и `follow_mouse`, срез с растворением. Модели собираются в коде
//! теста из коробок и пишутся в `.glb` (оси файла: Y вверх, Z к игроку), игры — как в `phase57_painted_world.rs`.

use engine::core::building::DISSOLVE_STEPS;
use engine::core::camera::Camera3d;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::model::Model;
use engine::core::property;
use engine::core::scene::pointer_hit;
use engine::core::surface;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ModelFile, load_rest_with_models, read_entry};
use engine::render::models::model_draws;
use serde_json::json;

const PROPS: &str = r#"{"properties":{"hero":"flag","walker":"flag"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const MOVE: &str = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
const WALK: &str = r#"{"rules":[{"kind":"walk","for":{"has":["walker"]}}]}"#;

// -------------------------------------------------------------------------------------------
// Модели
// -------------------------------------------------------------------------------------------

/// Коробка в осях сцены: x вправо, y к игроку, z вверх.
struct Block {
    name: &'static str,
    low: [f32; 3],
    high: [f32; 3],
    /// Поворот узла вокруг вертикали, градусы.
    turn: f32,
}

const fn block(name: &'static str, low: [f32; 3], high: [f32; 3]) -> Block {
    Block {
        name,
        low,
        high,
        turn: 0.0,
    }
}

impl Block {
    const fn turned(mut self, degrees: f32) -> Block {
        self.turn = degrees;
        self
    }
}

/// Грани коробки `[low, high]` в осях файла: четыре угла и наружная нормаль каждой.
fn faces(low: [f32; 3], high: [f32; 3]) -> Vec<([[f32; 3]; 4], [f32; 3])> {
    let mut out = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let at = if sign > 0.0 { high[axis] } else { low[axis] };
            let corner = |a: f32, b: f32| {
                let mut p = [0.0; 3];
                p[axis] = at;
                p[u] = a;
                p[v] = b;
                p
            };
            let mut quad = [
                corner(low[u], low[v]),
                corner(high[u], low[v]),
                corner(high[u], high[v]),
                corner(low[u], high[v]),
            ];
            let (a, b, c) = (quad[0], quad[1], quad[2]);
            let (e, f) = (
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            let cross = [
                e[1] * f[2] - e[2] * f[1],
                e[2] * f[0] - e[0] * f[2],
                e[0] * f[1] - e[1] * f[0],
            ];
            if cross[axis] * sign < 0.0 {
                quad.reverse();
            }
            let mut normal = [0.0; 3];
            normal[axis] = sign;
            out.push((quad, normal));
        }
    }
    out
}

/// `.glb` из коробок, по узлу-сетке на каждую; имя узла — имя коробки.
fn glb(blocks: &[Block]) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut meshes, mut nodes) = (vec![], vec![], vec![], vec![]);
    for (at, block) in blocks.iter().enumerate() {
        let to_file = |p: [f32; 3]| [p[0], p[2], p[1]];
        let (low, high) = (to_file(block.low), to_file(block.high));
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        for (quad, normal) in faces(low, high) {
            let base = positions.len() as u16 / 3;
            for corner in quad {
                positions.extend(corner);
                normals.extend(normal);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let mut push = |bytes: Vec<u8>| {
            let view = json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len()});
            bin.extend(bytes);
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            views.push(view);
            views.len() - 1
        };
        let floats = |values: &[f32]| values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let position_view = push(floats(&positions));
        let normal_view = push(floats(&normals));
        let index_view = push(indices.iter().flat_map(|i| i.to_le_bytes()).collect());
        let first = accessors.len();
        accessors.push(
            json!({"bufferView": position_view, "componentType": 5126, "count": 24,
            "type": "VEC3", "min": low, "max": high}),
        );
        accessors.push(
            json!({"bufferView": normal_view, "componentType": 5126, "count": 24, "type": "VEC3"}),
        );
        accessors.push(
            json!({"bufferView": index_view, "componentType": 5123, "count": 36, "type": "SCALAR"}),
        );
        meshes.push(
            json!({"primitives": [{"attributes": {"POSITION": first, "NORMAL": first + 1},
            "indices": first + 2}]}),
        );
        let mut node = json!({"name": block.name, "mesh": at});
        if block.turn != 0.0 {
            let half = block.turn.to_radians() / 2.0;
            node["rotation"] = json!([0.0, half.sin(), 0.0, half.cos()]);
        }
        nodes.push(node);
    }
    let root: Vec<usize> = (0..nodes.len()).collect();
    let document = json!({"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": root}],
        "nodes": nodes, "meshes": meshes, "accessors": accessors, "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}]});
    let mut text = document.to_string().into_bytes();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((text.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(text);
    file.extend((bin.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(bin);
    file
}

/// Зал 6 × 6 × 3,3: пол с верхом 0,3, стены от пола до 3, дверь шириной 1 в южной стене, стол, крыша и коробка
/// `inside_room` над полом.
fn hall(with_roof_and_south_wall: bool) -> Vec<u8> {
    let mut blocks = vec![
        block("floor", [-3.0, -3.0, 0.0], [3.0, 3.0, 0.3]),
        block("west", [-3.0, -3.0, 0.3], [-2.8, 3.0, 3.0]),
        block("east", [2.8, -3.0, 0.3], [3.0, 3.0, 3.0]),
        block("north", [-2.8, -3.0, 0.3], [2.8, -2.8, 3.0]),
        block("table", [-0.5, -1.5, 0.3], [0.5, -0.5, 1.1]),
    ];
    if with_roof_and_south_wall {
        blocks.extend([
            block("south_left", [-2.8, 2.8, 0.3], [-0.5, 3.0, 3.0]),
            block("south_right", [0.5, 2.8, 0.3], [2.8, 3.0, 3.0]),
            block("roof", [-3.0, -3.0, 3.0], [3.0, 3.0, 3.3]),
            block("inside_room", [-2.8, -2.8, 0.3], [2.8, 2.8, 3.0]),
        ]);
    }
    glb(&blocks)
}

fn slab(height: f32) -> Vec<u8> {
    glb(&[block("slab", [-1.0, -1.0, 0.0], [1.0, 1.0, height])])
}

// -------------------------------------------------------------------------------------------
// Игры
// -------------------------------------------------------------------------------------------

fn describe(failure: &LoadFailure) -> Vec<String> {
    failure.errors.iter().map(error_text).collect()
}

fn error_text(error: &GameError) -> String {
    format!("{} → {}: {}", error.file, error.path, error.message)
}

fn page_file(bytes: Vec<u8>) -> ModelFile {
    ModelFile {
        bytes: Some(bytes),
        images: Vec::new(),
    }
}

const MODELS: &str = r#"{
  "hall": {"path": "models/hall.glb", "solid": true, "cutaway": "hall_cut"},
  "hall_cut": {"path": "models/hall_cut.glb"},
  "low": {"path": "models/low.glb", "solid": true},
  "high": {"path": "models/high.glb", "solid": true}
}"#;

fn standard_files() -> Vec<(String, ModelFile)> {
    [
        ("hall", hall(true)),
        ("hall_cut", hall(false)),
        ("low", slab(0.3)),
        ("high", slab(0.5)),
    ]
    .into_iter()
    .map(|(name, bytes)| (name.to_string(), page_file(bytes)))
    .collect()
}

struct Setup<'a> {
    camera: &'a str,
    models: &'a str,
    files: Vec<(String, ModelFile)>,
    objects: &'a str,
    rules: &'a str,
}

impl<'a> Setup<'a> {
    fn new(objects: &'a str, rules: &'a str) -> Setup<'a> {
        Setup {
            camera: r#","view_height":12,"camera":{"pitch":30,"projection":"orthographic"}"#,
            models: MODELS,
            files: standard_files(),
            objects,
            rules,
        }
    }

    fn load(self) -> Result<(Game, Vec<String>), Vec<String>> {
        let text = format!(
            r##"{{"name":"B","scene":{{"width":20,"height":20,"background":"#000000"{}}},
"random_seed":1,"max_objects":64,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"models":{}}}}}"##,
            self.camera, self.models
        );
        let scene = format!(r#"{{"objects":[{}]}}"#, self.objects);
        let (config, _) = read_entry(&text).map_err(|f| describe(&f))?;
        load_rest_with_models(
            &text,
            config,
            Some(PROPS),
            Some(&scene),
            Some(self.rules),
            Some(SCREENS),
            &[],
            &[],
            &[],
            &[],
            None,
            false,
            &[],
            None,
            &[],
            &[],
            &[],
            &self.files,
        )
        .map(|(game, _, warnings, _)| (game, warnings.iter().map(error_text).collect()))
        .map_err(|failure| describe(&failure))
    }
}

fn build(objects: &str, rules: &str) -> Game {
    Setup::new(objects, rules)
        .load()
        .unwrap_or_else(|errors| panic!("игра не загрузилась: {errors:#?}"))
        .0
}

/// Игра со своими моделями: `models` — объявление, `files` — имя модели и её `.glb`.
fn build_with(objects: &str, rules: &str, models: &str, files: Vec<(&str, Vec<u8>)>) -> Game {
    let mut setup = Setup::new(objects, rules);
    setup.models = models;
    setup.files = files
        .into_iter()
        .map(|(name, bytes)| (name.to_string(), page_file(bytes)))
        .collect();
    setup
        .load()
        .unwrap_or_else(|errors| panic!("игра не загрузилась: {errors:#?}"))
        .0
}

fn expect_error(result: Result<(Game, Vec<String>), Vec<String>>, path: &str, fragment: &str) {
    let Err(errors) = result else {
        panic!("игра должна была не загрузиться");
    };
    assert!(
        errors
            .iter()
            .any(|e| e.contains(path) && e.contains(fragment)),
        "нет ошибки с местом \"{path}\" и словом \"{fragment}\": {errors:#?}"
    );
}

fn model_of<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель есть")
}

fn step(game: &mut Game, count: u32) {
    for _ in 0..count {
        game.step(StepInput::empty());
    }
}

fn near(actual: f64, want: f64, what: &str) {
    assert!(
        (actual - want).abs() < 1e-6,
        "{what}: {actual} вместо {want}"
    );
}

const HALL: &str = r#"{"name":"hall","model":"hall","position":[5,5],"size":[6,6]}"#;

fn hero(position: &str, velocity: &str, extra: &str) -> String {
    format!(
        r##"{{"name":"hero","position":{position},"size":[0.5,0.5],"shape":"box","height":1.7,"color":"#ff0000","hero":true,"velocity":{velocity},"camera_follows":true{extra}}}"##
    )
}

fn hero_position(game: &Game, id: u32) -> [f64; 2] {
    game.world.vec2(id, property::POSITION).expect("место")
}

fn hero_id(game: &Game) -> u32 {
    let hero = game.properties.resolve("hero").expect("свойство");
    game.world
        .ids()
        .find(|&id| game.world.flag(id, hero))
        .expect("герой")
}

// -------------------------------------------------------------------------------------------
// Разбор: `solid`, `cutaway`
// -------------------------------------------------------------------------------------------

#[test]
fn solid_and_cutaway_are_read_and_a_solid_model_gets_a_column_grid() {
    let (game, _) = Setup::new(HALL, NO_RULES).load().expect("игра");
    let hall = model_of(&game, "hall");
    assert!(hall.building.solid.is_some());
    assert_eq!(
        hall.building.cutaway,
        game.world.models().index_of("hall_cut")
    );
    let cut = model_of(&game, "hall_cut");
    assert!(cut.building.solid.is_none() && cut.building.cutaway.is_none());
    assert!(game.world.models().any_solid() && game.world.models().any_cutaway());
}

#[test]
fn solid_false_and_a_missing_key_make_an_ordinary_model() {
    let mut setup = Setup::new(HALL, NO_RULES);
    setup.models = r#"{"hall":{"path":"models/hall.glb","solid":false},"hall_cut":{"path":"models/hall_cut.glb"},
        "low":{"path":"models/low.glb"},"high":{"path":"models/high.glb"}}"#;
    let (game, _) = setup.load().expect("игра");
    assert!(!game.world.models().any_solid());
}

#[test]
fn a_solid_that_is_not_true_or_false_is_an_error_with_its_own_place() {
    for value in ["\"yes\"", "1", "null", "[true]"] {
        let mut setup = Setup::new(HALL, NO_RULES);
        let models = format!(
            r#"{{"hall":{{"path":"models/hall.glb","solid":{value}}},"hall_cut":{{"path":"models/hall_cut.glb"}},
            "low":{{"path":"models/low.glb"}},"high":{{"path":"models/high.glb"}}}}"#
        );
        setup.models = &models;
        expect_error(setup.load(), "files → models → hall → solid", "solid");
    }
}

#[test]
fn a_cutaway_without_solid_or_unknown_or_itself_is_an_error() {
    let case = |hall: &str, fragment: &str| {
        let mut setup = Setup::new(HALL, NO_RULES);
        let models = format!(
            r#"{{"hall":{{"path":"models/hall.glb"{hall}}},"hall_cut":{{"path":"models/hall_cut.glb"}},
            "low":{{"path":"models/low.glb"}},"high":{{"path":"models/high.glb"}}}}"#
        );
        setup.models = &models;
        expect_error(setup.load(), "files → models → hall → cutaway", fragment);
    };
    case(r#","cutaway":"hall_cut""#, "без solid");
    case(r#","solid":false,"cutaway":"hall_cut""#, "без solid");
    case(r#","solid":true,"cutaway":"nowhere""#, "такой модели нет");
    case(r#","solid":true,"cutaway":"hall""#, "саму");
    case(r#","solid":true,"cutaway":3"#, "ожидалась строка");
}

#[test]
fn solid_and_cutaway_in_a_flat_scene_are_errors() {
    let mut setup = Setup::new(HALL, NO_RULES);
    setup.camera = "";
    let result = setup.load();
    expect_error(
        match result {
            Ok(_) => panic!("игра должна была не загрузиться"),
            Err(errors) => Err(errors),
        },
        "files → models → hall → solid",
        "трёхмерной сцене",
    );
    let mut setup = Setup::new(HALL, NO_RULES);
    setup.camera = "";
    expect_error(
        setup.load(),
        "files → models → hall → cutaway",
        "трёхмерной сцене",
    );
}

#[test]
fn a_cutaway_model_without_inside_nodes_is_a_warning_and_with_them_is_not() {
    let (_, warnings) = Setup::new(HALL, NO_RULES).load().expect("игра");
    assert!(
        !warnings.iter().any(|w| w.contains("inside")),
        "у зала узлы есть: {warnings:#?}"
    );
    let mut setup = Setup::new(HALL, NO_RULES);
    setup.files = vec![
        ("hall".to_string(), page_file(hall(false))),
        ("hall_cut".to_string(), page_file(hall(false))),
        ("low".to_string(), page_file(slab(0.3))),
        ("high".to_string(), page_file(slab(0.5))),
    ];
    let (_, warnings) = setup.load().expect("игра идёт с предупреждением");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("files → models → hall → cutaway") && w.contains("inside")),
        "{warnings:#?}"
    );
}

// -------------------------------------------------------------------------------------------
// Узлы `inside`
// -------------------------------------------------------------------------------------------

#[test]
fn inside_nodes_are_boxes_that_are_neither_drawn_nor_solid() {
    let game = build(HALL, NO_RULES);
    let hall = model_of(&game, "hall");
    assert_eq!(hall.building.inside.len(), 1);
    let room = hall.building.inside[0];
    for (axis, (low, high)) in [(-2.8, 2.8), (-2.8, 2.8), (0.3, 3.0)]
        .into_iter()
        .enumerate()
    {
        assert!((f64::from(room.low[axis]) - low).abs() < 1e-5, "{room:?}");
        assert!((f64::from(room.high[axis]) - high).abs() < 1e-5, "{room:?}");
    }
    let vertices: usize = hall.parts.iter().map(|part| part.vertices.len()).sum();
    assert_eq!(
        vertices,
        24 * 8,
        "коробка inside не рисуется: восемь других коробок"
    );
    assert!(
        (hall.size[2] - 3.3).abs() < 1e-5 && (hall.size[0] - 6.0).abs() < 1e-5,
        "и размер модели её не учитывает: {:?}",
        hall.size
    );
    let spans = hall
        .building
        .solid
        .as_ref()
        .expect("сетка")
        .spans_at([0.0, 1.5]);
    assert_eq!(
        spans.len(),
        2,
        "в комнате твёрдые только пол и крыша: {spans:?}"
    );
    assert!((spans[0].high - 0.3).abs() < 1e-5 && (spans[1].low - 3.0).abs() < 1e-5);
}

// -------------------------------------------------------------------------------------------
// Препятствия, скольжение, ступени
// -------------------------------------------------------------------------------------------

#[test]
fn a_walker_stops_at_a_wall_of_a_solid_model_and_slides_along_it() {
    let objects = format!("{HALL},{}", hero("[2,7]", "[3,0]", ""));
    let mut game = build(&objects, MOVE);
    let id = hero_id(&game);
    step(&mut game, 120);
    let [x, y] = hero_position(&game, id);
    assert!(
        x <= 4.5 + 1e-6 && x > 4.4,
        "упёрся в западную стену (x от 5): {x}"
    );
    near(y, 7.0, "вдоль стены он не шёл");

    let objects = format!("{HALL},{}", hero("[2,7]", "[3,1.5]", ""));
    let mut game = build(&objects, MOVE);
    let id = hero_id(&game);
    step(&mut game, 120);
    let [x, y] = hero_position(&game, id);
    assert!(x <= 4.5 + 1e-6, "{x}");
    assert!(y > 8.5, "по стене он скользит дальше: {y}");
}

#[test]
fn a_walker_enters_the_door_steps_up_to_the_floor_and_is_stopped_by_the_table() {
    // Дверь — зазор x от 7,5 до 8,5 в южной стене (y от 10,8 до 11): идёт снизу вверх по сцене.
    let objects = format!("{HALL},{}", hero("[7.75,12.5]", "[0,-3]", ""));
    let mut game = build(&objects, MOVE);
    let id = hero_id(&game);
    near(game.world.base_z(id), 0.0, "снаружи на земле");
    step(&mut game, 60);
    let [x, y] = hero_position(&game, id);
    near(x, 7.75, "прошёл дверь прямо");
    assert!(y < 10.0, "вошёл в зал: {y}");
    near(game.world.base_z(id), 0.3, "встал на пол зала");
    step(&mut game, 120);
    let [_, y] = hero_position(&game, id);
    // Стол: y от 6,5 до 7,5, x от 7,5 до 8,5; тело 0,5 × 0,5 упирается в его южную грань.
    assert!((7.5 - 1e-6..7.6).contains(&y), "стол не пускает: {y}");
}

#[test]
fn a_step_of_a_third_is_climbed_and_a_step_of_a_half_is_not() {
    let climb = |model: &str, want_z: f64, reach: bool| {
        let step_object =
            format!(r#"{{"name":"step","model":"{model}","position":[8,5],"size":[2,2]}}"#);
        let objects = format!("{step_object},{}", hero("[5,5.75]", "[3,0]", ""));
        let mut game = build(&objects, MOVE);
        let id = hero_id(&game);
        step(&mut game, 75);
        let [x, _] = hero_position(&game, id);
        near(game.world.base_z(id), want_z, model);
        assert_eq!(x > 8.5, reach, "{model}: x = {x}");
    };
    climb("low", 0.3, true);
    climb("high", 0.0, false);
}

#[test]
fn an_object_without_z_stands_on_the_highest_floor_of_a_solid_model_and_z_in_the_data_is_exact() {
    let seated = |position: &str| {
        let game = build(&format!("{HALL},{}", hero(position, "[0,0]", "")), NO_RULES);
        game.world.base_z(hero_id(&game))
    };
    near(
        seated("[7.75,8.5]"),
        3.3,
        "без z — на самый высокий пол, крышу",
    );
    near(seated("[7.75,8.5,0.3]"), 0.3, "z в данных ставит ровно");
    near(seated("[1,1]"), 0.0, "вне зала — на землю");
}

#[test]
fn a_move_inside_the_room_keeps_the_floor_and_the_roof_is_too_high_to_seat() {
    let objects = format!("{HALL},{}", hero("[6,8.5,0.3]", "[0.6,0]", ""));
    let mut game = build(&objects, MOVE);
    let id = hero_id(&game);
    step(&mut game, 30);
    near(game.world.base_z(id), 0.3, "в комнате остался на полу");
}

#[test]
fn a_solid_model_without_z_stands_on_the_terrain_and_never_on_a_floor() {
    let objects = format!(
        "{HALL},{}",
        r#"{"name":"hall2","model":"hall","position":[5,5],"size":[6,6]}"#
    );
    let game = build(&objects, NO_RULES);
    for id in game.world.ids() {
        near(game.world.base_z(id), 0.0, "твёрдая модель на рельефе");
    }
    let mut game = game;
    game.world.set_vec2(0, property::POSITION, [5.0, 5.0]);
    game.world.set_vec2(0, property::POSITION, [5.5, 5.0]);
    near(game.world.base_z(0), 0.0, "сдвинутая тоже");
}

#[test]
fn an_object_placed_inside_a_part_leaves_by_the_shortest_way_on_its_first_move() {
    // Западная стена — x от 5 до 5,2; центр героя 0,3 от её западной грани и 0,4 до восточной (с телом 0,5).
    let objects = format!("{HALL},{}", hero("[4.8,7.75,0]", "[0,0.6]", ""));
    let mut game = build(&objects, MOVE);
    let id = hero_id(&game);
    let before = hero_position(&game, id);
    step(&mut game, 1);
    let [x, y] = hero_position(&game, id);
    assert!(
        x <= 4.5 + 1e-6 && x > 4.4,
        "вышел на запад, к ближней грани: {x}"
    );
    assert!(
        (y - before[1]).abs() < 0.3,
        "и не ушёл вдоль стены далеко: {y}"
    );
    step(&mut game, 5);
    let [x_later, _] = hero_position(&game, id);
    assert!(x_later <= 4.5 + 1e-6, "и не вернулся в стену: {x_later}");
}

#[test]
fn a_shift_slides_along_a_wall_like_a_move() {
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["hero"]},
        "do":[["shift",{"group":{"has":["hero"]},"by":[0.2,0.1]}]]}]}"#;
    let objects = format!("{HALL},{}", hero("[4,7]", "[0,0]", ""));
    let mut game = build(&objects, rules);
    let id = hero_id(&game);
    step(&mut game, 20);
    let [x, y] = hero_position(&game, id);
    assert!(x <= 4.5 + 1e-6, "{x}");
    assert!(y > 8.0, "вдоль стены он ушёл: {y}");
}

#[test]
fn a_walking_object_does_not_enter_a_solid_part() {
    let objects = format!(
        "{HALL},{}",
        r##"{"name":"w","position":[2,7],"size":[0.5,0.5],"shape":"box","height":1.7,"color":"#00ff00","walker":true,"walk_to":[9,7],"walk_speed":4}"##
    );
    let mut game = build(&objects, WALK);
    let w = game.world.ids().find(|&id| id != 0).expect("идущий");
    step(&mut game, 200);
    let [x, _] = hero_position(&game, w);
    assert!(x <= 4.5 + 1e-6, "идущий упёрся в стену: {x}");
    assert!(
        game.world.vec2(w, property::WALK_TO).is_some(),
        "и не дошёл: цель осталась"
    );
}

#[test]
fn surface_rest_height_counts_solid_floors_with_decks() {
    let objects = format!(
        "{HALL},{}",
        r##"{"name":"bench","position":[7,6.2,0.55],"size":[2,0.5],"shape":"box","height":0.1,"color":"#888888","deck":true}"##
    );
    let game = build(&objects, NO_RULES);
    let place = engine::core::footprint::Footprint::flat([7.5, 8.0], [0.5, 0.5]);
    near(
        surface::rest_height(&game.world, None, &place, Some(0.3)),
        0.3,
        "с пола зала: пол и настил (0,65) выше ступеньки — встаёт на пол",
    );
    near(
        surface::rest_height(&game.world, None, &place, Some(0.0)),
        0.3,
        "с земли: пол зала в пределах ступеньки",
    );
    near(
        surface::rest_height(&game.world, None, &place, None),
        3.3,
        "без прежнего основания — на самый высокий",
    );
    near(
        game.world.base_z(0),
        0.0,
        "сам зал настил внутри не поднимает: твёрдая модель стоит на рельефе",
    );
}

/// Стена 2 × 0,2 × 2 вдоль своей оси `x`.
const WALL_MODELS: &str = r#"{"wall": {"path": "models/wall.glb", "solid": true}}"#;

fn wall_game(objects: &str) -> Game {
    let wall = glb(&[block("wall", [-1.0, -0.1, 0.0], [1.0, 0.1, 2.0])]);
    build_with(objects, MOVE, WALL_MODELS, vec![("wall", wall)])
}

#[test]
fn a_solid_model_blocks_where_it_is_drawn_when_it_faces_aside() {
    // `facing` 90 кладёт стену вдоль мировой `y`: тонкая сторона смотрит на идущего по `x`.
    let wall = r#"{"name":"wall","model":"wall","position":[8,8],"size":[2,2],"facing":90}"#;
    let mut game = wall_game(&format!("{wall},{}", hero("[5.75,8.75]", "[3,0]", "")));
    let id = hero_id(&game);
    step(&mut game, 120);
    let [x, _] = hero_position(&game, id);
    assert!(
        (8.38..=8.4 + 1e-6).contains(&x),
        "упёрся в тонкую грань стены (x от 8,9 до 9,1): {x}"
    );
}

#[test]
fn a_solid_model_turned_and_scaled_blocks_where_it_is_drawn() {
    // Файл высотой 2, объект `height` 4: увеличение 2, стена 4 × 0,4. Поворот 30°: её тонкая ось — по (-0,5; 0,866).
    let wall =
        r#"{"name":"wall","model":"wall","position":[8,8],"size":[4,4],"rotation":30,"height":4}"#;
    let normal = [-0.5, 3f64.sqrt() / 2.0];
    let centre = [10.0 - 3.0 * normal[0], 10.0 - 3.0 * normal[1]];
    let hero = hero(
        &format!("[{},{}]", centre[0] - 0.25, centre[1] - 0.25),
        &format!("[{},{}]", 3.0 * normal[0], 3.0 * normal[1]),
        "",
    );
    let mut game = wall_game(&format!("{wall},{hero}"));
    let id = hero_id(&game);
    step(&mut game, 120);
    let [x, y] = hero_position(&game, id);
    let along = (x + 0.25 - 10.0) * normal[0] + (y + 0.25 - 10.0) * normal[1];
    // Грань стены — на 0,2 от её середины; тело 0,5 × 0,5 вдоль нормали занимает 0,25 × (0,5 + 0,866).
    let want = -0.2 - 0.25 * (0.5 + normal[1]);
    assert!(
        (along - want).abs() < 0.02,
        "упёрся в нарисованную грань: {along} вместо {want}"
    );
}

#[test]
fn a_follow_mouse_stopped_by_a_wall_sits_on_the_floor_of_the_stopped_place() {
    let puck = r##"{"name":"puck","position":[6,8.5,0.3],"size":[0.5,0.5],"shape":"box","height":1,"color":"#cc4444","follow_mouse":"xy","hero":true}"##;
    let mut game = build(&format!("{HALL},{puck}"), NO_RULES);
    let id = hero_id(&game);
    // Курсор за северной стеной и высоко над землёй: шайба встанет у стены на пол зала, а не на высоту курсора.
    game.step(StepInput {
        cursor: Some([6.25, 3.0, 5.0]),
        ..StepInput::default()
    });
    let [x, y] = hero_position(&game, id);
    near(x, 6.0, "по x курсор не сдвинул");
    assert!((5.2..5.3).contains(&y), "упёрлась в северную стену: {y}");
    near(game.world.base_z(id), 0.3, "на полу зала");

    game.step(StepInput {
        cursor: Some([6.25, 8.75, 0.3]),
        ..StepInput::default()
    });
    near(
        game.world.base_z(id),
        0.3,
        "свободный ход — по высоте курсора",
    );
}

#[test]
fn the_pointer_sees_the_floor_of_a_solid_model_like_a_deck_top() {
    let game = build(HALL, NO_RULES);
    let camera = Camera3d::looking_at([8.0, 8.0], 55.0, 12.0, [1920.0, 1080.0]);
    let roof = camera
        .project([8.0, 8.0, 3.3])
        .expect("точка перед камерой");
    let hit = pointer_hit(&game.world, &game.scene, &camera, roof).expect("луч вниз");
    assert!((hit[2] - 3.3).abs() < 1e-3, "луч ловит крышу зала: {hit:?}");
    let ground = camera
        .project([1.0, 1.0, 0.0])
        .expect("точка перед камерой");
    let hit = pointer_hit(&game.world, &game.scene, &camera, ground).expect("луч вниз");
    near(hit[2], 0.0, "вне зала — земля");
}

// -------------------------------------------------------------------------------------------
// Срез
// -------------------------------------------------------------------------------------------

#[test]
fn the_cutaway_share_grows_in_three_tenths_of_a_second_while_the_followed_object_is_inside() {
    assert_eq!(DISSOLVE_STEPS, 18, "0,3 секунды при 60 шагах в секунду");
    let objects = format!("{HALL},{}", hero("[1,1]", "[0,0]", ""));
    let mut game = build(&objects, NO_RULES);
    let id = hero_id(&game);
    let hall = 0;
    step(&mut game, 5);
    assert_eq!(game.world.cutaway_steps(hall), 0, "снаружи среза нет");

    game.world.set_position_exact(id, [7.75, 8.5], 0.3);
    for expected in 1..=DISSOLVE_STEPS {
        step(&mut game, 1);
        assert_eq!(game.world.cutaway_steps(hall), expected);
    }
    step(&mut game, 10);
    assert_eq!(
        game.world.cutaway_steps(hall),
        DISSOLVE_STEPS,
        "срез держится"
    );

    game.world.set_position_exact(id, [1.0, 1.0], 0.0);
    for expected in (0..DISSOLVE_STEPS).rev() {
        step(&mut game, 1);
        assert_eq!(game.world.cutaway_steps(hall), expected);
    }
}

#[test]
fn the_cutaway_waits_for_the_middle_of_the_base_inside_the_box_and_the_floor_height() {
    let objects = format!("{HALL},{}", hero("[1,1]", "[0,0]", ""));
    let mut game = build(&objects, NO_RULES);
    let id = hero_id(&game);
    // Середина основания 7,75 + 0,25 = 8, 8,5 + 0,25: в коробке; на крыше (z 3,3) — над коробкой.
    game.world.set_position_exact(id, [7.75, 8.5], 3.3);
    step(&mut game, 3);
    assert_eq!(game.world.cutaway_steps(0), 0, "над потолком комнаты");
    // Край коробки по x: от 5,2 до 10,8; середина основания 5,1 — ещё в стене.
    game.world.set_position_exact(id, [4.85, 8.5], 0.3);
    step(&mut game, 3);
    assert_eq!(game.world.cutaway_steps(0), 0, "в стене, вне коробки");
    game.world.set_position_exact(id, [5.0, 8.5], 0.3);
    step(&mut game, 3);
    assert_eq!(game.world.cutaway_steps(0), 3, "середина в коробке");
}

#[test]
fn both_models_are_drawn_with_complementary_dissolve_while_the_share_is_between_zero_and_one() {
    let objects = format!("{HALL},{}", hero("[1,1]", "[0,0]", ""));
    let mut game = build(&objects, NO_RULES);
    let id = hero_id(&game);
    let hall = game.world.models().index_of("hall").expect("модель");
    let cut = game.world.models().index_of("hall_cut").expect("модель");

    let draws = model_draws(&game);
    assert_eq!(draws.instances[hall].len(), 1);
    assert_eq!(draws.instances[hall][0].fade, 0.0);
    assert!(draws.dissolving.iter().all(Vec::is_empty), "срез не нужен");

    game.world.set_position_exact(id, [7.75, 8.5], 0.3);
    step(&mut game, 9);
    let draws = model_draws(&game);
    assert!(draws.instances[hall].is_empty() && draws.instances[cut].is_empty());
    let (main, section) = (draws.dissolving[hall][0], draws.dissolving[cut][0]);
    assert!((main.fade - 0.5).abs() < 1e-6, "{}", main.fade);
    assert!((section.fade + 0.5).abs() < 1e-6, "{}", section.fade);
    assert_eq!(main.placement, section.placement, "тот же центр и поворот");
    assert_eq!(main.base, section.base, "та же высота и размер");

    step(&mut game, 9);
    let draws = model_draws(&game);
    assert_eq!(draws.dissolving[hall][0].fade, 1.0);
    assert_eq!(draws.dissolving[cut][0].fade, -1.0);
    assert!(
        draws.top >= 3.3 - 1e-6,
        "карта теней охватывает основную модель"
    );
}

#[test]
fn the_cutaway_model_sits_on_the_building_by_the_shared_file_point_not_by_its_own_box() {
    let mut setup = Setup::new(HALL, NO_RULES);
    // У среза нет южной стены, но и коробка северной стены короче: срез уже здания по плану, середина смещена.
    setup.files = vec![
        ("hall".to_string(), page_file(hall(true))),
        (
            "hall_cut".to_string(),
            page_file(glb(&[block("floor", [-3.0, -3.0, 0.0], [1.0, 3.0, 0.3])])),
        ),
        ("low".to_string(), page_file(slab(0.3))),
        ("high".to_string(), page_file(slab(0.5))),
    ];
    let (mut game, _) = setup.load().expect("игра");
    let hall = game.world.models().index_of("hall").expect("модель");
    let cut = game.world.models().index_of("hall_cut").expect("модель");
    game.world.set_cutaway_steps(0, DISSOLVE_STEPS);
    let draws = model_draws(&game);
    let main = draws.dissolving[hall][0];
    let section = draws.dissolving[cut][0];
    near(
        f64::from(section.placement[0] - main.placement[0]),
        -1.0,
        "срез смещён на разницу середин коробок (середина пола среза — x = -1)",
    );
}

#[test]
fn an_inside_box_keeps_the_orientation_of_its_node() {
    // Коробка 2 × 1, повёрнутая узлом на 45°: её длинная ось идёт по (1, -1) плана, габарит по осям — квадрат 2,12.
    let turret = glb(&[
        block("floor", [-3.0, -3.0, 0.0], [3.0, 3.0, 0.3]),
        block("inside_room", [-1.0, -0.5, 0.3], [1.0, 0.5, 3.0]).turned(45.0),
    ]);
    let cut = glb(&[block("floor", [-3.0, -3.0, 0.0], [3.0, 3.0, 0.3])]);
    let models = r#"{"turret": {"path": "models/turret.glb", "solid": true, "cutaway": "turret_cut"},
        "turret_cut": {"path": "models/turret_cut.glb"}}"#;
    let place = r#"{"name":"turret","model":"turret","position":[5,5],"size":[6,6]}"#;
    let objects = format!("{place},{}", hero("[1,1]", "[0,0]", ""));
    let mut game = build_with(
        &objects,
        NO_RULES,
        models,
        vec![("turret", turret), ("turret_cut", cut)],
    );
    let room = model_of(&game, "turret").building.inside[0];
    assert!(room.contains([0.0, 0.0, 1.0]), "середина в коробке");
    assert!(room.contains([0.5, -0.5, 1.0]), "на длинной оси");
    assert!(!room.contains([0.5, 0.5, 1.0]), "поперёк неё — вне");
    assert!(
        !room.contains([1.0, 1.0, 1.0]),
        "угол габарита по осям — вне"
    );
    assert!(
        !room.contains([0.9, 0.0, 1.0]),
        "габарит по осям сказал бы «внутри»"
    );
    assert!(!room.contains([0.0, 0.0, 3.0]), "верх не входит");

    let id = hero_id(&game);
    let share_at = |game: &mut Game, centre: [f64; 2]| {
        game.world
            .set_position_exact(id, [8.0 + centre[0] - 0.25, 8.0 + centre[1] - 0.25], 0.3);
        step(game, 3);
        game.world.cutaway_steps(0)
    };
    assert_eq!(
        share_at(&mut game, [1.0, 1.0]),
        0,
        "угол габарита: среза нет"
    );
    assert_eq!(
        share_at(&mut game, [0.9, 0.0]),
        0,
        "и сбоку от узкой стороны"
    );
    assert_eq!(share_at(&mut game, [0.0, 0.0]), 3, "середина: срез растёт");
}

#[test]
fn a_fast_follow_mouse_jump_does_not_pass_through_a_wall() {
    let puck = r##"{"name":"puck","position":[2,7.5],"size":[0.5,0.5],"shape":"box","height":1,"color":"#cc4444","follow_mouse":"xy","hero":true}"##;
    let mut game = build(&format!("{HALL},{puck}"), NO_RULES);
    let id = hero_id(&game);
    game.step(StepInput {
        cursor: Some([8.0, 7.75, 0.0]),
        ..StepInput::default()
    });
    let [x, y] = hero_position(&game, id);
    assert!(
        x <= 4.5 + 1e-6 && x > 4.4,
        "курсор за стеной, шайба у стены: {x}"
    );
    near(y, 7.5, "и по y осталась на своей линии");
    near(game.world.base_z(id), 0.0, "высота — по точке курсора");
}

#[test]
fn a_model_named_by_a_cutaway_is_not_reported_as_unused() {
    let (_, warnings) = Setup::new(HALL, NO_RULES).load().expect("игра");
    assert!(
        !warnings.iter().any(|w| w.contains("\"hall_cut\"")),
        "{warnings:#?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("\"low\"") && w.contains("нигде не названа")),
        "а неназванная по-прежнему замечена: {warnings:#?}"
    );
}

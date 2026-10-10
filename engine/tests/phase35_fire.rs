//! Фаза 35 — огонь: три свойства объекта, их проверка перед запуском, код и правила, `set_property` вне
//! партии, огонь в списке рисования (ореол, пламя, дым и искры того же объекта), разгорание и угасание
//! на часах движения, шейдер пламени. Формулы огня — в `render::fire`'s own `mod tests`; сама
//! видеокарта и браузер — только QA на стенде.

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::scene::{LayerView, object_at};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest_with_stamps, read_entry};
use engine::data::session;
use engine::render::atlas::{
    AtlasImage, AtlasRect, HALO_RECT, RectPaint, WHITE_PIXEL, compose_world_paints, fire_objects,
    pack, particle_emitters, sway_objects,
};
use engine::render::wind::Motion;
use serde_json::json;

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"mark":"flag","hits":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;
const FILES: &str = r#","images":{"plain":{"path":"plain.png"}}"#;

const HEARTH: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.8}]}"#;
const PLAIN: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true}]}"#;

fn game_json(scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

type Loaded = (Game, Vec<GameError>, Vec<ImageDecl>);

fn load_with(
    game: &str,
    scene: &str,
    rules: &str,
    code: Option<&str>,
) -> Result<Loaded, LoadFailure> {
    let (config, _warnings) = read_entry(game).expect("game.json должен разбираться");
    let plain = ImageVerdict::Ok {
        width: 8,
        height: 8,
        pixels: vec![255u8; 8 * 8 * 4],
    };
    load_rest_with_stamps(
        game,
        config,
        Some(PROPS),
        Some(scene),
        Some(rules),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &[("plain".to_string(), plain)],
        code,
        false,
        &[],
        None,
        &[],
        &[],
        &[],
    )
    .map(|(game, _screens, warnings, images)| (game, warnings, images))
}

fn load_flat(scene: &str, rules: &str, code: Option<&str>) -> Result<Loaded, LoadFailure> {
    let files = if code.is_some() {
        format!(r#"{FILES},"code":"code.lua""#)
    } else {
        FILES.to_string()
    };
    load_with(&game_json("", &files), scene, rules, code)
}

fn errors_of(result: Result<Loaded, LoadFailure>) -> Vec<GameError> {
    result.expect_err("загрузка должна провалиться").errors
}

fn assert_error(errors: &[GameError], file: &str, path: &str, message: &str) {
    assert!(
        errors
            .iter()
            .any(|e| e.file == file && e.path.contains(path) && e.message.contains(message)),
        "нет ошибки {file} → {path}: {message}: {errors:?}"
    );
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

// -------------------------------------------------------------------------------------------
// Три свойства: загрузка и проверка перед запуском
// -------------------------------------------------------------------------------------------

#[test]
fn the_three_properties_load_as_numbers_and_a_colour_without_a_warning() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"fire":0.6,"fire_color":"#3399ff",
        "fire_glow":0.25}]}"##;
    let (game, warnings, _images) = load_flat(scene, NO_RULES, None).expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("огн")),
        "{warnings:?}"
    );
    assert_eq!(game.world.number_like(0, property::FIRE), Some(0.6));
    assert_eq!(game.world.number_like(0, property::FIRE_GLOW), Some(0.25));
    assert!(game.world.color(0, property::FIRE_COLOR).is_some());
}

#[test]
fn the_colour_and_the_glow_without_the_fire_are_not_an_error() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"fire_color":"#3399ff",
        "fire_glow":0.25}]}"##;
    load_flat(scene, NO_RULES, None).expect("без fire они ничего не дают, но и не ошибка");
}

#[test]
fn the_edges_of_the_range_are_allowed() {
    for object in [
        r#""fire":0"#,
        r#""fire":1"#,
        r#""fire":0.01,"fire_glow":0"#,
        r#""fire":1,"fire_glow":1"#,
    ] {
        let scene = format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],{object}}}]}}"#);
        load_flat(&scene, NO_RULES, None).unwrap_or_else(|e| panic!("{object}: {e:?}"));
    }
}

#[test]
fn every_check_before_the_start_names_the_property_and_the_place() {
    let cases = [
        (r#""fire":1.5"#, "fire", "fire: нужно от 0 до 1"),
        (r#""fire":-0.1"#, "fire", "fire: нужно от 0 до 1"),
        (
            r#""fire_glow":2"#,
            "fire_glow",
            "fire_glow: нужно от 0 до 1",
        ),
        (
            r#""fire_glow":-1"#,
            "fire_glow",
            "fire_glow: нужно от 0 до 1",
        ),
        (r#""fire":"много""#, "fire", "ожидалось число"),
        (r#""fire_glow":true"#, "fire_glow", "ожидалось число"),
        (
            r#""fire_color":"orange""#,
            "fire_color",
            "цвет должен быть вида",
        ),
        (
            r##""fire_color":"#12""##,
            "fire_color",
            "цвет должен быть вида",
        ),
        (r#""fire_color":5"#, "fire_color", "ожидалась строка"),
    ];
    for (field, path, message) in cases {
        let scene = format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],{field}}}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(&errors, "scene.json", path, message);
    }
}

#[test]
fn the_properties_need_a_position_and_a_size_and_do_not_repeat() {
    for object in [
        r#"{"size":[1,1],"fire":0.5}"#,
        r#"{"position":[1,1],"fire":0.5}"#,
        r##"{"position":[1,1],"fire_color":"#ff0000"}"##,
        r#"{"size":[1,1],"fire_glow":0.5}"#,
    ] {
        let scene = format!(r#"{{"objects":[{object}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(
            &errors,
            "scene.json",
            "",
            "свойства огня разрешены только объекту с position и size",
        );
    }
    let repeated = r##"{"objects":[{"position":[1,1],"size":[1,1],"fire":0.5,"repeat_x":true,
        "color":"#ffffff"}]}"##;
    let errors = errors_of(load_flat(repeated, NO_RULES, None));
    assert_error(
        &errors,
        "scene.json",
        "",
        "свойства огня заданы вместе с repeat_x",
    );

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"fire":0.5}}]}"#;
    let errors = errors_of(load_flat(PLAIN, template, None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "свойства огня разрешены только объекту с position и size",
    );
}

#[test]
fn a_rule_may_light_the_fire_and_a_constant_outside_the_range_is_an_error() {
    let rule = |value: &str| {
        format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["mark"]}},"do":[["set","fire",{value}]]}}]}}"#
        )
    };
    load_flat(PLAIN, &rule("0.8"), None).expect("0,8 годится");
    let errors = errors_of(load_flat(PLAIN, &rule("1.5"), None));
    assert_error(&errors, "rules.json", "", "fire: нужно от 0 до 1");

    let glow =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","fire_glow",-1]]}]}"#;
    let errors = errors_of(load_flat(PLAIN, glow, None));
    assert_error(&errors, "rules.json", "", "fire_glow: нужно от 0 до 1");

    let colour =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","fire_color","red"]]}]}"#;
    let errors = errors_of(load_flat(PLAIN, colour, None));
    assert_error(&errors, "rules.json", "", "цвет должен быть вида");

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"size":[1,1],"fire":3}}]}"#;
    let errors = errors_of(load_flat(PLAIN, template, None));
    assert_error(&errors, "rules.json", "", "fire: нужно от 0 до 1");
}

#[test]
fn the_properties_in_a_three_dimensional_scene_are_errors_everywhere() {
    let flat3d = game_json(CAMERA_3D, "");
    let plain3d = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    for field in [
        r#""fire":0.5"#,
        r##""fire_color":"#ffffff""##,
        r#""fire_glow":0.5"#,
    ] {
        let scene = format!(
            r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff",{field}}}]}}"##
        );
        let errors = errors_of(load_with(&flat3d, &scene, NO_RULES, None));
        assert_error(&errors, "scene.json", "", "только в плоской сцене");
    }
    for (name, value) in [("fire", "0.5"), ("fire_color", r##""#ff0000""##)] {
        let rule = format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["mark"]}},"do":[["set","{name}",{value}]]}}]}}"#
        );
        let errors = errors_of(load_with(&flat3d, plain3d, &rule, None));
        assert_error(
            &errors,
            "rules.json",
            "",
            &format!("{name} есть только в плоской сцене"),
        );
    }
}

// -------------------------------------------------------------------------------------------
// Код игры
// -------------------------------------------------------------------------------------------

const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;

fn run_code(body: &str) -> Game {
    let code = format!("function tick(obj)\n{body}\nend");
    let (mut game, ..) = load_flat(PLAIN, CODE_RULES, Some(&code)).expect("должно загрузиться");
    step(&mut game, 1);
    game
}

#[test]
fn code_reads_and_writes_the_fire_as_numbers_and_a_colour() {
    let game = run_code(
        "  print(obj.fire)\n  obj.fire = 1\n  obj.fire_color = \"#3399ff\"\n  obj.fire_glow = 0.2\n  print(obj.fire)\n  print(obj.fire_color)",
    );
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.messages(),
        ["print: nil", "print: 1.0", "print: #3399ff"]
    );
    assert_eq!(game.world.number_like(0, property::FIRE), Some(1.0));
    assert_eq!(game.world.number_like(0, property::FIRE_GLOW), Some(0.2));
}

#[test]
fn code_writing_a_value_outside_the_range_or_a_bad_colour_is_a_runtime_error() {
    for (body, part) in [
        ("obj.fire = 1.5", "fire должен быть от 0 до 1"),
        ("obj.fire = -0.1", "fire должен быть от 0 до 1"),
        ("obj.fire_glow = 2", "fire_glow должен быть от 0 до 1"),
        ("obj.fire_color = \"orange\"", "цвет должен быть вида"),
        ("obj.fire_color = 5", "ожидалась строка цвета"),
        ("obj.fire = \"много\"", "ожидалось число"),
    ] {
        let game = run_code(body);
        let error = game.code_error().unwrap_or_else(|| panic!("{body}"));
        assert!(error.message.contains(part), "{body}: {error:?}");
        assert_eq!(game.world.number_like(0, property::FIRE), None, "{body}");
    }
}

#[test]
fn code_may_not_light_the_fire_in_a_three_dimensional_scene() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let files = r#","code":"code.lua""#;
    let (mut game, ..) = load_with(
        &game_json(CAMERA_3D, files),
        scene,
        CODE_RULES,
        Some("function tick(obj)\n  obj.fire = 0.5\nend"),
    )
    .expect("должно загрузиться");
    step(&mut game, 1);
    let error = game.code_error().expect("ошибка кода");
    assert!(
        error.message.contains("только в плоской сцене"),
        "{error:?}"
    );
}

// -------------------------------------------------------------------------------------------
// `set_property` и `remove_property` вне партии
// -------------------------------------------------------------------------------------------

fn loaded(scene: &str) -> (Game, Vec<ImageDecl>) {
    let (game, _warnings, images) = load_flat(scene, NO_RULES, None).expect("должно загрузиться");
    (game, images)
}

fn load_message(object: &str) -> String {
    let scene = format!(r#"{{"objects":[{object}]}}"#);
    errors_of(load_flat(&scene, NO_RULES, None))
        .into_iter()
        .next()
        .expect("ошибка загрузки")
        .message
}

#[test]
fn the_properties_are_set_and_taken_off_outside_a_party() {
    let (mut game, images) = loaded(PLAIN);
    for (name, value) in [
        ("fire", json!(0.7)),
        ("fire_color", json!("#3399ff")),
        ("fire_glow", json!(0.1)),
    ] {
        assert_eq!(
            session::set_property(None, &mut game, &images, 0, name, &value),
            Ok(()),
            "{name}"
        );
    }
    assert_eq!(game.world.number_like(0, property::FIRE), Some(0.7));
    for name in ["fire", "fire_color", "fire_glow"] {
        assert_eq!(session::remove_property(None, &mut game, 0, name), Ok(()));
    }
    assert!(!game.world.has(0, property::FIRE));
}

#[test]
fn a_failed_check_outside_a_party_changes_nothing_and_says_why() {
    let (mut game, images) = loaded(HEARTH);
    for (name, value, part) in [
        ("fire", json!(2), "от 0 до 1"),
        ("fire_glow", json!(-1), "от 0 до 1"),
        ("fire_color", json!("red"), "цвет"),
    ] {
        let error =
            session::set_property(None, &mut game, &images, 0, name, &value).expect_err(name);
        assert!(error.contains(part), "{name}: {error}");
    }
    assert_eq!(game.world.number_like(0, property::FIRE), Some(0.8));
    assert_eq!(game.world.number_like(0, property::FIRE_GLOW), None);
}

#[test]
fn the_fire_is_refused_outside_a_party_without_a_position_a_size_or_with_a_repeat() {
    for (object, lacks) in [
        (r#"{"position":[1,1],"mark":true}"#, "size"),
        (r#"{"size":[1,1],"mark":true}"#, "position"),
        (
            r##"{"position":[1,1],"size":[1,1],"repeat_x":true,"color":"#ffffff"}"##,
            "repeat_x",
        ),
    ] {
        let (mut game, images) = loaded(&format!(r#"{{"objects":[{object}]}}"#));
        for (name, value) in [
            ("fire", json!(0.5)),
            ("fire_color", json!("#ff0000")),
            ("fire_glow", json!(0.5)),
        ] {
            let expected = load_message(&object.replace('}', &format!(r#","{name}":{value}}}"#)));
            let error = session::set_property(None, &mut game, &images, 0, name, &value)
                .expect_err(&format!("{name} ({lacks})"));
            assert_eq!(error, expected, "{name} ({lacks})");
            let prop = game.properties.resolve(name).expect("встроенное");
            assert!(!game.world.has(0, prop), "{name}: мир не меняется");
        }
    }
}

#[test]
fn position_size_or_repeat_x_edited_on_a_burning_object_outside_a_party_is_refused() {
    for name in ["position", "size"] {
        let (mut game, _images) = loaded(HEARTH);
        let error = session::remove_property(None, &mut game, 0, name).expect_err(name);
        assert!(error.contains("свойства огня"), "{name}: {error}");
        assert!(game.world.has(0, game.properties.resolve(name).unwrap()));
    }
    let (mut game, images) = loaded(HEARTH);
    let error = session::set_property(None, &mut game, &images, 0, "repeat_x", &json!(true))
        .expect_err("repeat_x");
    assert!(error.contains("repeat_x"), "{error}");
}

// -------------------------------------------------------------------------------------------
// Огонь в мире, в списке рисования и на часах движения
// -------------------------------------------------------------------------------------------

struct Rig {
    game: Game,
    images: Vec<ImageDecl>,
    rects: Vec<AtlasRect>,
    motion: Motion,
}

impl Rig {
    fn new(scene: &str) -> Rig {
        let (game, _warnings, images) =
            load_flat(scene, NO_RULES, None).expect("должно загрузиться");
        let plain = AtlasImage {
            width: 8,
            height: 8,
            pixels: vec![255u8; 8 * 8 * 4],
        };
        let rects = pack(&[plain]).expect("умещается").rects;
        Rig {
            game,
            images,
            rects,
            motion: Motion::default(),
        }
    }

    /// То же, что `wasm::update_motion`.
    fn settle(&mut self) {
        let wind = self.game.wind();
        self.motion
            .update(wind[0], sway_objects(&self.game.world, &self.images));
        self.motion.update_particles(
            self.game.has_world(),
            wind,
            particle_emitters(&self.game.world, &self.images),
        );
        self.motion
            .update_fires(self.game.has_world(), wind, fire_objects(&self.game.world));
    }

    fn frames(&mut self, count: u32, dt: f64) {
        for _ in 0..count {
            self.motion.tick(None, dt);
            self.settle();
        }
    }

    fn paints_in(&self, layers: &LayerView) -> Vec<RectPaint> {
        compose_world_paints(
            &self.game.world,
            &self.game.scene,
            self.game.world.ids(),
            &self.motion,
            &self.images,
            &self.rects,
            layers,
        )
    }

    fn paints(&self) -> Vec<RectPaint> {
        self.paints_in(&LayerView::default())
    }

    fn strength(&self, id: u32) -> Option<f64> {
        self.motion
            .fires()
            .strength(id, self.game.world.generation(id))
    }
}

fn is_halo(paint: &RectPaint) -> bool {
    paint.atlas_rect == HALO_RECT && paint.object.is_none()
}

fn is_flame(paint: &RectPaint) -> bool {
    paint.fire.is_some()
}

#[test]
fn an_object_without_a_picture_or_colour_stands_in_the_draw_list_with_its_halo_and_flame() {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    let paints = rig.paints();
    assert_eq!(paints.len(), 2, "{paints:?}");
    assert!(is_halo(&paints[0]), "снизу ореол");
    assert!(is_flame(&paints[1]), "на нём пламя");
    assert!(paints.iter().all(|paint| paint.glow), "оба светятся");
    assert!(paints.iter().all(|paint| paint.object.is_none()));
}

#[test]
fn the_flame_stands_on_the_bottom_edge_with_the_colour_of_the_fire() {
    let scene =
        r##"{"objects":[{"position":[10,10],"size":[2,1],"fire":1,"fire_color":"#3399ff"}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let flame = *rig.paints().iter().find(|p| is_flame(p)).expect("пламя");
    assert_eq!(flame.atlas_rect, WHITE_PIXEL);
    assert_eq!(flame.position[0], 10.0);
    assert_eq!(flame.size[0], 2.0);
    assert!(
        (flame.position[1] + flame.size[1] - 11.0).abs() < 1e-5,
        "нижний край — нижний край прямоугольника огня: {flame:?}"
    );
    assert!(flame.size[1] > 0.79 && flame.size[1] <= 1.0, "{flame:?}");
    assert_eq!(&flame.color[..3], &[0.2, 0.6, 1.0]);
    assert_eq!(flame.color[3], 1.0);
}

#[test]
fn the_halo_and_the_flame_follow_the_object_before_its_smoke_and_sparks_and_before_the_next_object()
{
    let scene = r##"{"objects":[
        {"position":[10,10],"size":[2,1],"image":"plain","fire":0.8,"smoke":0.5,"sparks":0.5},
        {"position":[2,2],"size":[1,1],"color":"#ffffff","layer":0},
        {"position":[6,2],"size":[1,1],"color":"#ffffff","layer":1}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let paints = rig.paints();
    let kinds: Vec<&str> = paints
        .iter()
        .map(|paint| match paint.object {
            Some(_) => "object",
            None if is_halo(paint) => "halo",
            None if is_flame(paint) => "flame",
            None => "particle",
        })
        .collect();
    assert_eq!(kinds[0], "object");
    assert_eq!(&kinds[1..3], ["halo", "flame"]);
    assert!(kinds[3..kinds.len() - 2].iter().all(|k| *k == "particle"));
    assert!(kinds.len() > 5, "дым и искры есть: {kinds:?}");
    assert_eq!(&kinds[kinds.len() - 2..], ["object", "object"]);
    let order: Vec<Option<u32>> = paints.iter().map(|p| p.object).collect();
    assert_eq!(order[0], Some(0));
    assert_eq!(&order[order.len() - 2..], [Some(1), Some(2)]);
}

#[test]
fn the_fire_is_shifted_by_the_parallax_of_its_object_like_the_object_itself() {
    let scene = r##"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.8,"parallax":0.5,"color":"#ffffff"}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let still = rig.paints_in(&LayerView::default());
    let moved = rig.paints_in(&LayerView {
        shift: [10.0, 0.0],
        window_cells: 0.0,
    });
    assert_eq!(still.len(), 3);
    assert_eq!(still.len(), moved.len());
    for (a, b) in still.iter().zip(&moved) {
        assert!(
            (b.position[0] - a.position[0] - 5.0).abs() < 1e-4,
            "{a:?} {b:?}"
        );
        assert_eq!(a.position[1], b.position[1]);
    }
}

#[test]
fn a_fire_wholly_outside_the_window_is_not_drawn() {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    let window = |left: f64, right: f64| LayerView {
        shift: [(left + right) / 2.0 - 20.0, 0.0],
        window_cells: right - left,
    };
    let flames = |layers: LayerView| rig.paints_in(&layers).len();
    assert_eq!(flames(window(0.0, 40.0)), 2, "в окне");
    assert_eq!(flames(window(30.0, 40.0)), 0, "справа от окна");
    assert_eq!(flames(window(0.0, 5.0)), 0, "слева от окна");
    assert_eq!(flames(window(11.0, 40.0)), 2, "виден краем");
}

#[test]
fn a_scene_fire_burns_in_full_from_the_first_frame_and_a_fire_lit_by_the_code_rises() {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    assert_eq!(rig.strength(0), Some(0.8));

    let scene = r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    assert_eq!(rig.strength(0), None);
    assert!(rig.paints().is_empty());
    rig.game.world.set_number(0, property::FIRE, 1.0);
    rig.settle();
    assert_eq!(rig.strength(0), Some(0.0), "начинает с нуля");
    rig.frames(5, 0.1);
    let strength = rig.strength(0).expect("горит");
    assert!(
        strength > 0.95,
        "через полсекунды почти в полную силу: {strength}"
    );
}

#[test]
fn a_zero_fire_goes_out_within_a_second_and_a_deleted_object_at_once() {
    let scene = r#"{"objects":[
        {"position":[10,10],"size":[2,1],"fire":1},
        {"position":[20,10],"size":[2,1],"fire":1}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    rig.game.world.set_number(0, property::FIRE, 0.0);
    rig.game.world.delete(1);
    rig.frames(1, 0.1);
    assert!(
        rig.strength(0).is_some_and(|s| s < 1.0),
        "сила уходит плавно"
    );
    assert_eq!(rig.strength(1), None, "удалённый объект гаснет сразу");
    rig.frames(10, 0.1);
    assert_eq!(rig.strength(0), None);
    assert!(rig.paints().is_empty());
}

#[test]
fn the_stop_and_the_party_start_light_the_scene_fire_again_at_once() {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    rig.game.world.set_number(0, property::FIRE, 0.2);
    rig.frames(20, 0.1);
    rig.motion.reset();
    rig.game.world.set_number(0, property::FIRE, 0.8);
    rig.settle();
    assert_eq!(rig.strength(0), Some(0.8));
}

#[test]
fn a_paused_world_freezes_the_fire_and_the_editor_clock_moves_it() {
    let mut rig = Rig::new(HEARTH);
    rig.motion.tick(Some(60), 0.0);
    rig.settle();
    let before = rig.paints();
    for _ in 0..30 {
        rig.motion.tick(Some(60), 0.1);
        rig.settle();
    }
    assert_eq!(rig.paints(), before, "мир стоит — огонь стоит");
    rig.motion.tick(Some(66), 0.0);
    rig.settle();
    assert_ne!(rig.paints(), before, "шаги мира двигают огонь");

    let mut editor = Rig::new(HEARTH);
    editor.settle();
    let before = editor.paints();
    editor.frames(1, 0.1);
    assert_ne!(editor.paints(), before, "вне партии огонь идёт по draw(dt)");
}

#[test]
fn the_wind_leans_the_flame_and_the_halo_follows_it() {
    let mut calm = Rig::new(HEARTH);
    calm.settle();
    let mut windy =
        Rig::new(r#"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.8}],"wind":[1.5,0]}"#);
    windy.settle();
    windy.frames(20, 0.1);
    calm.frames(20, 0.1);
    let lean = |rig: &Rig| {
        rig.paints()
            .iter()
            .find(|p| is_flame(p))
            .expect("пламя")
            .lean
    };
    assert!(lean(&windy) > 0.05, "{}", lean(&windy));
    assert!(lean(&calm).abs() < lean(&windy));
}

#[test]
fn the_fire_does_not_touch_the_world_the_selection_or_the_count_of_objects() {
    let mut rig =
        Rig::new(r##"{"objects":[{"position":[10,10],"size":[2,1],"color":"#ffffff","fire":1}]}"##);
    rig.settle();
    rig.frames(20, 0.1);
    assert_eq!(rig.game.world.alive_count(), 1);
    assert_eq!(rig.game.world.ids().count(), 1);

    let viewport = [800.0, 400.0];
    let scale = 20.0;
    let on_object = [11.0 * scale, 10.5 * scale];
    assert_eq!(
        object_at(&rig.game.world, &rig.game.scene, on_object, viewport),
        Some(0)
    );
    let on_halo = [8.0 * scale, 10.5 * scale];
    assert_eq!(
        object_at(&rig.game.world, &rig.game.scene, on_halo, viewport),
        None,
        "ореол щелчок не видит"
    );

    let capped = game_json("", FILES).replace(r#""max_objects":100"#, r#""max_objects":1"#);
    let (mut loaded, ..) =
        load_with(&capped, HEARTH, NO_RULES, None).expect("один объект при потолке в один");
    step(&mut loaded, 5);
    assert_eq!(loaded.world.alive_count(), 1);
}

#[test]
fn the_fire_does_not_touch_the_course_of_the_party() {
    let code = r#"function tick(obj) print(math.random(1000)) end"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;
    let with_fire = r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true,"fire":0.7}]}"#;
    let run = |scene: &str| {
        let (mut game, _, images) =
            load_flat(scene, rules, Some(code)).expect("должно загрузиться");
        let mut motion = Motion::default();
        let mut snapshots = Vec::new();
        for _ in 0..40_u64 {
            game.step(StepInput::empty());
            motion.tick(Some(game.step_count()), 0.0);
            motion.update_fires(game.has_world(), game.wind(), fire_objects(&game.world));
            motion.update(game.wind()[0], sway_objects(&game.world, &images));
            snapshots.push((
                game.messages().to_vec(),
                game.world.vec2(0, property::POSITION),
            ));
        }
        snapshots
    };
    assert_eq!(run(with_fire), run(PLAIN), "та же партия с огнём и без");
}

// -------------------------------------------------------------------------------------------
// Шейдер
// -------------------------------------------------------------------------------------------

fn instance_locations(module: &naga::Module) -> Vec<u32> {
    let entry = module
        .entry_points
        .iter()
        .find(|entry| entry.name == "vs_main")
        .expect("вершинный шейдер");
    entry
        .function
        .arguments
        .iter()
        .flat_map(|argument| match &module.types[argument.ty].inner {
            naga::TypeInner::Struct { members, .. } => members.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .filter_map(|member| match member.binding {
            Some(naga::Binding::Location { location, .. }) => Some(location),
            _ => None,
        })
        .collect()
}

#[test]
fn rect_wgsl_with_the_flame_validates_and_has_no_more_than_sixteen_vertex_attributes() {
    let source = include_str!("../shaders/rect.wgsl");
    let module = naga::front::wgsl::parse_str(source).expect("WGSL разбирается");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("WGSL проходит проверку");
    let mut locations = instance_locations(&module);
    locations.sort_unstable();
    locations.dedup();
    assert!(
        locations.len() <= 16,
        "атрибутов {}: {locations:?}",
        locations.len()
    );
    assert!(
        locations.contains(&13) && locations.contains(&14),
        "{locations:?}"
    );
}

/// Как `wgpu-hal` собирает GLSL для WebGL2: биндинг → слот из раскладки группы, `es 300`.
fn to_webgl2_glsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry_point: &str,
) -> String {
    use naga::back::glsl;
    let mut binding_map = glsl::BindingMap::default();
    for binding in 0..3 {
        binding_map.insert(naga::ResourceBinding { group: 0, binding }, 0);
    }
    let options = glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE
            | glsl::WriterFlags::FORCE_POINT_SIZE,
        binding_map,
        zero_initialize_workgroup_memory: true,
    };
    let pipeline = glsl::PipelineOptions {
        shader_stage: stage,
        entry_point: entry_point.to_string(),
        multiview: None,
    };
    let mut out = String::new();
    glsl::Writer::new(
        &mut out,
        module,
        info,
        &options,
        &pipeline,
        naga::proc::BoundsCheckPolicies::default(),
    )
    .unwrap_or_else(|e| panic!("{entry_point}: писатель GLSL не создан: {e}"))
    .write()
    .unwrap_or_else(|e| panic!("{entry_point}: GLSL не записан: {e}"));
    out
}

#[test]
fn rect_wgsl_with_the_flame_translates_to_glsl_es_300_for_webgl2() {
    let source = include_str!("../shaders/rect.wgsl");
    let module = naga::front::wgsl::parse_str(source).expect("WGSL разбирается");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("WGSL проходит проверку");
    for (stage, entry) in [
        (naga::ShaderStage::Vertex, "vs_main"),
        (naga::ShaderStage::Fragment, "fs_main"),
    ] {
        let glsl = to_webgl2_glsl(&module, &info, stage, entry);
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
    }
}

#[test]
fn the_values_in_keys_and_clicks_are_checked_like_those_in_the_scene() {
    let object = |extra: &str| {
        format!(r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff",{extra}}}]}}"##)
    };
    for extra in [
        r#""keys":{"Space":{"press":[["fire",0.5]]}}"#,
        r##""on_click":[["fire_color","#ff0000"]]"##,
    ] {
        load_flat(&object(extra), NO_RULES, None).unwrap_or_else(|e| panic!("{extra}: {e:?}"));
    }
    for (extra, part) in [
        (
            r#""keys":{"Space":{"press":[["fire",2]]}}"#,
            "fire: нужно от 0 до 1",
        ),
        (
            r#""keys":{"Space":{"press":[["fire_glow",-1]]}}"#,
            "fire_glow: нужно от 0 до 1",
        ),
        (
            r#""on_click":[["fire_color","red"]]"#,
            "цвет должен быть вида",
        ),
    ] {
        let errors = errors_of(load_flat(&object(extra), NO_RULES, None));
        assert_error(&errors, "scene.json", "", part);
    }
    let errors = errors_of(load_with(
        &game_json(CAMERA_3D, ""),
        &object(r#""keys":{"Space":{"press":[["fire",1]]}}"#),
        NO_RULES,
        None,
    ));
    assert_error(
        &errors,
        "scene.json",
        "",
        "fire есть только в плоской сцене",
    );
}

//! Фаза 36 — угли: свойство `fire_coals`, его проверка перед запуском, код и правила, `set_property` вне
//! партии, угли в списке рисования (объект, ореол, пламя, угли, дым и искры того же объекта), накал на
//! часах движения, шейдер углей. Формулы накала и порыва — в `render::fire`'s own `mod tests`; вид углей
//! и сама видеокарта — только QA на стенде.

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::scene::{LayerView, object_at};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest_with_stamps, read_entry};
use engine::data::session;
use engine::render::atlas::{
    AtlasImage, AtlasRect, FireKind, HALO_RECT, RectPaint, WHITE_PIXEL, compose_world_paints,
    fire_objects, pack, particle_emitters, sway_objects,
};
use engine::render::wind::Motion;
use serde_json::json;

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"mark":"flag","hits":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;
const FILES: &str = r#","images":{"plain":{"path":"plain.png"}}"#;

const HEARTH: &str =
    r#"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.8,"fire_coals":0.5}]}"#;
const COALS_ONLY: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"fire_coals":0.6}]}"#;
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
// Свойство: загрузка и проверка перед запуском
// -------------------------------------------------------------------------------------------

#[test]
fn the_property_loads_as_a_number_without_a_warning_and_the_edges_of_the_range_are_allowed() {
    let (game, warnings, _images) = load_flat(HEARTH, NO_RULES, None).expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("угл")),
        "{warnings:?}"
    );
    assert_eq!(game.world.number_like(0, property::FIRE_COALS), Some(0.5));
    for coals in ["0", "1", "0.05"] {
        let scene =
            format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],"fire_coals":{coals}}}]}}"#);
        load_flat(&scene, NO_RULES, None).unwrap_or_else(|e| panic!("{coals}: {e:?}"));
    }
}

#[test]
fn every_check_before_the_start_names_the_property_and_the_place() {
    let cases = [
        (r#""fire_coals":1.5"#, "fire_coals: нужно от 0 до 1"),
        (r#""fire_coals":-0.1"#, "fire_coals: нужно от 0 до 1"),
        (r#""fire_coals":"много""#, "ожидалось число"),
        (r#""fire_coals":true"#, "ожидалось число"),
    ];
    for (field, message) in cases {
        let scene = format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],{field}}}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(&errors, "scene.json", "fire_coals", message);
    }
}

#[test]
fn the_property_needs_a_position_and_a_size_and_does_not_repeat() {
    for object in [
        r#"{"size":[1,1],"fire_coals":0.5}"#,
        r#"{"position":[1,1],"fire_coals":0.5}"#,
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
    let repeated = r##"{"objects":[{"position":[1,1],"size":[1,1],"fire_coals":0.5,
        "repeat_x":true,"color":"#ffffff"}]}"##;
    let errors = errors_of(load_flat(repeated, NO_RULES, None));
    assert_error(
        &errors,
        "scene.json",
        "",
        "свойства огня заданы вместе с repeat_x",
    );

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"fire_coals":0.5}}]}"#;
    let errors = errors_of(load_flat(PLAIN, template, None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "свойства огня разрешены только объекту с position и size",
    );
}

#[test]
fn a_rule_may_pile_the_coals_and_a_constant_outside_the_range_is_an_error() {
    let rule = |value: &str| {
        format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["mark"]}},"do":[["set","fire_coals",{value}]]}}]}}"#
        )
    };
    load_flat(PLAIN, &rule("0.8"), None).expect("0,8 годится");
    let errors = errors_of(load_flat(PLAIN, &rule("1.5"), None));
    assert_error(&errors, "rules.json", "", "fire_coals: нужно от 0 до 1");
    let errors = errors_of(load_flat(PLAIN, &rule("-1"), None));
    assert_error(&errors, "rules.json", "", "fire_coals: нужно от 0 до 1");

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"size":[1,1],"fire_coals":3}}]}"#;
    let errors = errors_of(load_flat(PLAIN, template, None));
    assert_error(&errors, "rules.json", "", "fire_coals: нужно от 0 до 1");
}

#[test]
fn the_smoulder_is_checked_like_the_coals_in_the_scene_in_three_dimensions_and_in_code() {
    for (field, message) in [
        (r#""fire_smolder":1.5"#, "fire_smolder: нужно от 0 до 1"),
        (r#""fire_smolder":-0.1"#, "fire_smolder: нужно от 0 до 1"),
        (r#""fire_smolder":"красно""#, "ожидалось число"),
    ] {
        let scene = format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],{field}}}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(&errors, "scene.json", "fire_smolder", message);
    }
    let scene =
        r#"{"objects":[{"position":[1,1],"size":[1,1],"fire_coals":0.5,"fire_smolder":0.35}]}"#;
    let (game, ..) = load_flat(scene, NO_RULES, None).expect("должно загрузиться");
    assert_eq!(
        game.world.number_like(0, property::FIRE_SMOLDER),
        Some(0.35)
    );

    let flat3d = game_json(CAMERA_3D, "");
    let scene3d =
        r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","fire_smolder":0.5}]}"##;
    let errors = errors_of(load_with(&flat3d, scene3d, NO_RULES, None));
    assert_error(&errors, "scene.json", "", "только в плоской сцене");

    let game = run_code("  obj.fire_smolder = 0.3");
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.number_like(0, property::FIRE_SMOLDER), Some(0.3));
    let game = run_code("  obj.fire_smolder = 1.5");
    let error = game.code_error().expect("ошибка кода");
    assert!(
        error.message.contains("fire_smolder должен быть от 0 до 1"),
        "{error:?}"
    );
}

#[test]
fn the_property_in_a_three_dimensional_scene_is_an_error_everywhere() {
    let flat3d = game_json(CAMERA_3D, "");
    let scene =
        r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","fire_coals":0.5}]}"##;
    let errors = errors_of(load_with(&flat3d, scene, NO_RULES, None));
    assert_error(&errors, "scene.json", "", "только в плоской сцене");

    let plain3d = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let rule =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","fire_coals",0.5]]}]}"#;
    let errors = errors_of(load_with(&flat3d, plain3d, rule, None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "fire_coals есть только в плоской сцене",
    );
}

#[test]
fn the_values_in_keys_and_clicks_are_checked_like_those_in_the_scene() {
    let object = |extra: &str| {
        format!(r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff",{extra}}}]}}"##)
    };
    for extra in [
        r#""keys":{"Space":{"press":[["fire_coals",0.5]]}}"#,
        r#""on_click":[["fire_coals",1]]"#,
    ] {
        load_flat(&object(extra), NO_RULES, None).unwrap_or_else(|e| panic!("{extra}: {e:?}"));
    }
    for extra in [
        r#""keys":{"Space":{"press":[["fire_coals",2]]}}"#,
        r#""on_click":[["fire_coals",-1]]"#,
    ] {
        let errors = errors_of(load_flat(&object(extra), NO_RULES, None));
        assert_error(&errors, "scene.json", "", "fire_coals: нужно от 0 до 1");
    }
    let errors = errors_of(load_with(
        &game_json(CAMERA_3D, ""),
        &object(r#""keys":{"Space":{"press":[["fire_coals",1]]}}"#),
        NO_RULES,
        None,
    ));
    assert_error(
        &errors,
        "scene.json",
        "",
        "fire_coals есть только в плоской сцене",
    );
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
fn code_reads_and_writes_the_coals_as_a_number() {
    let game = run_code("  print(obj.fire_coals)\n  obj.fire_coals = 0.4\n  print(obj.fire_coals)");
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.messages(), ["print: nil", "print: 0.4"]);
    assert_eq!(game.world.number_like(0, property::FIRE_COALS), Some(0.4));
}

#[test]
fn code_writing_a_value_outside_the_range_is_a_runtime_error() {
    for (body, part) in [
        ("obj.fire_coals = 1.5", "fire_coals должен быть от 0 до 1"),
        ("obj.fire_coals = -0.1", "fire_coals должен быть от 0 до 1"),
        ("obj.fire_coals = \"много\"", "ожидалось число"),
    ] {
        let game = run_code(body);
        let error = game.code_error().unwrap_or_else(|| panic!("{body}"));
        assert!(error.message.contains(part), "{body}: {error:?}");
        assert_eq!(
            game.world.number_like(0, property::FIRE_COALS),
            None,
            "{body}"
        );
    }
}

#[test]
fn code_may_not_pile_the_coals_in_a_three_dimensional_scene() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let files = r#","code":"code.lua""#;
    let (mut game, ..) = load_with(
        &game_json(CAMERA_3D, files),
        scene,
        CODE_RULES,
        Some("function tick(obj)\n  obj.fire_coals = 0.5\nend"),
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
fn the_property_is_set_and_taken_off_outside_a_party() {
    let (mut game, images) = loaded(PLAIN);
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "fire_coals", &json!(0.7)),
        Ok(())
    );
    assert_eq!(game.world.number_like(0, property::FIRE_COALS), Some(0.7));
    assert_eq!(
        session::remove_property(None, &mut game, 0, "fire_coals"),
        Ok(())
    );
    assert!(!game.world.has(0, property::FIRE_COALS));
}

#[test]
fn a_failed_check_outside_a_party_changes_nothing_and_says_why() {
    let (mut game, images) = loaded(HEARTH);
    for value in [json!(2), json!(-1), json!("много")] {
        let error = session::set_property(None, &mut game, &images, 0, "fire_coals", &value)
            .expect_err("вне отрезка");
        assert!(
            error.contains("от 0 до 1") || error.contains("число"),
            "{error}"
        );
    }
    assert_eq!(game.world.number_like(0, property::FIRE_COALS), Some(0.5));
}

#[test]
fn the_property_is_refused_outside_a_party_without_a_position_a_size_or_with_a_repeat() {
    for object in [
        r#"{"position":[1,1],"mark":true}"#,
        r#"{"size":[1,1],"mark":true}"#,
        r##"{"position":[1,1],"size":[1,1],"repeat_x":true,"color":"#ffffff"}"##,
    ] {
        let (mut game, images) = loaded(&format!(r#"{{"objects":[{object}]}}"#));
        let expected = load_message(&object.replace('}', r#","fire_coals":0.5}"#));
        let error = session::set_property(None, &mut game, &images, 0, "fire_coals", &json!(0.5))
            .expect_err(object);
        assert_eq!(error, expected, "{object}");
        assert!(!game.world.has(0, property::FIRE_COALS), "{object}");
    }
}

#[test]
fn position_or_size_edited_on_an_object_with_only_coals_outside_a_party_is_refused() {
    for name in ["position", "size"] {
        let (mut game, _images) = loaded(COALS_ONLY);
        let error = session::remove_property(None, &mut game, 0, name).expect_err(name);
        assert!(error.contains("свойства огня"), "{name}: {error}");
    }
}

// -------------------------------------------------------------------------------------------
// Угли в мире, в списке рисования и на часах движения
// -------------------------------------------------------------------------------------------

struct Rig {
    game: Game,
    images: Vec<ImageDecl>,
    rects: Vec<AtlasRect>,
    motion: Motion,
}

impl Rig {
    fn new(scene: &str) -> Rig {
        Rig::with_wind(scene, "")
    }

    fn with_wind(scene: &str, wind: &str) -> Rig {
        let scene = scene.replacen(r#"]}"#, &format!("]{wind}}}"), 1);
        let (game, _warnings, images) =
            load_flat(&scene, NO_RULES, None).expect("должно загрузиться");
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

    fn coals(&self) -> Option<RectPaint> {
        self.paints().into_iter().find(is_coals)
    }

    fn heat(&self) -> f32 {
        self.coals().expect("угли рисуются").fire.expect("угли").1[0]
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
    matches!(paint.fire, Some((FireKind::Flame, _)))
}

fn is_coals(paint: &RectPaint) -> bool {
    matches!(paint.fire, Some((FireKind::Coals, _)))
}

#[test]
fn a_scene_fire_with_coals_draws_a_heap_on_the_bottom_edge_heated_to_its_strength_on_the_first_frame()
 {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    let paints = rig.paints();
    assert_eq!(paints.len(), 3, "ореол, пламя и угли: {paints:?}");
    let coals = rig.coals().expect("угли");
    assert_eq!(coals.atlas_rect, WHITE_PIXEL);
    assert_eq!(coals.position[0], 10.0);
    assert_eq!(coals.size, [2.0, 0.25]);
    assert!((coals.position[1] + coals.size[1] - 11.0).abs() < 1e-5);
    assert!(!coals.glow, "угли закрывают, а не светят");
    assert_eq!(coals.lean, 0.0);
    assert_eq!(coals.object, None);
    let heat = f64::from(rig.heat());
    assert!((0.8..=0.8 * 1.4 + 1e-6).contains(&heat), "{heat}");
}

#[test]
fn the_heap_takes_the_colour_of_the_fire() {
    let scene = r##"{"objects":[{"position":[10,10],"size":[2,1],"fire":1,"fire_coals":1,
        "fire_color":"#3399ff"}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let coals = rig.coals().expect("угли");
    assert_eq!(&coals.color[..3], &[0.2, 0.6, 1.0]);
    assert_eq!(coals.size, [2.0, 0.5]);
}

#[test]
fn coals_without_a_fire_are_a_cold_heap_from_the_first_frame_standing_in_the_draw_list() {
    let mut rig = Rig::new(COALS_ONLY);
    rig.settle();
    let paints = rig.paints();
    assert_eq!(paints.len(), 1, "ни ореола, ни пламени: {paints:?}");
    assert!(is_coals(&paints[0]));
    assert_eq!(rig.heat(), 0.0);
    assert_eq!(rig.strength(0), Some(0.0));
    rig.frames(30, 0.1);
    assert_eq!(rig.heat(), 0.0, "холодная куча остаётся холодной");
    assert_eq!(rig.paints().len(), 1);
}

#[test]
fn smouldering_coals_without_a_fire_glow_from_the_first_frame_with_no_flame_or_halo() {
    let scene =
        r#"{"objects":[{"position":[10,10],"size":[2,1],"fire_coals":0.8,"fire_smolder":0.35}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    let paints = rig.paints();
    assert_eq!(paints.len(), 1, "одни угли: {paints:?}");
    assert!(is_coals(&paints[0]));
    let glow = f64::from(rig.heat());
    assert!((0.35 - 1e-6..=0.35 * 1.4 + 1e-6).contains(&glow), "{glow}");
    rig.frames(30, 0.1);
    let glow = f64::from(rig.heat());
    assert!(
        (0.35 - 1e-6..=0.35 * 1.4 + 1e-6).contains(&glow),
        "угли тлеют дальше: {glow}"
    );
    assert_eq!(rig.paints().len(), 1, "пламя не появилось");
}

#[test]
fn a_fire_set_to_zero_drops_the_flame_while_the_coals_smoulder_and_cool_to_a_dark_heap() {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    rig.game.world.set_number(0, property::FIRE, 0.0);
    rig.frames(5, 0.1);
    assert!(
        rig.strength(0).is_some_and(|s| s < 0.05),
        "пламя почти опало"
    );
    assert!(
        rig.heat() > 0.3,
        "через полсекунды угли тлеют: {}",
        rig.heat()
    );
    rig.frames(2, 0.1);
    let paints = rig.paints();
    assert!(
        paints.iter().all(|p| !is_flame(p) && !is_halo(p)),
        "пламени и ореола нет: {paints:?}"
    );
    assert!(rig.coals().is_some());
    rig.frames(100, 0.1);
    assert!(
        rig.heat() < 0.02,
        "через десять секунд тёмная куча: {}",
        rig.heat()
    );
    assert!(rig.coals().is_some(), "куча остаётся");
}

#[test]
fn a_fire_raised_from_a_low_flame_heats_the_coals_in_a_second() {
    let scene = r#"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.2,"fire_coals":0.5}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    rig.game.world.set_number(0, property::FIRE, 1.0);
    rig.frames(10, 0.1);
    assert!(rig.heat() > 0.9, "{}", rig.heat());
}

#[test]
fn coals_added_to_a_burning_fire_are_heated_to_its_strength_at_once() {
    let mut rig = Rig::new(r#"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.7}]}"#);
    rig.settle();
    rig.frames(5, 0.1);
    assert!(rig.coals().is_none());
    rig.game.world.set_number(0, property::FIRE_COALS, 0.5);
    rig.settle();
    let heat = f64::from(rig.heat());
    assert!((0.7..=0.7 * 1.4 + 1e-6).contains(&heat), "{heat}");
}

#[test]
fn coals_taken_off_vanish_at_once_a_deleted_object_too_and_an_edit_leaves_the_heat_alone() {
    let scene = r#"{"objects":[
        {"position":[10,10],"size":[2,1],"fire":0.7,"fire_coals":0.5},
        {"position":[20,10],"size":[2,1],"fire":0.7,"fire_coals":0.5}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    rig.frames(5, 0.1);
    let coals_of = |rig: &Rig, x: f32| {
        rig.paints()
            .into_iter()
            .find(|p| is_coals(p) && p.position[0] == x)
    };
    let before = coals_of(&rig, 10.0).expect("угли").fire.expect("угли").1;
    rig.game.world.set_number(0, property::FIRE_COALS, 0.2);
    rig.settle();
    let edited = coals_of(&rig, 10.0).expect("угли");
    assert_eq!(edited.size[1], 0.1, "куча сразу другой высоты");
    assert_eq!(edited.fire.expect("угли").1, before, "накал и узор те же");

    rig.game.world.delete(1);
    rig.settle();
    assert!(coals_of(&rig, 20.0).is_none(), "удалённый объект");
    let result = session::remove_property(None, &mut rig.game, 0, "fire_coals");
    assert_eq!(result, Ok(()));
    rig.settle();
    assert!(rig.coals().is_none(), "угли сняты");
    assert!(rig.paints().iter().any(is_flame), "пламя горит дальше");
}

#[test]
fn a_strong_wind_blows_the_coals_up_to_one_and_four_tenths_of_the_heat_and_never_cold_ones() {
    let mut windy = Rig::with_wind(HEARTH, r#","wind":[3,0]"#);
    windy.settle();
    let mut cold = Rig::with_wind(COALS_ONLY, r#","wind":[3,0]"#);
    cold.settle();
    let mut blown = 0;
    for _ in 0..400 {
        windy.frames(1, 0.05);
        cold.frames(1, 0.05);
        let base = windy.strength(0).expect("горит");
        let drawn = f64::from(windy.heat());
        assert!(
            drawn >= base - 1e-6 && drawn <= 1.4 * base + 1e-6,
            "{base} {drawn}"
        );
        blown += usize::from(drawn > base + 1e-3);
        assert_eq!(cold.heat(), 0.0, "холодные угли ветер не разжигает");
    }
    assert!(blown > 20, "порывы раздувают: {blown}");
}

#[test]
fn a_paused_world_freezes_the_coals_and_the_editor_clock_moves_them() {
    let mut rig = Rig::new(HEARTH);
    rig.motion.tick(Some(60), 0.0);
    rig.settle();
    rig.game.world.set_number(0, property::FIRE, 0.0);
    rig.settle();
    let before = rig.paints();
    for _ in 0..30 {
        rig.motion.tick(Some(60), 0.1);
        rig.settle();
    }
    assert_eq!(rig.paints(), before, "мир стоит — накал и узор стоят");
    rig.motion.tick(Some(90), 0.0);
    rig.settle();
    let moved = rig.coals().expect("угли");
    assert_ne!(Some(moved), before.iter().copied().find(is_coals));
    assert!(
        rig.heat()
            < before
                .iter()
                .find(|p| is_coals(p))
                .expect("угли")
                .fire
                .expect("угли")
                .1[0]
    );

    let mut editor = Rig::new(HEARTH);
    editor.settle();
    let before = editor.paints();
    editor.frames(1, 0.1);
    assert_ne!(editor.paints(), before, "вне партии угли идут по draw(dt)");
}

#[test]
fn the_stop_and_the_party_start_heat_the_coals_at_once() {
    let mut rig = Rig::new(HEARTH);
    rig.settle();
    rig.game.world.set_number(0, property::FIRE, 0.0);
    rig.frames(80, 0.1);
    assert!(rig.heat() < 0.1);
    rig.motion.reset();
    rig.game.world.set_number(0, property::FIRE, 0.8);
    rig.settle();
    let heat = f64::from(rig.heat());
    assert!((0.8..=0.8 * 1.4 + 1e-6).contains(&heat), "{heat}");
}

#[test]
fn after_a_reread_the_fire_with_only_coals_stays_with_its_object() {
    let scene = r#"{"objects":[
        {"position":[4,10],"size":[1,1],"mark":true},
        {"position":[10,10],"size":[2,1],"fire_coals":0.6}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    rig.frames(5, 0.1);
    let seed = rig.coals().expect("угли").fire.expect("угли").1[1];
    let rest = r#"{"objects":[{"position":[10,10],"size":[2,1],"fire_coals":0.6}]}"#;
    let mut reread = Rig::new(rest);
    reread.motion = std::mem::take(&mut rig.motion);
    reread.motion.world_rebuilt();
    reread.settle();
    let after = reread.coals().expect("угли у своего объекта");
    assert_eq!(after.fire.expect("угли").1[1], seed, "узор тот же");
}

#[test]
fn the_draw_order_is_the_object_the_halo_the_flame_the_coals_then_its_smoke_and_sparks() {
    let scene = r##"{"objects":[
        {"position":[10,10],"size":[2,1],"image":"plain","fire":0.8,"fire_coals":0.5,
         "smoke":0.5,"sparks":0.5},
        {"position":[2,2],"size":[1,1],"color":"#ffffff","layer":0},
        {"position":[6,2],"size":[1,1],"color":"#ffffff","layer":1}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let kinds: Vec<&str> = rig
        .paints()
        .iter()
        .map(|paint| match paint.object {
            Some(_) => "object",
            None if is_halo(paint) => "halo",
            None if is_flame(paint) => "flame",
            None if is_coals(paint) => "coals",
            None => "particle",
        })
        .collect();
    assert_eq!(kinds[0], "object");
    assert_eq!(&kinds[1..4], ["halo", "flame", "coals"]);
    assert!(kinds[4..kinds.len() - 2].iter().all(|k| *k == "particle"));
    assert!(kinds.len() > 6, "дым и искры есть: {kinds:?}");
    assert_eq!(&kinds[kinds.len() - 2..], ["object", "object"]);
}

#[test]
fn the_coals_are_shifted_by_the_parallax_of_their_object_and_not_drawn_wholly_outside_the_window() {
    let scene = r##"{"objects":[{"position":[10,10],"size":[2,1],"fire":0.8,"fire_coals":0.5,
        "parallax":0.5,"color":"#ffffff"}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let still = rig.paints_in(&LayerView::default());
    let moved = rig.paints_in(&LayerView {
        shift: [10.0, 0.0],
        window_cells: 0.0,
    });
    assert_eq!(still.len(), 4);
    for (a, b) in still.iter().zip(&moved) {
        assert!(
            (b.position[0] - a.position[0] - 5.0).abs() < 1e-4,
            "{a:?} {b:?}"
        );
    }

    let mut only = Rig::new(COALS_ONLY);
    only.settle();
    let window = |left: f64, right: f64| LayerView {
        shift: [(left + right) / 2.0 - 20.0, 0.0],
        window_cells: right - left,
    };
    let drawn = |layers: LayerView| only.paints_in(&layers).len();
    assert_eq!(drawn(window(0.0, 40.0)), 1, "в окне");
    assert_eq!(drawn(window(30.0, 40.0)), 0, "справа от окна");
    assert_eq!(drawn(window(0.0, 5.0)), 0, "слева от окна");
    assert_eq!(drawn(window(11.0, 40.0)), 1, "виден краем");
}

#[test]
fn the_coals_do_not_touch_the_world_the_selection_or_the_count_of_objects() {
    let mut rig = Rig::new(
        r##"{"objects":[{"position":[10,10],"size":[2,1],"color":"#ffffff","fire_coals":1}]}"##,
    );
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
    let above = [11.0 * scale, 9.0 * scale];
    assert_eq!(
        object_at(&rig.game.world, &rig.game.scene, above, viewport),
        None
    );

    let capped = game_json("", FILES).replace(r#""max_objects":100"#, r#""max_objects":1"#);
    let (mut loaded, ..) =
        load_with(&capped, HEARTH, NO_RULES, None).expect("один объект при потолке в один");
    step(&mut loaded, 5);
    assert_eq!(loaded.world.alive_count(), 1);
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

fn validated() -> (naga::Module, naga::valid::ModuleInfo) {
    let source = include_str!("../shaders/rect.wgsl");
    let module = naga::front::wgsl::parse_str(source).expect("WGSL разбирается");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("WGSL проходит проверку");
    (module, info)
}

#[test]
fn rect_wgsl_with_the_coals_validates_and_has_no_more_than_sixteen_vertex_attributes() {
    let (module, _info) = validated();
    assert!(
        module
            .functions
            .iter()
            .any(|(_, function)| function.name.as_deref() == Some("coals_color")),
        "угли считает шейдер прямоугольников"
    );
    let mut locations = instance_locations(&module);
    locations.sort_unstable();
    locations.dedup();
    assert!(
        locations.len() <= 16,
        "атрибутов {}: {locations:?}",
        locations.len()
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
fn rect_wgsl_with_the_coals_translates_to_glsl_es_300_for_webgl2() {
    let (module, info) = validated();
    for (stage, entry) in [
        (naga::ShaderStage::Vertex, "vs_main"),
        (naga::ShaderStage::Fragment, "fs_main"),
    ] {
        let glsl = to_webgl2_glsl(&module, &info, stage, entry);
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
    }
}

//! Фаза 57 — нарисованный мир на болванках (шаги 1–2): `scene.camera.projection`, ключ модели `look` со значением
//! `picture`, `scene.light.shade_color`. Игры собираются в коде теста, как в `phase15_3d_scene_shapes.rs`; модели
//! берутся из настоящих файлов `art/modelcheck/*.glb`, как в `phase25_models.rs`. Геометрия камеры и карта теней
//! солнца проверяются ещё и рядом с кодом — в `core::camera` и `render::scene3d`.

use std::fs;
use std::path::PathBuf;

use engine::core::camera::Camera3d;
use engine::core::game::Game;
use engine::core::math3;
use engine::core::model::{Look, Model};
use engine::core::scene::{CameraConfig, Projection, on_click_target_ray_at, pointer_hit};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::read_entry;
use engine::data::load::{ImageVerdict, ModelFile, load_game_from_texts, load_rest_with_models};
use engine::render::models::model_draws;
use engine::render::relief::FlatShapes;
use engine::render::scene3d::compose_frame3d;
use engine::render::wind::Motion;

const WINDOW: [f32; 2] = [1920.0, 1080.0];
const WINDOW64: [f64; 2] = [1920.0, 1080.0];
const PROPS: &str = r#"{"properties":{"clicked":"number","hero":"flag"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;

fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":30,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

fn load(scene_extra: &str, scene: &str) -> Result<Game, Vec<String>> {
    load_game_from_texts(&game_json(scene_extra), PROPS, scene, NO_RULES, SCREENS)
        .map(|(game, _, _)| game)
        .map_err(|failure| describe(&failure))
}

fn describe(failure: &LoadFailure) -> Vec<String> {
    failure.errors.iter().map(error_text).collect()
}

fn error_text(error: &GameError) -> String {
    format!("{} → {}: {}", error.file, error.path, error.message)
}

fn expect_error(result: Result<Game, Vec<String>>, path: &str, fragment: &str) {
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

fn ortho(pitch: u32) -> String {
    format!(r#","view_height":12,"camera":{{"pitch":{pitch},"projection":"orthographic"}}"#)
}

const EMPTY: &str = r#"{"objects":[]}"#;
const HERO: &str = r##"{"objects":[
    {"name":"hero","position":[20,15],"size":[1,1],"color":"#ff0000","hero":true,"camera_follows":true}
]}"##;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// -------------------------------------------------------------------------------------------
// Разбор: `projection`, `shade_color`
// -------------------------------------------------------------------------------------------

#[test]
fn projection_is_perspective_without_the_key_and_orthographic_when_named() {
    let camera = |extra: &str| {
        load(extra, EMPTY)
            .expect("игра должна загрузиться")
            .scene
            .camera
            .expect("камера")
    };
    assert_eq!(
        camera(r#","view_height":12,"camera":{"pitch":55}"#),
        CameraConfig {
            pitch: 55.0,
            projection: Projection::Perspective
        }
    );
    assert_eq!(
        camera(r#","view_height":12,"camera":{"pitch":55,"projection":"perspective"}"#).projection,
        Projection::Perspective
    );
    assert_eq!(
        camera(&ortho(30)),
        CameraConfig {
            pitch: 30.0,
            projection: Projection::Orthographic
        }
    );
}

#[test]
fn a_projection_that_is_neither_perspective_nor_orthographic_is_an_error_with_its_own_place() {
    for projection in [
        "\"isometric\"",
        "\"Orthographic\"",
        "\"\"",
        "3",
        "null",
        "true",
    ] {
        let scene =
            format!(r#","view_height":12,"camera":{{"pitch":55,"projection":{projection}}}"#);
        expect_error(
            load(&scene, EMPTY),
            "scene → camera → projection",
            "projection",
        );
    }
    let Err(errors) = load(r#","view_height":12,"camera":{"projection":"bad"}"#, EMPTY) else {
        panic!("игра должна была не загрузиться");
    };
    for place in ["scene → camera: ", "scene → camera → projection: "] {
        assert!(
            errors.iter().any(|e| e.contains(place)),
            "обе ошибки сразу, нет \"{place}\": {errors:#?}"
        );
    }
}

#[test]
fn shade_color_is_a_cold_dark_blue_by_default_and_reads_a_color() {
    let shade = |light: &str| {
        let extra = format!(r#","view_height":12,"camera":{{"pitch":55}}{light}"#);
        load(&extra, EMPTY)
            .expect("игра должна загрузиться")
            .scene
            .light
            .shade_color
    };
    let default = shade("");
    let want = [0x5b, 0x6a, 0x9e].map(|c| c as f32 / 255.0);
    assert_eq!(default, want);
    assert_eq!(shade(r#","light":{}"#), want);
    assert_eq!(
        shade(r##","light":{"shade_color":"#ff8000"}"##),
        [1.0, 128.0 / 255.0, 0.0]
    );
}

#[test]
fn shade_color_that_is_not_a_color_is_an_error_with_its_own_place() {
    for value in ["\"red\"", "\"#12345\"", "5", "[1,2,3]", "null"] {
        let extra = format!(
            r#","view_height":12,"camera":{{"pitch":55}},"light":{{"shade_color":{value}}}"#
        );
        expect_error(
            load(&extra, EMPTY),
            "scene → light → shade_color",
            "shade_color",
        );
    }
}

// -------------------------------------------------------------------------------------------
// Разбор: `look`
// -------------------------------------------------------------------------------------------

fn art(name: &str) -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../art/modelcheck");
    path.push(format!("{name}.glb"));
    fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

fn page_file(bytes: Vec<u8>) -> ModelFile {
    let image = ImageVerdict::Ok {
        width: 4,
        height: 4,
        pixels: vec![200; 64],
    };
    ModelFile {
        bytes: Some(bytes),
        images: (0..2).map(|index| (index, image.clone())).collect(),
    }
}

fn with_models(models: &str, scene: &str) -> Result<Game, Vec<String>> {
    let text = format!(
        r##"{{"name":"M","scene":{{"width":40,"height":30,"background":"#000000","view_height":16,"camera":{{"pitch":55}}}},
"random_seed":1,"max_objects":512,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"models":{models}}}}}"##
    );
    let files: Vec<(String, ModelFile)> = ["house", "birch"]
        .into_iter()
        .map(|name| (name.to_string(), page_file(art(name))))
        .collect();
    let scene = format!(r#"{{"objects":[{scene}]}}"#);
    let (config, _) = read_entry(&text).map_err(|f| describe(&f))?;
    load_rest_with_models(
        &text,
        config,
        Some(PROPS),
        Some(&scene),
        Some(NO_RULES),
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
        &files,
    )
    .map(|(game, _, _, _)| game)
    .map_err(|failure| describe(&failure))
}

fn model_of<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель есть")
}

fn house_and_birch(look: &str) -> String {
    format!(
        r#"{{"house":{{"path":"models/house.glb"{look}}},"birch":{{"path":"models/birch.glb"}}}}"#
    )
}

const BOTH: &str = r#"{"name":"дом","model":"house","position":[10,10],"size":[2,2],"height":7},
{"name":"берёза","model":"birch","position":[20,10],"size":[1,1],"height":5}"#;

#[test]
fn look_is_lit_without_the_key_and_picture_when_named() {
    let game = with_models(&house_and_birch(""), BOTH).expect("игра должна загрузиться");
    assert_eq!(model_of(&game, "house").look, Look::Lit);
    let game = with_models(&house_and_birch(r#","look":"lit""#), BOTH).expect("lit");
    assert_eq!(model_of(&game, "house").look, Look::Lit);
    let game = with_models(&house_and_birch(r#","look":"picture""#), BOTH).expect("picture");
    assert_eq!(model_of(&game, "house").look, Look::Picture);
    assert_eq!(
        model_of(&game, "birch").look,
        Look::Lit,
        "другая модель не затронута"
    );
}

#[test]
fn a_look_that_is_neither_lit_nor_picture_is_an_error_even_painted() {
    for look in ["\"painted\"", "\"Picture\"", "\"\"", "1", "null", "true"] {
        let models = house_and_birch(&format!(r#","look":{look}"#));
        expect_error(with_models(&models, BOTH), "look", "look");
    }
}

// -------------------------------------------------------------------------------------------
// Модель `picture` не отбрасывает теней
// -------------------------------------------------------------------------------------------

#[test]
fn a_picture_model_casts_no_shadow_and_does_not_widen_the_shadow_map_but_is_still_drawn() {
    let lit = with_models(&house_and_birch(""), BOTH).expect("lit");
    let picture = with_models(&house_and_birch(r#","look":"picture""#), BOTH).expect("picture");
    assert!(model_of(&lit, "house").casts_shadow());
    assert!(!model_of(&picture, "house").casts_shadow());
    assert!(model_of(&picture, "birch").casts_shadow());

    let house = picture.world.models().index_of("house").expect("дом");
    let draws = model_draws(&picture);
    assert_eq!(draws.instances[house].len(), 1, "картина рисуется");
    let birch_only = 5.0;
    assert!(
        (draws.top - birch_only).abs() < 1e-9,
        "самая высокая точка для карты теней — берёза без дома: {}",
        draws.top
    );
    assert!(
        model_draws(&lit).top > 6.9,
        "освещённый дом поднимает карту теней"
    );
}

/// Слово хозяина 2026-10-29: нарисованная вышка отбрасывает тень. Картина с `shadow: true` бросает тень своим объёмом
/// и поднимает карту теней, как освещённая модель.
#[test]
fn a_picture_with_shadow_true_casts_a_shadow_and_widens_the_shadow_map() {
    let game = with_models(&house_and_birch(r#","look":"picture","shadow":true"#), BOTH)
        .expect("картина с тенью");
    let house = model_of(&game, "house");
    assert_eq!(house.look, Look::Picture);
    assert!(house.picture_shadow);
    assert!(house.casts_shadow());
    assert!(model_draws(&game).top > 6.9, "дом поднимает карту теней");

    let game = with_models(
        &house_and_birch(r#","look":"picture","shadow":false"#),
        BOTH,
    )
    .expect("картина без тени");
    assert!(!model_of(&game, "house").casts_shadow());
}

#[test]
fn shadow_on_a_lit_model_or_not_a_flag_is_an_error_with_its_own_place() {
    for extra in [r#","shadow":true"#, r#","look":"lit","shadow":false"#] {
        expect_error(
            with_models(&house_and_birch(extra), BOTH),
            "house → shadow",
            "только у картины",
        );
    }
    for value in ["1", "\"да\"", "null"] {
        let models = house_and_birch(&format!(r#","look":"picture","shadow":{value}"#));
        expect_error(
            with_models(&models, BOTH),
            "house → shadow",
            "shadow должен быть true или false",
        );
    }
    let Err(errors) = with_models(&house_and_birch(r#","shadow":1"#), BOTH) else {
        panic!("игра должна была не загрузиться");
    };
    let at_shadow: Vec<&String> = errors
        .iter()
        .filter(|e| e.contains("house → shadow"))
        .collect();
    assert_eq!(
        at_shadow.len(),
        1,
        "одна ошибка на ключ у живой модели: {errors:#?}"
    );
}

// -------------------------------------------------------------------------------------------
// Ортографическая камера игры
// -------------------------------------------------------------------------------------------

fn space(scene_extra: &str, scene: &str) -> Game {
    load(scene_extra, scene).expect("игра должна загрузиться")
}

#[test]
fn the_window_height_is_view_height_cells_of_depth_and_every_cell_is_the_same_size() {
    for pitch in [30, 55, 90] {
        let game = space(&ortho(pitch), HERO);
        let camera = game.camera_3d(WINDOW).expect("камера");
        assert_eq!(camera.projection, Projection::Orthographic);
        let middle = WINDOW64[0] / 2.0;
        let top = camera.ground_point([middle, 0.0]);
        let bottom = camera.ground_point([middle, WINDOW64[1]]);
        assert!(
            close(bottom[1] - top[1], 12.0),
            "pitch {pitch}: {top:?} {bottom:?}"
        );

        let cell = |at: [f64; 3]| {
            let a = camera.project(at).expect("видна");
            let b = camera
                .project([at[0] + 1.0, at[1] + 1.0, at[2]])
                .expect("видна");
            [b[0] - a[0], b[1] - a[1]]
        };
        let near = cell([20.0, 20.0, 0.0]);
        let far = cell([20.0, 10.0, 0.0]);
        assert!(
            close(near[0], far[0]) && close(near[1], far[1]),
            "pitch {pitch}"
        );
        assert!(
            close(near[1], WINDOW64[1] / 12.0),
            "pitch {pitch}: {near:?}"
        );
    }
}

#[test]
fn at_ninety_degrees_the_orthographic_ground_lands_where_the_flat_scene_puts_it() {
    let flat = space(r#","view_height":12"#, HERO);
    let (scale, offset) = flat.camera_frame(WINDOW);
    let camera = space(&ortho(90), HERO).camera_3d(WINDOW).expect("камера");
    for point in [[20.5, 15.5], [12.5, 8.0], [25.25, 20.75]] {
        let px = camera.project([point[0], point[1], 0.0]).expect("видна");
        let flat_px = [
            offset[0] as f64 + point[0] * scale as f64,
            offset[1] as f64 + point[1] * scale as f64,
        ];
        assert!(
            close(px[0], flat_px[0]) && close(px[1], flat_px[1]),
            "{px:?} {flat_px:?}"
        );
    }
}

#[test]
fn the_followed_object_stays_in_the_middle_and_ground_hit_returns_the_point_it_is_drawn_at() {
    let game = space(&ortho(55), HERO);
    let camera = game.camera_3d(WINDOW).expect("камера");
    assert_eq!(camera.target, [20.5, 15.5]);
    let middle = [WINDOW64[0] / 2.0, WINDOW64[1] / 2.0];
    let under = camera.ground_hit(middle).expect("земля");
    assert!(close(under[0], 20.5) && close(under[1], 15.5), "{under:?}");
    for point in [[3.0, 4.0], [20.5, 15.5], [37.0, 28.0], [-5.0, 40.0]] {
        let px = camera.project([point[0], point[1], 0.0]).expect("видна");
        let back = camera.ground_hit(px).expect("земля");
        assert!(
            close(back[0], point[0]) && close(back[1], point[1]),
            "{point:?} → {back:?}"
        );
        let hit = pointer_hit(&game.world, &game.scene, &camera, px).expect("под указателем");
        assert!(
            close(hit[0], point[0]) && close(hit[1], point[1]) && hit[2] == 0.0,
            "{point:?} → {hit:?}"
        );
    }
}

#[test]
fn the_camera_stops_at_every_edge_of_the_scene_like_the_perspective_one() {
    let at = |x: f64, y: f64| {
        let scene = format!(
            r##"{{"objects":[{{"name":"hero","position":[{x},{y}],"size":[1,1],"color":"#ff0000","hero":true,"camera_follows":true}}]}}"##
        );
        space(&ortho(55), &scene).camera_3d(WINDOW).expect("камера")
    };
    let (w, h) = (WINDOW64[0], WINDOW64[1]);
    let left = at(0.0, 15.0).ground_point([0.0, h / 2.0]);
    assert!(close(left[0], 0.0), "{left:?}");
    let right = at(39.0, 15.0).ground_point([w, h / 2.0]);
    assert!(close(right[0], 40.0), "{right:?}");
    let top = at(20.0, 0.0).ground_point([w / 2.0, 0.0]);
    assert!(close(top[1], 0.0), "{top:?}");
    let bottom = at(20.0, 29.0).ground_point([w / 2.0, h]);
    assert!(close(bottom[1], 30.0), "{bottom:?}");
    let free = at(20.0, 15.0);
    assert_eq!(free.target, [20.5, 15.5]);
}

const CLICKABLE: &str = r##"{"objects":[
    {"name":"near","position":[20,16],"size":[2,1],"shape":"box","height":3,"color":"#ff0000",
     "on_click":[["clicked",1]]},
    {"name":"far","position":[20,12],"size":[2,1],"shape":"box","height":3,"color":"#00ff00",
     "on_click":[["clicked",2]]}
]}"##;

/// Что мышь шлёт ядру при щелчке по точке мира: место на земле под пикселем и начало луча через него.
fn click_at(game: &Game, point: [f64; 3]) -> (Option<u32>, [f64; 3], [f64; 3]) {
    let camera = game.camera_3d(WINDOW).expect("камера");
    let window = camera.project(point).expect("видна");
    let hit = pointer_hit(&game.world, &game.scene, &camera, window).expect("под указателем");
    let origin = camera.ray_origin(window);
    (
        on_click_target_ray_at(&game.world, &game.scene, Some(origin), hit),
        hit,
        origin,
    )
}

#[test]
fn an_object_under_the_pointer_is_picked_by_a_ray_parallel_to_the_view() {
    let game = space(&ortho(55), CLICKABLE);
    assert_eq!(
        click_at(&game, [21.0, 16.5, 1.5]).0,
        Some(0),
        "ближняя коробка"
    );
    assert_eq!(
        click_at(&game, [21.0, 12.5, 1.5]).0,
        Some(1),
        "дальняя коробка"
    );
    assert_eq!(
        click_at(&game, [21.0, 16.5, 12.0]).0,
        None,
        "выше коробок луч, идущий вдоль взгляда, проходит мимо обеих"
    );
    assert_eq!(click_at(&game, [30.0, 16.5, 1.5]).0, None, "мимо коробок");
    let (_, _, origin) = click_at(&game, [21.0, 16.5, 1.5]);
    let camera = game.camera_3d(WINDOW).expect("камера");
    assert_ne!(origin, camera.eye, "луч идёт не из одного глаза");
    let to_hit = math3::sub([21.0, 16.5, 1.5], origin);
    let along = math3::normalize(to_hit);
    let forward = camera.forward();
    assert!(
        (0..3).all(|axis| close(along[axis], forward[axis])),
        "луч через пиксель идёт вдоль взгляда"
    );
}

#[test]
fn a_mouse_press_with_the_recorded_ray_origin_fires_on_click_of_the_object_it_hits() {
    let mut game = space(&ortho(55), CLICKABLE);
    let (_, hit, origin) = click_at(&game, [21.0, 12.5, 1.5]);
    game.set_cursor_point(hit, Some(origin));
    game.key_down("MouseLeft");
    let snapshot = game.take_input_snapshot();
    game.step(snapshot);
    let clicked = game.properties.resolve("clicked").expect("свойство");
    assert_eq!(game.world.number_like(1, clicked), Some(2.0));
    assert_eq!(game.world.number_like(0, clicked), None);
    assert_eq!(game.cursor_eye(), Some(origin));
}

const WHEEL_VIEWPORT: [f32; 2] = [1000.0, 600.0];

#[test]
fn the_wheel_zooms_the_orthographic_view_and_the_place_under_the_pointer_stays_under_it() {
    let extra = format!(
        r#"{},"player_camera":{{"zoom":[6,40],"start":[20,15]}}"#,
        ortho(55)
    );
    let mut game = space(&extra, EMPTY);
    let pointer = [700.0_f32, 200.0];
    let under = |game: &Game| {
        let camera = game.camera_3d(WHEEL_VIEWPORT).expect("камера");
        pointer_hit(
            &game.world,
            &game.scene,
            &camera,
            [pointer[0] as f64, pointer[1] as f64],
        )
        .expect("под указателем")
    };
    let before = under(&game);
    let start = game.player_view_height().expect("камера игрока");
    assert!(game.camera_wheel(-300.0, pointer, WHEEL_VIEWPORT));
    let closer = game.player_view_height().expect("камера игрока");
    assert!(closer < start, "{closer} < {start}");
    let after = under(&game);
    assert!(
        (0..3).all(|axis| close(before[axis], after[axis])),
        "{before:?} → {after:?}"
    );
    let camera = game.camera_3d(WHEEL_VIEWPORT).expect("камера");
    let top = camera.ground_point([500.0, 0.0]);
    let bottom = camera.ground_point([500.0, 600.0]);
    assert!(
        close(bottom[1] - top[1], closer),
        "высота окна — новые клетки вида"
    );
    assert!(game.camera_wheel(300.0, pointer, WHEEL_VIEWPORT));
    assert!(close(game.player_view_height().expect("вид"), start));
}

// -------------------------------------------------------------------------------------------
// Кадр: направление блика, цвет тени, карта теней
// -------------------------------------------------------------------------------------------

fn frame_of(game: &Game) -> engine::render::scene3d::Frame3d {
    let camera = game.camera_3d(WINDOW).expect("камера");
    compose_frame3d(
        game,
        &camera,
        &Motion::default(),
        &[],
        &[],
        &mut FlatShapes::default(),
    )
}

#[test]
fn the_frame_carries_the_view_direction_for_the_orthographic_camera_and_the_eye_otherwise() {
    let flat_eye = frame_of(&space(r#","view_height":12,"camera":{"pitch":55}"#, HERO)).eye;
    let game = space(&ortho(55), HERO);
    let camera = game.camera_3d(WINDOW).expect("камера");
    let eye = frame_of(&game).eye;
    assert_eq!(flat_eye[3], 0.0);
    assert!(flat_eye[2] > 1.0, "место камеры над землёй: {flat_eye:?}");
    assert_eq!(eye[3], 1.0);
    let back = math3::scale(camera.forward(), -1.0);
    for axis in 0..3 {
        assert!((f64::from(eye[axis]) - back[axis]).abs() < 1e-6, "{eye:?}");
    }
}

#[test]
fn the_frame_carries_the_shade_color_in_linear_light() {
    let default = frame_of(&space(&ortho(55), HERO)).shade_color;
    let linear = |c: u8| (f64::from(c) / 255.0).powf(2.2);
    for (got, byte) in default.iter().zip([0x5b, 0x6a, 0x9e]) {
        assert!((f64::from(*got) - linear(byte)).abs() < 0.01, "{default:?}");
    }
    let extra = format!(r##"{},"light":{{"shade_color":"#ffffff"}}"##, ortho(55));
    assert_eq!(frame_of(&space(&extra, HERO)).shade_color, [1.0; 3]);
    let extra = format!(r##"{},"light":{{"shade_color":"#000000"}}"##, ortho(55));
    assert_eq!(frame_of(&space(&extra, HERO)).shade_color, [0.0; 3]);
}

#[test]
fn the_view_proj_of_the_orthographic_frame_matches_the_projection_of_points() {
    let game = space(&ortho(55), HERO);
    let camera = game.camera_3d(WINDOW).expect("камера");
    let frame = frame_of(&game);
    for point in [[20.5, 15.5, 0.0], [10.0, 8.0, 0.0], [30.0, 22.0, 25.0]] {
        let clip = math3::transform_point(&frame.view_proj, point);
        assert!(
            close(clip[3], 1.0),
            "ортографическая матрица w не делит: {clip:?}"
        );
        let px = camera.project(point).expect("видна");
        assert!(((clip[0] * 0.5 + 0.5) * WINDOW64[0] - px[0]).abs() < 1e-3);
        assert!(((0.5 - clip[1] * 0.5) * WINDOW64[1] - px[1]).abs() < 1e-3);
        assert!(
            (0.0..=1.0).contains(&clip[2]),
            "глубина {point:?}: {}",
            clip[2]
        );
    }
}

// -------------------------------------------------------------------------------------------
// Шейдер: картина без кривой яркости
// -------------------------------------------------------------------------------------------

#[test]
fn the_picture_color_goes_to_the_screen_through_srgb_only_not_through_the_tone_curve() {
    let source = include_str!("../shaders/scene3d.wgsl");
    let module = naga::front::wgsl::parse_str(source).expect("WGSL разбирается");
    let name_of = |handle: naga::Handle<naga::Function>| {
        module.functions[handle].name.clone().unwrap_or_default()
    };
    let picture = module
        .functions
        .iter()
        .find(|(_, function)| function.name.as_deref() == Some("picture_color"))
        .map(|(_, function)| function)
        .expect("picture_color есть в шейдере");
    let mut called = Vec::new();
    collect_calls(&picture.body, &mut called);
    let called: Vec<String> = called.into_iter().map(name_of).collect();
    assert_eq!(called, ["linear_to_srgb"], "никакой tone_map и finish");
}

fn collect_calls(block: &naga::Block, out: &mut Vec<naga::Handle<naga::Function>>) {
    for statement in block {
        if let naga::Statement::Call { function, .. } = statement {
            out.push(*function);
        }
    }
}

// -------------------------------------------------------------------------------------------
// Свойства камеры для остального кода
// -------------------------------------------------------------------------------------------

#[test]
fn a_perspective_camera_is_unchanged_by_the_new_field() {
    let camera = Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, WINDOW64);
    assert_eq!(camera.projection, Projection::Perspective);
    assert_eq!(camera.ray_origin([0.0, 0.0]), camera.eye);
    assert_eq!(camera.ray_origin(WINDOW64), camera.eye);
    let game_camera = Camera3d::game(
        CameraConfig {
            pitch: 55.0,
            projection: Projection::Perspective,
        },
        [16.0, 12.0],
        12.0,
        WINDOW64,
    );
    assert_eq!(game_camera, camera);
}

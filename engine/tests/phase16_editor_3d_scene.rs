//! Фаза 16 — редактор трёхмерной сцены. Математика камеры с поворотом, подбор камеры редактора,
//! выбор по лучу без `on_click`, углы рамки, `transform_object` и камера игры на паузе, видимая
//! земля для теней: игры собираются из текстов, как в `phase15_3d_scene_shapes.rs`. Вызовы
//! `engine::wasm` собираются только под WebAssembly и здесь не запускаются — они тонкая обёртка
//! над тем, что проверено ниже.

use engine::core::camera::{Camera3d, EDITOR_PITCH_RANGE, EditorCamera};
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::math3;
use engine::core::property;
use engine::core::scene::{
    ObjectTransform, editor_target_ray, ground_footprint, object_screen_corners, transform_object,
};
use engine::core::screens::ScreenState;
use engine::data::load::load_game_from_texts;
use engine::data::session::PlaySession;
use engine::render::scene3d::compose_frame3d;
use serde_json::json;

const WINDOW: [f32; 2] = [1920.0, 1080.0];
const WINDOW_F64: [f64; 2] = [1920.0, 1080.0];
const YAWS: [f64; 4] = [0.0, 37.0, 90.0, 200.0];
const PITCHES: [f64; 3] = [5.0, 55.0, 90.0];

const PROPS: &str = r#"{"properties":{}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;

fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":32,"height":24,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

const CAMERA: &str = r#","view_height":12,"camera":{"pitch":55}"#;
const FLAT: &str = r#","view_height":12"#;
const FLAT_OBJECTS: &str = r##"
    {"name":"hero","position":[9,9],"size":[1,1],"camera_follows":true,"color":"#2f6fdb"},
    {"name":"wall","position":[15,10],"size":[3,1],"color":"#996633"},
    {"name":"note"}"##;

fn load(scene_extra: &str, objects: &str) -> Game {
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    let (game, _screens, _warnings) =
        load_game_from_texts(&game_json(scene_extra), PROPS, &scene, NO_RULES, SCREENS)
            .expect("игра должна загрузиться");
    game
}

const OBJECTS: &str = r##"
    {"name":"hero","position":[9,9],"size":[1,1],"camera_follows":true,"shape":"capsule","height":1.8,"color":"#2f6fdb"},
    {"name":"wall","position":[15,10],"size":[3,1],"shape":"box","height":2,"rotation":30,"color":"#996633"},
    {"name":"stream","position":[20,10],"size":[4,1],"rotation":40,"color":"#3399ff"},
    {"name":"blocker","position":[26,10],"size":[3,3]},
    {"name":"note"}"##;

fn load3d() -> Game {
    load(CAMERA, OBJECTS)
}

const HERO: u32 = 0;
const WALL: u32 = 1;
const STREAM: u32 = 2;
const NOTE: u32 = 4;

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn inside_window(point: [f64; 2]) -> bool {
    let tolerance = 1e-6;
    point[0] >= -tolerance
        && point[0] <= WINDOW_F64[0] + tolerance
        && point[1] >= -tolerance
        && point[1] <= WINDOW_F64[1] + tolerance
}

// -------------------------------------------------------------------------------------------
// Камера с поворотом
// -------------------------------------------------------------------------------------------

/// «Камера игры» — `yaw` 0 и прежние числа: оси ровно те, что были до поворота, а камера редактора
/// с нулевым поворотом стоит там же.
#[test]
fn a_camera_without_yaw_keeps_the_numbers_of_the_game_camera() {
    let game_camera = Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, WINDOW_F64);
    let (sin, cos) = 55.0_f64.to_radians().sin_cos();
    assert_eq!(game_camera.yaw, 0.0);
    assert_eq!(game_camera.forward(), [0.0, -cos, -sin]);
    assert_eq!(game_camera.up(), [0.0, -sin, cos]);
    assert_eq!(game_camera.right(), [1.0, 0.0, 0.0]);

    let distance = math3::dot(
        math3::sub(game_camera.eye, [16.0, 12.0, 0.0]),
        math3::sub(game_camera.eye, [16.0, 12.0, 0.0]),
    )
    .sqrt();
    let orbiting = Camera3d::orbiting([16.0, 12.0], 0.0, 55.0, distance, WINDOW_F64);
    for axis in 0..3 {
        assert!(near(orbiting.eye[axis], game_camera.eye[axis], 1e-9));
    }
    assert!(near(orbiting.view_height, 12.0, 1e-9));
    for point in [[16.0, 12.0, 0.0], [10.0, 9.0, 2.0], [22.5, 15.0, 0.5]] {
        let (a, b) = (game_camera.project(point), orbiting.project(point));
        let (a, b) = (a.expect("in front"), b.expect("in front"));
        assert!(
            near(a[0], b[0], 1e-6) && near(a[1], b[1], 1e-6),
            "{a:?} {b:?}"
        );
    }
}

/// Поворот по часовой, если смотреть сверху: при 90° камера стоит слева от точки и смотрит вправо,
/// север сцены (меньшие `y`) оказывается слева в окне.
#[test]
fn yaw_turns_the_camera_clockwise_seen_from_above() {
    let camera = Camera3d::orbiting([16.0, 12.0], 90.0, 55.0, 15.0, WINDOW_F64);
    assert!(camera.eye[0] < 16.0 && near(camera.eye[1], 12.0, 1e-9));
    assert!(camera.forward()[0] > 0.0 && near(camera.forward()[1], 0.0, 1e-9));
    let north = camera.project([16.0, 8.0, 0.0]).expect("in front");
    let south = camera.project([16.0, 16.0, 0.0]).expect("in front");
    assert!(north[0] < WINDOW_F64[0] / 2.0 && south[0] > WINDOW_F64[0] / 2.0);
}

/// Точка земли → точка окна → та же точка земли, при любом повороте и наклоне; середина окна —
/// точка камеры.
#[test]
fn a_ground_point_survives_the_trip_to_the_window_and_back_at_every_yaw_and_pitch() {
    for yaw in YAWS {
        for pitch in PITCHES {
            let camera = Camera3d::orbiting([16.0, 12.0], yaw, pitch, 20.0, WINDOW_F64);
            let middle = camera
                .ground_hit([WINDOW_F64[0] / 2.0, WINDOW_F64[1] / 2.0])
                .expect("земля");
            assert!(
                near(middle[0], 16.0, 1e-6) && near(middle[1], 12.0, 1e-6),
                "{yaw} {pitch}: {middle:?}"
            );
            for point in [[16.0, 12.0], [19.0, 10.0], [12.0, 15.0], [18.0, 16.0]] {
                let px = camera
                    .project([point[0], point[1], 0.0])
                    .expect("перед камерой");
                let back = camera.ground_hit(px).expect("земля");
                assert!(
                    near(back[0], point[0], 1e-6) && near(back[1], point[1], 1e-6),
                    "{yaw} {pitch}: {point:?} → {back:?}"
                );
            }
        }
    }
}

/// Над горизонтом земли нет; точка за камерой не проецируется.
#[test]
fn a_point_above_the_horizon_has_no_ground_and_a_point_behind_the_camera_no_place() {
    let low = Camera3d::orbiting([16.0, 12.0], 33.0, 5.0, 12.0, WINDOW_F64);
    assert_eq!(low.ground_hit([WINDOW_F64[0] / 2.0, 0.0]), None);
    assert_eq!(
        low.ground_point([WINDOW_F64[0] / 2.0, 0.0]),
        low.target,
        "прежний ground_point над горизонтом — точка камеры"
    );
    assert!(
        low.ground_hit([WINDOW_F64[0] / 2.0, WINDOW_F64[1]])
            .is_some()
    );

    let behind = math3::add(low.eye, math3::scale(low.forward(), -3.0));
    assert_eq!(low.project(behind), None);
    let ahead = math3::add(low.eye, math3::scale(low.forward(), 3.0));
    assert!(low.project(ahead).is_some());
}

// -------------------------------------------------------------------------------------------
// Камера редактора и подбор расстояния
// -------------------------------------------------------------------------------------------

/// Без присланной камеры сцена видна так, что вся земля в окне, `yaw` 0, наклон игры.
#[test]
fn without_a_sent_camera_the_whole_ground_is_in_the_window_under_the_game_pitch() {
    let game = load3d();
    let fit = game.fit_camera(None, WINDOW).expect("трёхмерная сцена");
    assert_eq!((fit.yaw, fit.pitch), (0.0, 55.0));
    assert_eq!(fit.target, [16.0, 12.0]);
    let camera = game.editor_camera_3d(WINDOW).expect("трёхмерная сцена");
    assert_eq!(camera, fit.camera(WINDOW_F64));
    let mut extreme = 0.0_f64;
    for corner in [[0.0, 0.0], [32.0, 0.0], [32.0, 24.0], [0.0, 24.0]] {
        let px = camera
            .project([corner[0], corner[1], 0.0])
            .expect("in front");
        assert!(inside_window(px), "{px:?}");
        extreme = extreme
            .max((px[0] - WINDOW_F64[0] / 2.0).abs() / (WINDOW_F64[0] / 2.0))
            .max((px[1] - WINDOW_F64[1] / 2.0).abs() / (WINDOW_F64[1] / 2.0));
    }
    assert!(
        near(extreme, 1.0, 1e-6),
        "земля касается края окна: {extreme}"
    );
}

#[test]
fn a_flat_scene_has_no_editor_camera_and_no_fit() {
    let game = load(FLAT, FLAT_OBJECTS);
    assert_eq!(game.editor_camera_3d(WINDOW), None);
    assert_eq!(game.fit_camera(None, WINDOW), None);
    assert_eq!(game.fit_camera(Some(WALL), WINDOW), None);
}

/// Присланная камера держится, пока не пришла другая; сборка мира и начало партии её не сбрасывают;
/// наклон приводится к 5–90°.
#[test]
fn the_sent_camera_stays_through_the_world_rebuilds_and_its_pitch_is_kept_in_range() {
    let mut game = load3d();
    let sent = EditorCamera {
        target: [5.0, 6.0],
        yaw: 123.0,
        pitch: 20.0,
        distance: 9.0,
    };
    game.set_editor_camera(sent);
    let camera = game.editor_camera_3d(WINDOW).expect("камера");
    assert_eq!(
        (camera.target, camera.yaw, camera.pitch),
        ([5.0, 6.0], 123.0, 20.0)
    );
    game.show_scene();
    game.reset_for_play(true);
    game.show_scene();
    assert_eq!(game.editor_camera_3d(WINDOW), Some(camera));

    for (pitch, expected) in [(1.0, EDITOR_PITCH_RANGE.0), (100.0, EDITOR_PITCH_RANGE.1)] {
        game.set_editor_camera(EditorCamera { pitch, ..sent });
        let fit = game.fit_camera(Some(WALL), WINDOW).expect("объект");
        assert_eq!(fit.pitch, expected);
    }
}

/// Объём объекта — все восемь углов в окне при любых повороте и наклоне, середина объекта на земле —
/// в середине окна, а ближе подойти нельзя: что-то касается края.
#[test]
fn the_fit_to_an_object_shows_its_whole_volume_and_no_closer_is_possible() {
    let mut game = load3d();
    for yaw in [0.0, 37.0, 200.0] {
        for pitch in [5.0, 30.0, 55.0, 90.0] {
            game.set_editor_camera(EditorCamera {
                target: [3.0, 3.0],
                yaw,
                pitch,
                distance: 10.0,
            });
            for id in [HERO, WALL, STREAM] {
                let fit = game.fit_camera(Some(id), WINDOW).expect("объект");
                assert_eq!((fit.yaw, fit.pitch), (yaw, pitch));
                let camera = fit.camera(WINDOW_F64);
                let footprint = ground_footprint(&game.world, id).expect("место");
                let height = match id {
                    STREAM => 0.0,
                    _ => {
                        engine::core::shapes::Body::of_object(&game.world, id)
                            .expect("фигура")
                            .height
                    }
                };
                let center = footprint.center();
                let middle = camera
                    .project([center[0], center[1], 0.0])
                    .expect("in front");
                assert!(
                    near(middle[0], WINDOW_F64[0] / 2.0, 1e-6)
                        && near(middle[1], WINDOW_F64[1] / 2.0, 1e-6),
                    "{id} {yaw} {pitch}: {middle:?}"
                );
                let mut extreme = 0.0_f64;
                for corner in footprint.corners() {
                    for z in [0.0, height] {
                        let px = camera.project([corner[0], corner[1], z]).expect("in front");
                        assert!(inside_window(px), "{id} {yaw} {pitch}: {px:?}");
                        extreme = extreme
                            .max((px[0] - WINDOW_F64[0] / 2.0).abs() / (WINDOW_F64[0] / 2.0))
                            .max((px[1] - WINDOW_F64[1] / 2.0).abs() / (WINDOW_F64[1] / 2.0));
                    }
                }
                assert!(near(extreme, 1.0, 1e-6), "{id} {yaw} {pitch}: {extreme}");
            }
        }
    }
}

/// Без объекта, без его `position` и `size` — ничего.
#[test]
fn the_fit_to_a_missing_object_or_one_without_a_place_is_nothing() {
    let game = load3d();
    assert_eq!(game.fit_camera(Some(NOTE), WINDOW), None);
    assert_eq!(game.fit_camera(Some(99), WINDOW), None);
}

// -------------------------------------------------------------------------------------------
// Выбор в редакторе по лучу
// -------------------------------------------------------------------------------------------

fn pick(game: &Game, camera: &Camera3d, point: [f64; 3]) -> Option<u32> {
    let window = camera.project(point).expect("перед камерой");
    editor_target_ray(&game.world, &game.scene, camera, window)
}

fn cameras(game: &mut Game) -> Vec<Camera3d> {
    [(0.0, 55.0), (37.0, 55.0), (200.0, 30.0), (90.0, 90.0)]
        .into_iter()
        .map(|(yaw, pitch)| {
            game.set_editor_camera(EditorCamera {
                target: [16.0, 12.0],
                yaw,
                pitch,
                distance: 24.0,
            });
            game.editor_camera_3d(WINDOW).expect("камера")
        })
        .collect()
}

/// Объект без `on_click` выбирается щелчком по фигуре и по плоскому повёрнутому прямоугольнику;
/// мимо — ничего; объект без `shape`, `image` и `color` не выбирается.
#[test]
fn a_click_picks_a_drawn_object_without_on_click_and_only_where_it_really_is() {
    let mut game = load3d();
    for camera in cameras(&mut game) {
        assert_eq!(
            pick(&game, &camera, [16.5, 10.5, 1.0]),
            Some(WALL),
            "{camera:?}"
        );
        assert_eq!(
            pick(&game, &camera, [9.5, 9.5, 0.9]),
            Some(HERO),
            "{camera:?}"
        );
        assert_eq!(
            pick(&game, &camera, [22.0, 10.5, 0.0]),
            Some(STREAM),
            "{camera:?}"
        );
        // Угол неповёрнутого прямоугольника ручья лежит вне повёрнутого.
        assert_eq!(pick(&game, &camera, [20.0, 10.0, 0.0]), None, "{camera:?}");
        assert_eq!(pick(&game, &camera, [3.0, 20.0, 0.0]), None, "{camera:?}");
        assert_eq!(
            pick(&game, &camera, [27.5, 11.5, 0.0]),
            None,
            "невидимый: {camera:?}"
        );
    }
}

/// Из двух объектов на луче — ближний к камере; выше ближнего — дальний.
#[test]
fn of_two_objects_on_the_ray_the_nearer_one_is_picked() {
    let game = load(
        CAMERA,
        r##"{"name":"front","position":[10,10],"size":[1,1],"shape":"box","height":1.5,"color":"#ff0000"},
            {"name":"back","position":[10,9],"size":[1,1],"shape":"box","height":6,"color":"#00ff00"}"##,
    );
    let camera = Camera3d::orbiting([10.5, 10.0], 0.0, 55.0, 12.0, WINDOW_F64);
    assert_eq!(pick(&game, &camera, [10.5, 10.5, 1.5]), Some(0));
    assert_eq!(pick(&game, &camera, [10.5, 9.5, 5.0]), Some(1));
}

/// Плоские объекты на одной земле: при равной дальности — нарисованный сверху, то есть с большим
/// `layer`, а при равных слоях — с большим номером.
#[test]
fn flat_objects_on_the_same_ground_are_told_apart_by_the_drawing_order() {
    let game = load(
        CAMERA,
        r##"{"position":[10,10],"size":[4,4],"color":"#ff0000","layer":2},
            {"position":[10,10],"size":[4,4],"color":"#00ff00","layer":1},
            {"position":[10,10],"size":[4,4],"color":"#0000ff","layer":2}"##,
    );
    let camera = game.editor_camera_3d(WINDOW).expect("камера");
    assert_eq!(pick(&game, &camera, [12.0, 12.0, 0.0]), Some(2));
}

// -------------------------------------------------------------------------------------------
// Углы рамки
// -------------------------------------------------------------------------------------------

/// Четыре угла повёрнутого прямоугольника — те же точки, что `screen_point` его углов.
#[test]
fn the_frame_corners_are_the_projected_corners_of_the_turned_rectangle() {
    let mut game = load3d();
    for camera in cameras(&mut game) {
        for id in [WALL, STREAM] {
            let corners = object_screen_corners(&game.world, id, &camera).expect("углы");
            let ground = ground_footprint(&game.world, id).expect("место").corners();
            for (screen, corner) in corners.iter().zip(ground) {
                let expected = camera
                    .project([corner[0], corner[1], 0.0])
                    .expect("in front");
                assert_eq!(*screen, expected);
            }
        }
    }
}

/// Без объекта, без `position` и `size` и с углом за камерой — ничего.
#[test]
fn the_frame_has_no_corners_for_a_missing_object_or_one_behind_the_camera() {
    let game = load(
        CAMERA,
        r##"{"name":"near","position":[15,20],"size":[2,4],"color":"#ff0000"},{"name":"note"}"##,
    );
    let camera = Camera3d::orbiting([16.0, 12.0], 0.0, 5.0, 10.0, WINDOW_F64);
    assert_eq!(object_screen_corners(&game.world, 0, &camera), None);
    assert_eq!(object_screen_corners(&game.world, 1, &camera), None);
    assert_eq!(object_screen_corners(&game.world, 99, &camera), None);
    let far = Camera3d::orbiting([16.0, 12.0], 0.0, 55.0, 24.0, WINDOW_F64);
    assert!(object_screen_corners(&game.world, 0, &far).is_some());
}

// -------------------------------------------------------------------------------------------
// transform_object и камера игры
// -------------------------------------------------------------------------------------------

fn angle_of(game: &Game, id: u32) -> Option<f64> {
    game.world
        .rotation(id, property::ROTATION)
        .map(|rotation| rotation.angle())
}

/// Названные значения стоят в мире, не названные — как были; `show_scene` возвращает сцену из файла.
#[test]
fn transform_object_sets_what_is_named_and_show_scene_brings_the_file_back() {
    let mut game = load3d();
    transform_object(
        &mut game.world,
        WALL,
        ObjectTransform {
            position: [3.0, 4.0],
            z: None,
            size: [5.0, 6.0],
            height: None,
            rotation: None,
        },
    );
    assert_eq!(game.world.vec2(WALL, property::POSITION), Some([3.0, 4.0]));
    assert_eq!(game.world.vec2(WALL, property::SIZE), Some([5.0, 6.0]));
    assert_eq!(game.world.number_like(WALL, property::HEIGHT), Some(2.0));
    assert_eq!(angle_of(&game, WALL), Some(30.0));

    transform_object(
        &mut game.world,
        WALL,
        ObjectTransform {
            position: [3.0, 4.0],
            z: None,
            size: [5.0, 6.0],
            height: Some(4.5),
            rotation: Some(280.0),
        },
    );
    assert_eq!(game.world.number_like(WALL, property::HEIGHT), Some(4.5));
    assert_eq!(angle_of(&game, WALL), Some(280.0));
    assert_eq!(
        game.world.vec2(HERO, property::POSITION),
        Some([9.0, 9.0]),
        "другие не тронуты"
    );

    game.show_scene();
    assert_eq!(
        game.world.vec2(WALL, property::POSITION),
        Some([15.0, 10.0])
    );
    assert_eq!(game.world.vec2(WALL, property::SIZE), Some([3.0, 1.0]));
    assert_eq!(game.world.number_like(WALL, property::HEIGHT), Some(2.0));
    assert_eq!(angle_of(&game, WALL), Some(30.0));
}

/// Без объекта и без его `position` и `size` — ничего, а высота фигуры без `height` появляется.
#[test]
fn transform_object_skips_a_missing_object_and_adds_a_height_a_shape_did_not_have() {
    let mut game = load(
        CAMERA,
        r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000"},{"name":"note"}"##,
    );
    let transform = ObjectTransform {
        position: [7.0, 8.0],
        z: None,
        size: [2.0, 2.0],
        height: Some(1.0),
        rotation: None,
    };
    transform_object(&mut game.world, 1, transform);
    transform_object(&mut game.world, 99, transform);
    assert_eq!(game.world.vec2(1, property::POSITION), None);
    assert_eq!(game.world.number_like(0, property::HEIGHT), None);
    transform_object(&mut game.world, 0, transform);
    assert_eq!(game.world.number_like(0, property::HEIGHT), Some(1.0));
    assert_eq!(angle_of(&game, 0), None);
}

fn after_one_step(game: &mut Game) {
    game.step(StepInput::empty());
}

/// Требование 18: в трёхмерной сцене перенос в живом мире камеру игры не сдвигает, в плоской —
/// как раньше, сдвигает.
#[test]
fn moving_an_object_of_a_three_dimensional_scene_leaves_the_game_camera_where_it_stood() {
    let mut game = load3d();
    after_one_step(&mut game);
    let before = game.camera_3d(WINDOW).expect("камера");
    assert!(near(before.target[0], 9.5, 1e-9));
    game.move_object(HERO, [20.0, 14.0], None);
    assert_eq!(
        game.world.vec2(HERO, property::POSITION),
        Some([20.0, 14.0])
    );
    assert_eq!(game.camera_3d(WINDOW), Some(before));
    transform_object(
        &mut game.world,
        HERO,
        ObjectTransform {
            position: [21.0, 15.0],
            z: None,
            size: [1.0, 1.0],
            height: None,
            rotation: Some(10.0),
        },
    );
    assert_eq!(game.camera_3d(WINDOW), Some(before));

    let mut flat = load(FLAT, FLAT_OBJECTS);
    after_one_step(&mut flat);
    let frame = flat.camera_frame(WINDOW);
    flat.move_object(HERO, [20.0, 14.0], None);
    assert_ne!(flat.camera_frame(WINDOW), frame);
}

fn objects_scene() -> String {
    format!(r#"{{"objects":[{OBJECTS}]}}"#)
}

/// Требование 18: правка места через `set_property` на паузе ставит камеру игры на объект сразу.
#[test]
fn setting_the_place_on_a_pause_puts_the_game_camera_on_the_object() {
    let (mut game, config, _warnings) = load_game_from_texts(
        &game_json(CAMERA),
        PROPS,
        &objects_scene(),
        NO_RULES,
        SCREENS,
    )
    .expect("игра должна загрузиться");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    after_one_step(&mut game);
    let before = game.camera_3d(WINDOW).expect("камера");

    game.move_object(HERO, [20.0, 14.0], None);
    assert_eq!(
        game.camera_3d(WINDOW),
        Some(before),
        "во время жеста камера стоит"
    );

    session
        .set_property(&mut game, &[], HERO, "position", &json!([20.0, 14.0]))
        .expect("правка принята");
    let after = game.camera_3d(WINDOW).expect("камера");
    assert!(
        near(after.target[0], 20.5, 1e-9) && near(after.target[1], 14.5, 1e-9),
        "{:?}",
        after.target
    );
}

// -------------------------------------------------------------------------------------------
// Видимая земля и тени
// -------------------------------------------------------------------------------------------

/// Видимая земля лежит в пределах сцены, не пуста и содержит всё, что попадает в окно, — при низком
/// наклоне и повороте, когда верх окна смотрит выше горизонта.
#[test]
fn the_visible_ground_of_a_low_turned_camera_is_inside_the_scene_and_holds_the_window() {
    let game = load3d();
    for (yaw, pitch, distance) in [
        (33.0, 5.0, 12.0),
        (200.0, 5.0, 30.0),
        (90.0, 20.0, 15.0),
        (0.0, 55.0, 18.0),
    ] {
        let camera = Camera3d::orbiting([16.0, 12.0], yaw, pitch, distance, WINDOW_F64);
        let [low, high] = camera.visible_ground(&game.scene).expect("что-то видно");
        assert!(low[0] >= 0.0 && low[1] >= 0.0 && high[0] <= 32.0 && high[1] <= 24.0);
        assert!(low[0] < high[0] && low[1] < high[1], "{low:?} {high:?}");
        let eps = 1e-6;
        for ix in 0..=64 {
            for iy in 0..=48 {
                let point = [ix as f64 * 0.5, iy as f64 * 0.5];
                let Some(px) = camera.project([point[0], point[1], 0.0]) else {
                    continue;
                };
                if inside_window(px) {
                    assert!(
                        point[0] >= low[0] - eps
                            && point[0] <= high[0] + eps
                            && point[1] >= low[1] - eps
                            && point[1] <= high[1] + eps,
                        "{yaw} {pitch}: {point:?} в окне, но не в {low:?} {high:?}"
                    );
                }
            }
        }
    }
}

/// Камера, смотрящая мимо сцены, не видит её земли.
#[test]
fn a_camera_looking_away_from_the_scene_sees_none_of_its_ground() {
    let game = load3d();
    let away = Camera3d::orbiting([16.0, -30.0], 0.0, 30.0, 10.0, WINDOW_F64);
    assert_eq!(away.visible_ground(&game.scene), None);
}

/// Кадр с камерой редактора у горизонта и с поворотом собирается: тени и матрицы конечны.
#[test]
fn a_frame_from_a_low_turned_editor_camera_has_finite_shadow_matrices() {
    let mut game = load3d();
    game.set_editor_camera(EditorCamera {
        target: [16.0, 12.0],
        yaw: 33.0,
        pitch: 5.0,
        distance: 14.0,
    });
    let camera = game.editor_camera_3d(WINDOW).expect("камера");
    let frame = compose_frame3d(&game, &camera, 0.0, &[], &[]);
    assert!(!frame.shapes.is_empty());
    for matrix in [frame.view_proj, frame.light_view_proj] {
        assert!(matrix.iter().flatten().all(|v| v.is_finite()), "{matrix:?}");
    }
    assert!(frame.depth_per_cell.is_finite() && frame.depth_per_cell > 0.0);
}

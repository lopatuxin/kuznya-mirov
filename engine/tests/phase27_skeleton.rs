//! Фаза 27 — скелет и движения. Игры собираются в коде теста, как в `phase25_models.rs`; модель со скелетом —
//! маленький `.glb`, который собирает сам тест: таз, две ноги из бедра, голени и ступни и плоский щиток на тазе,
//! движения `Idle`, `Walk` и `Roll`. Проверяются ключи `idle`, `walk`, `feet` и ошибки перед запуском, свойство
//! `animation`, стойка и ходьба с плавным переходом, поворот по ходу, ступни на настиле и шейдер скелетной модели.

use std::fs;
use std::path::PathBuf;

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::model::Model;
use engine::core::motion::MotionState;
use engine::core::property;
use engine::core::skeleton::{Blend, Playing, PoseScratch};
use engine::data::error::GameError;
use engine::data::load::{ImageVerdict, ModelFile, load_rest_with_models, read_entry};
use engine::render::models::model_draws;

#[path = "skeleton_support/mod.rs"]
mod skeleton_support;
use skeleton_support::joint_point;

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag"}}"#;
const MODELS: &str = r#"{"rig":{"path":"models/rig.glb","idle":"Idle","walk":"Walk","feet":["foot_left","foot_right"]},"house":{"path":"models/house.glb"}}"#;
const WALK_RULE: &str = r#"{"rules":[{"kind":"walk","for":{"has":["actor"]}}]}"#;
const KEYED: &str =
    r#","keys":{"KeyR":{"press":[["animation","Roll"]]},"KeyT":{"press":[["animation",""]]}}"#;

fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

/// Модель со скелетом. Файл: таз на высоте 1, от него две ноги по 1 вниз — бедро, голень, ступня; щиток 0,6 × 2
/// целиком на тазе.
fn rig_glb() -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<String> = Vec::new();
    let mut accessors: Vec<String> = Vec::new();
    let mut put = |bytes: Vec<u8>, component: u32, kind: &str, count: usize, extra: &str| {
        let offset = bin.len();
        bin.extend(&bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{}}}"#,
            bytes.len()
        ));
        accessors.push(format!(
            r#"{{"bufferView":{},"componentType":{component},"count":{count},"type":"{kind}"{extra}}}"#,
            views.len() - 1
        ));
        accessors.len() - 1
    };
    let position = put(
        floats([-0.3, 0.0, 0.0, 0.3, 0.0, 0.0, 0.3, 2.0, 0.0, -0.3, 2.0, 0.0]),
        5126,
        "VEC3",
        4,
        r#","min":[-0.3,0,0],"max":[0.3,2,0]"#,
    );
    let indices = put(
        vec![0, 0, 1, 0, 2, 0, 0, 0, 2, 0, 3, 0],
        5123,
        "SCALAR",
        6,
        "",
    );
    let joints = put(vec![0; 16], 5121, "VEC4", 4, "");
    let weights = put(floats([1.0, 0.0, 0.0, 0.0].repeat(4)), 5126, "VEC4", 4, "");
    let rest = [
        [0.0, 1.0],
        [-0.1, 1.0],
        [-0.1, 0.5],
        [-0.1, 0.0],
        [0.1, 1.0],
        [0.1, 0.5],
        [0.1, 0.0],
    ];
    let inverse_binds = put(
        floats(rest.iter().flat_map(|at| {
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -at[0], -at[1], 0.0,
                1.0,
            ]
        })),
        5126,
        "MAT4",
        7,
        "",
    );
    let times = put(
        floats([0.0, 1.0]),
        5126,
        "SCALAR",
        2,
        r#","min":[0],"max":[1]"#,
    );
    let mut animations = Vec::new();
    for (name, to) in [("Idle", 1.1), ("Walk", 1.2), ("Roll", 0.5)] {
        let values = put(floats([0.0, 1.0, 0.0, 0.0, to, 0.0]), 5126, "VEC3", 2, "");
        animations.push(format!(
            r#"{{"name":"{name}","samplers":[{{"input":{times},"output":{values},"interpolation":"LINEAR"}}],"channels":[{{"sampler":0,"target":{{"node":1,"path":"translation"}}}}]}}"#
        ));
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
"nodes":[
{{"name":"armature","children":[1,8]}},
{{"name":"hips","translation":[0,1,0],"children":[2,5]}},
{{"name":"thigh_left","translation":[-0.1,0,0],"children":[3]}},
{{"name":"shin_left","translation":[0,-0.5,0],"children":[4]}},
{{"name":"foot_left","translation":[0,-0.5,0]}},
{{"name":"thigh_right","translation":[0.1,0,0],"children":[6]}},
{{"name":"shin_right","translation":[0,-0.5,0],"children":[7]}},
{{"name":"foot_right","translation":[0,-0.5,0]}},
{{"name":"body","mesh":0,"skin":0}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":{position},"JOINTS_0":{joints},"WEIGHTS_0":{weights}}},"indices":{indices}}}]}}],
"skins":[{{"joints":[1,2,3,4,5,6,7],"inverseBindMatrices":{inverse_binds}}}],
"animations":[{}],
"buffers":[{{"byteLength":{}}}],"bufferViews":[{}],"accessors":[{}]}}"#,
        animations.join(","),
        bin.len(),
        views.join(","),
        accessors.join(","),
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((json.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(json);
    file.extend((bin.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(bin);
    file
}

fn house_glb() -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../art/modelcheck/house.glb");
    fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

/// Ответ страницы по модели: картинки внутри файла отданы готовыми точками.
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

struct Setup<'a> {
    camera: bool,
    models: &'a str,
    scene: String,
    rules: &'a str,
    code: Option<&'a str>,
}

impl<'a> Setup<'a> {
    fn new(scene: String) -> Setup<'a> {
        Setup {
            camera: true,
            models: MODELS,
            scene,
            rules: r#"{"rules":[]}"#,
            code: None,
        }
    }

    fn load(&self) -> Result<Game, Vec<String>> {
        let camera = if self.camera {
            r#","view_height":16,"camera":{"pitch":55}"#
        } else {
            ""
        };
        let text = format!(
            r##"{{"name":"M","scene":{{"width":40,"height":30,"background":"#000000"{camera}}},
"random_seed":1,"max_objects":512,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{}{}}}}}"##,
            if self.models.is_empty() {
                String::new()
            } else {
                format!(r#","models":{}"#, self.models)
            },
            if self.code.is_some() {
                r#","code":"code.lua""#
            } else {
                ""
            }
        );
        let scene = format!(r#"{{"objects":[{}]}}"#, self.scene);
        let (config, _) = read_entry(&text).map_err(|f| describe(&f.errors))?;
        let files = vec![
            ("rig".to_string(), page_file(rig_glb())),
            ("house".to_string(), page_file(house_glb())),
        ];
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
            self.code,
            false,
            &[],
            None,
            &[],
            &[],
            &[],
            &files,
        )
        .map(|(game, _, _, _)| game)
        .map_err(|failure| describe(&failure.errors))
    }

    fn game(&self) -> Game {
        match self.load() {
            Ok(game) => game,
            Err(errors) => panic!("игра не загрузилась: {errors:#?}"),
        }
    }
}

fn describe(errors: &[GameError]) -> Vec<String> {
    errors
        .iter()
        .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
        .collect()
}

fn expect_error(setup: &Setup<'_>, fragments: &[&str]) {
    let errors = setup
        .load()
        .err()
        .unwrap_or_else(|| panic!("игра должна была не загрузиться: нет ошибки с {fragments:?}"));
    assert!(
        errors
            .iter()
            .any(|e| fragments.iter().all(|f| e.contains(f))),
        "нет ошибки со словами {fragments:?}: {errors:#?}"
    );
}

fn actor(extra: &str) -> String {
    format!(
        r#"{{"name":"actor","model":"rig","actor":true,"position":[5,5],"size":[0.6,0.6],"walk_speed":4{extra}}}"#
    )
}

fn model_of<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель есть")
}

fn actor_id(game: &Game) -> u32 {
    actor_id_in(&game.world)
}

fn step(game: &mut Game, times: usize) {
    for _ in 0..times {
        game.step(StepInput::empty());
    }
}

fn key(game: &mut Game, code: &str) {
    game.key_down(code);
    let input = game.take_input_snapshot();
    game.step(input);
    game.key_up(code);
}

fn clip_of(game: &Game, name: &str) -> Option<usize> {
    model_of(game, "rig")
        .skeleton
        .as_ref()
        .and_then(|s| s.clip_named(name))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

// -------------------------------------------------------------------------------------------
// Загрузка
// -------------------------------------------------------------------------------------------

#[test]
fn the_skeleton_the_motions_and_the_feet_come_from_the_file_and_the_description() {
    let game = Setup::new(actor("")).game();
    let skeleton = model_of(&game, "rig")
        .skeleton
        .as_ref()
        .expect("скелет есть");
    assert_eq!(skeleton.joints.len(), 7);
    assert_eq!(skeleton.clips.len(), 3);
    assert_eq!(skeleton.idle, clip_of(&game, "Idle"));
    assert_eq!(skeleton.walk, clip_of(&game, "Walk"));
    assert!(skeleton.legs.is_some());
    assert!(
        model_of(&game, "house").skeleton.is_none(),
        "дом без скелета"
    );
}

#[test]
fn a_motion_or_a_foot_bone_missing_from_the_file_is_an_error_before_the_start() {
    let cases = [
        (
            r#"{"rig":{"path":"models/rig.glb","idle":"Run"}}"#,
            vec!["idle", "Run", "\"Idle\", \"Walk\""],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","walk":"Fly"}}"#,
            vec!["walk", "Fly"],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","feet":["foot_left"]}}"#,
            vec!["feet", "двух костей"],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","feet":["foot_left","toe"]}}"#,
            vec!["feet", "нет кости", "toe"],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","feet":["foot_left","foot_left"]}}"#,
            vec!["feet", "одна и та же"],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","feet":["hips","foot_right"]}}"#,
            vec!["feet", "голень, бедро и таз"],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","feet":["foot_left","shin_right"]}}"#,
            vec!["feet", "один таз"],
        ),
        (
            r#"{"rig":{"path":"models/rig.glb","idle":3}}"#,
            vec!["idle", "строка"],
        ),
        (
            r#"{"house":{"path":"models/house.glb","idle":"Idle"}}"#,
            vec!["idle", "нет скелета"],
        ),
        (
            r#"{"house":{"path":"models/house.glb","feet":["a","b"]}}"#,
            vec!["feet", "нет скелета"],
        ),
    ];
    for (models, fragments) in cases {
        let mut setup = Setup::new(String::new());
        setup.models = models;
        expect_error(&setup, &fragments);
    }
}

#[test]
fn animation_and_facing_need_a_three_dimensional_scene_and_a_motion_of_the_objects_model() {
    expect_error(&Setup::new(actor(r#","animation":"Fly""#)), &["Fly"]);
    let mut house = Setup::new(
        r#"{"name":"house","model":"house","position":[5,5],"size":[1,1],"animation":"Roll"}"#
            .to_string(),
    );
    expect_error(&house, &["animation", "нет скелета"]);
    let known = Setup::new(actor(r#","animation":"Roll","facing":90"#));
    assert!(known.load().is_ok(), "{:?}", known.load().err());
    house.camera = false;
    house.models = "";
    house.scene =
        r##"{"name":"x","position":[1,1],"size":[1,1],"color":"#ffffff","animation":"Roll"}"##
            .to_string();
    expect_error(&house, &["animation", "трёхмерной сцене"]);
    house.scene =
        r##"{"name":"x","position":[1,1],"size":[1,1],"color":"#ffffff","facing":90}"##.to_string();
    expect_error(&house, &["facing", "трёхмерной сцене"]);
}

// -------------------------------------------------------------------------------------------
// Ход игры
// -------------------------------------------------------------------------------------------

#[test]
fn before_the_first_step_the_object_stands_in_the_first_pose_of_idle() {
    let game = Setup::new(actor("")).game();
    let id = actor_id(&game);
    let skeleton = skeleton_of(&game);
    let blend = game.world.motion(id).blend(&skeleton);
    assert_eq!(blend.current().playing.clip, skeleton.idle);
    assert_eq!(blend.current().playing.time, 0.0);
    assert_eq!(blend.layers().len(), 1);
}

#[test]
fn a_walking_object_walks_and_faces_its_way_then_stands_again_with_a_fade() {
    let mut setup = Setup::new(actor(r#","walk_to":[5.3,25]"#));
    setup.rules = WALK_RULE;
    let mut game = setup.game();
    let id = actor_id(&game);
    let walk = clip_of(&game, "Walk");
    let idle = clip_of(&game, "Idle");
    step(&mut game, 2);
    let skeleton = skeleton_of(&game);
    let blend = game.world.motion(id).blend(&skeleton);
    assert_eq!(
        blend.current().playing.clip,
        walk,
        "сдвинулся за шаг — ходьба"
    );
    assert_eq!(
        blend.layers().get(1).map(|layer| layer.playing.clip),
        Some(idle),
        "переход от стойки"
    );
    assert!(blend.current().weight < 1.0);
    let facing = game.world.rotation(id, property::FACING).map(|r| r.angle());
    let facing = facing.expect("повернулся по ходу").rem_euclid(360.0);
    assert!(
        facing < 1e-6 || facing > 360.0 - 1e-6,
        "шёл вниз сцены, к игроку: {facing}"
    );
    step(&mut game, 60);
    let blend = game.world.motion(id).blend(&skeleton);
    assert_eq!(blend.current().playing.clip, walk);
    assert_eq!(
        blend.layers().len(),
        1,
        "переход кончился за двенадцать шагов"
    );
    let mut guard = 0;
    while game.world.has(id, property::WALK_TO) && guard < 1000 {
        step(&mut game, 1);
        guard += 1;
    }
    step(&mut game, 1);
    let blend = game.world.motion(id).blend(&skeleton);
    assert_eq!(blend.current().playing.clip, idle, "дошёл и встал");
    assert!(blend.layers().len() > 1);
    step(&mut game, 12);
    assert_eq!(game.world.motion(id).blend(&skeleton).layers().len(), 1);
}

#[test]
fn an_object_turns_to_face_the_way_it_went_in_the_measure_of_rotation() {
    let mut setup = Setup::new(actor(r#","walk_to":[9,5.3]"#));
    setup.rules = WALK_RULE;
    let mut game = setup.game();
    let id = actor_id(&game);
    step(&mut game, 3);
    let facing = game.world.rotation(id, property::FACING).map(|r| r.angle());
    assert!(
        facing.is_some_and(|angle| (angle - 270.0).abs() < 1e-6),
        "вправо — 270: {facing:?}"
    );
    let draws = model_draws(&game);
    let model = game.world.models().index_of("rig").unwrap();
    let placement = draws.instances[model][0].placement;
    assert!((placement[2] - 0.0).abs() < 1e-6 && (placement[3] + 1.0).abs() < 1e-6);
}

#[test]
fn a_moving_object_faces_along_its_velocity_and_a_standing_one_keeps_the_facing_it_has() {
    let still = r#"{"name":"still","model":"rig","actor":true,"position":[10,10],"size":[0.6,0.6],"facing":90}"#;
    let scene = format!("{},{still}", actor(r#","velocity":[60,0]"#));
    let mut setup = Setup::new(scene);
    setup.rules = r#"{"rules":[{"kind":"move","for":{"has":["velocity"]}}]}"#;
    let mut game = setup.game();
    let moving = actor_id(&game);
    step(&mut game, 2);
    let angle = |name_id: u32| {
        game.world
            .rotation(name_id, property::FACING)
            .map(|r| r.angle())
    };
    assert_eq!(angle(moving), Some(270.0), "вправо");
    let still = game
        .world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some("still"))
        .expect("в сцене есть still");
    assert_eq!(angle(still), Some(90.0));
}

#[test]
fn animation_starts_from_the_beginning_plays_once_freezes_and_an_empty_value_returns_to_idle() {
    let mut game = Setup::new(actor(KEYED)).game();
    let id = actor_id(&game);
    let roll = clip_of(&game, "Roll");
    step(&mut game, 2);
    key(&mut game, "KeyR");
    let skeleton = skeleton_of(&game);
    let blend = game.world.motion(id).blend(&skeleton);
    let current = blend.current().playing;
    assert_eq!(current.clip, roll);
    assert!(!current.looped, "один раз");
    assert!(current.time < 0.05, "с начала: {}", current.time);
    step(&mut game, 200);
    let current = game.world.motion(id).blend(&skeleton).current().playing;
    assert_eq!(current.clip, roll, "замер на последней позе, а не вернулся");
    let last = pose_at(&skeleton, current.clip, 1.0);
    assert_eq!(pose_at(&skeleton, current.clip, current.time), last);
    key(&mut game, "KeyT");
    let blend = game.world.motion(id).blend(&skeleton);
    assert_eq!(
        blend.current().playing.clip,
        clip_of(&game, "Idle"),
        "пустое значение — к стойке"
    );
    assert_eq!(blend.layers()[1].playing.clip, roll);
}

fn pose_at(
    skeleton: &engine::core::skeleton::Skeleton,
    clip: Option<usize>,
    time: f32,
) -> Vec<[[f32; 4]; 4]> {
    let blend = Blend::single(Playing {
        clip,
        time,
        looped: false,
    });
    let (mut scratch, mut out) = (PoseScratch::default(), Vec::new());
    skeleton.pose(&blend, &mut scratch);
    skeleton.skin_matrices(&scratch, &mut out);
    out
}

#[test]
fn a_game_that_has_not_stepped_keeps_every_model_still() {
    let game = Setup::new(actor("")).game();
    let id = actor_id(&game);
    let skeleton = skeleton_of(&game);
    let before = game.world.motion(id);
    let draws = model_draws(&game);
    let model = game.world.models().index_of("rig").unwrap();
    let flat: Vec<f32> = pose_at(&skeleton, skeleton.idle, 0.0)
        .iter()
        .flatten()
        .flatten()
        .copied()
        .collect();
    assert_eq!(draws.bones[model], flat, "первая поза стойки");
    assert_eq!(game.world.motion(id), before);
}

// -------------------------------------------------------------------------------------------
// Рисование
// -------------------------------------------------------------------------------------------

#[test]
fn a_model_with_a_skeleton_gets_its_joint_matrices_and_a_model_without_one_stays_as_it_was() {
    let scene = format!(
        "{},{}",
        actor(""),
        r#"{"name":"house","model":"house","position":[20,20],"size":[1,1]}"#
    );
    let game = Setup::new(scene).game();
    let draws = model_draws(&game);
    let rig = game.world.models().index_of("rig").unwrap();
    let house = game.world.models().index_of("house").unwrap();
    assert_eq!(draws.bones[rig].len(), 7 * 16);
    assert!(draws.bones[house].is_empty());
    assert_eq!(draws.instances[house].len(), 1);
}

#[test]
fn a_foot_over_a_step_rises_onto_it_the_knee_bends_forward_and_the_game_does_not_change() {
    let step_deck = r##"{"name":"step","position":[4.8,4.8],"size":[0.14,0.4],"deck":true,"shape":"box","height":0.3,"color":"#888888"}"##;
    let narrow =
        r#"{"name":"actor","model":"rig","actor":true,"position":[4.95,4.95],"size":[0.1,0.1]}"#;
    let game = Setup::new(format!("{narrow},{step_deck}")).game();
    let id = actor_id(&game);
    assert_eq!(
        game.world.base_z(id),
        0.0,
        "настил под ступнёй, а не под местом объекта"
    );
    let draws = model_draws(&game);
    let model = game.world.models().index_of("rig").unwrap();
    let bones = &draws.bones[model];
    let left = joint_point(bones, 3, [-0.1, 0.0, 0.0]);
    let right = joint_point(bones, 6, [0.1, 0.0, 0.0]);
    assert!(close(left[2], 0.3), "левая ступня на ступеньке: {left:?}");
    assert!(close(right[2], 0.0), "правая на земле: {right:?}");
    let knee = joint_point(bones, 2, [-0.1, 0.0, 0.5]);
    assert!(knee[1] > 0.05, "колено ушло вперёд: {knee:?}");
    assert_eq!(game.world.base_z(id), 0.0, "ход игры тот же");
}

// -------------------------------------------------------------------------------------------
// Шейдер
// -------------------------------------------------------------------------------------------

fn to_webgl2_glsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry_point: &str,
    bindings: &[(u32, u32)],
) -> String {
    use naga::back::glsl;
    let mut binding_map = glsl::BindingMap::default();
    for (slot, &(group, binding)) in bindings.iter().enumerate() {
        binding_map.insert(naga::ResourceBinding { group, binding }, slot as u8);
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
fn the_skinning_shaders_validate_and_translate_to_glsl_es_300_for_the_fallback_webgl2() {
    let module = naga::front::wgsl::parse_str(include_str!("../shaders/scene3d.wgsl"))
        .expect("WGSL разбирается");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("WGSL проходит проверку");
    for entry in [
        "vs_shadow_model_skinned",
        "vs_shadow_model_mask_skinned",
        "vs_model_skinned",
    ] {
        let glsl = to_webgl2_glsl(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            entry,
            &[(0, 0), (2, 0)],
        );
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
        assert!(
            glsl.contains("texelFetch("),
            "{entry}: матрицы читаются выборкой по целым"
        );
        assert!(
            glsl.contains("gl_InstanceID"),
            "{entry}: строка — номер объекта"
        );
        assert!(
            entry != "vs_model_skinned" || glsl.contains("cross("),
            "{entry}: нормаль сгибается обратной транспонированной через кофакторы"
        );
    }
}

const CODE: &str = r#"
local steps = 0
function roll(obj) obj.animation = "Roll" end
function fly(obj) obj.animation = "Fly" end
function clear(obj) obj.animation = nil end
function nudge(obj) obj.position.x = obj.position.x + 1 end
function stay(obj) obj.position.x = obj.position.x end
function nudge_pair(obj) obj.position = {x = obj.position.x, y = obj.position.y + 1} end
function nudge_then_face(obj)
    obj.position.x = obj.position.x + 1
    obj.facing = 180
end
function to_house(obj) obj.model = "house" end
function to_house_on_the_fourth_step(obj)
    steps = steps + 1
    if steps == 4 then obj.model = "house" end
end
"#;

fn code_game_with(function: &str, extra: &str) -> Game {
    let rules = format!(
        r#"{{"rules":[{{"kind":"check","for":{{"has":["actor"]}},"do":[["run","{function}"]]}}]}}"#
    );
    let mut setup = Setup::new(actor(extra));
    setup.rules = &rules;
    setup.code = Some(CODE);
    setup.game()
}

fn code_game(function: &str) -> Game {
    code_game_with(function, "")
}

#[test]
fn code_writes_a_motion_of_the_model_and_a_motion_it_does_not_have_is_a_code_error() {
    let mut game = code_game("roll");
    let id = actor_id(&game);
    step(&mut game, 3);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.text(id, property::ANIMATION), Some("Roll"));
    let skeleton = skeleton_of(&game);
    assert_eq!(
        game.world
            .motion(id)
            .blend(&skeleton)
            .current()
            .playing
            .clip,
        clip_of(&game, "Roll")
    );
    let mut game = code_game("fly");
    step(&mut game, 1);
    let message = game
        .code_error()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Fly") && message.contains("rig"),
        "{message}"
    );
    let mut game = code_game("clear");
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
}

fn facing_of(game: &Game, id: u32) -> Option<f64> {
    facing_of_in(&game.world, id)
}

fn facing_of_in(world: &engine::core::world::World, id: u32) -> Option<f64> {
    world
        .rotation(id, property::FACING)
        .map(|r| r.angle().rem_euclid(360.0))
}

fn actor_id_in(world: &engine::core::world::World) -> u32 {
    world
        .ids()
        .find(|&id| world.text(id, property::NAME) == Some("actor"))
        .expect("в сцене есть actor")
}

#[test]
fn a_model_changed_by_code_resets_the_motion_and_a_model_without_the_motion_is_a_code_error() {
    let mut game = code_game("to_house_on_the_fourth_step");
    let id = actor_id(&game);
    step(&mut game, 3);
    assert_ne!(
        game.world.motion(id),
        MotionState::default(),
        "стойка играет: время идёт"
    );
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.text(id, property::MODEL), Some("house"));
    assert_eq!(
        game.world.motion(id),
        MotionState::default(),
        "новая модель — движения с начала"
    );
    let mut game = code_game_with("to_house", r#","animation":"Roll""#);
    step(&mut game, 1);
    let message = game
        .code_error()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Roll") && message.contains("house"),
        "модель без движения, названного в animation: {message}"
    );
}

#[test]
fn code_that_writes_position_carries_the_object_without_turning_it() {
    for function in ["nudge", "nudge_pair", "stay"] {
        let mut game = code_game_with(function, r#","facing":90"#);
        let id = actor_id(&game);
        let before = game.world.vec2(id, property::POSITION);
        step(&mut game, 1);
        assert_eq!(
            game.world.vec2(id, property::POSITION) != before,
            function != "stay",
            "{function}"
        );
        assert_eq!(
            facing_of(&game, id),
            Some(90.0),
            "{function}: перенос на место не поворачивает"
        );
    }
    let mut game = code_game_with("nudge_then_face", r#","facing":90"#);
    let id = actor_id(&game);
    step(&mut game, 1);
    assert_eq!(
        facing_of(&game, id),
        Some(180.0),
        "facing, записанный кодом, остаётся"
    );
}

#[test]
fn a_shift_by_nothing_does_not_turn_a_standing_object_and_a_real_shift_does() {
    let shift = |by: &str| {
        let rules = format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["actor"]}},"do":[["shift",{{"group":{{"has":["actor"]}},"by":{by}}}]]}}]}}"#
        );
        let mut setup = Setup::new(actor(r#","facing":90"#));
        setup.rules = &rules;
        let mut game = setup.game();
        let id = actor_id(&game);
        step(&mut game, 1);
        facing_of(&game, id)
    };
    assert_eq!(shift("[0,0]"), Some(90.0));
    assert_eq!(shift("[1,0]"), Some(270.0));
}

#[test]
fn a_rule_carries_without_turning_the_cursor_turns_and_a_shape_keeps_its_facing() {
    let mut setup = Setup::new(actor(r#","facing":90"#));
    setup.rules =
        r#"{"rules":[{"kind":"check","for":{"has":["actor"]},"do":[["set","position",[8,5]]]}]}"#;
    let mut game = setup.game();
    let id = actor_id(&game);
    step(&mut game, 1);
    assert_eq!(
        game.world.vec2(id, property::POSITION),
        Some([8.0, 5.0]),
        "set position перенёс"
    );
    assert_eq!(
        facing_of(&game, id),
        Some(90.0),
        "перенос правилом не поворачивает"
    );

    let puck = r##"{"name":"actor","model":"rig","position":[5,5],"size":[0.6,0.6],"facing":90,"follow_mouse":"xy"}"##;
    let game_with_puck = Setup::new(puck.to_string()).game();
    let mut world = game_with_puck.world;
    let id = actor_id_in(&world);
    engine::core::step::apply_follow_mouse(
        &mut world,
        Some([20.0, 20.0, 0.0]),
        &game_with_puck.scene,
    );
    assert_ne!(
        facing_of_in(&world, id),
        Some(90.0),
        "за курсором повернулся"
    );

    let shape = r##"{"name":"actor","shape":"box","color":"#ffffff","position":[5,5],"size":[0.6,0.6],"velocity":[60,0],"facing":90}"##;
    let mut setup = Setup::new(shape.to_string());
    setup.rules = r#"{"rules":[{"kind":"move","for":{"has":["velocity"]}}]}"#;
    let mut game = setup.game();
    let id = actor_id(&game);
    step(&mut game, 2);
    assert_ne!(game.world.vec2(id, property::POSITION), Some([5.0, 5.0]));
    assert_eq!(
        facing_of(&game, id),
        Some(90.0),
        "фигура сама не поворачивается: её facing меняют данные, правила и код"
    );
}

/// `drive` ставит вид через `rotation`, и `facing`, заданный машине раньше, становится нулём — иначе она ехала бы
/// боком.
#[test]
fn a_driven_model_has_a_zero_facing_and_looks_along_its_road() {
    let road = r##"{"position":[0,9.5],"size":[20,1],"color":"#8a8f96","road":true}"##;
    let car = r#"{"name":"actor","model":"rig","actor":true,"position":[1.9,9.7],"size":[0.2,0.6],"drive_speed":4,"drive_to":[18,10],"facing":90}"#;
    let mut setup = Setup::new(format!("{road},{car}"));
    setup.rules = r#"{"rules":[{"kind":"drive","for":{"has":["actor"]},"side":"middle"}]}"#;
    let mut game = setup.game();
    let id = actor_id(&game);
    step(&mut game, 1);
    assert_eq!(
        game.world
            .rotation(id, property::ROTATION)
            .map(|r| r.angle().rem_euclid(360.0)),
        Some(270.0)
    );
    assert_eq!(facing_of(&game, id), Some(0.0));
}

#[test]
fn the_animation_a_key_or_a_click_writes_is_checked_against_the_model_of_the_object() {
    let keyed = |edits: &str| {
        Setup::new(format!(
            r#"{{"name":"house","model":"house","position":[5,5],"size":[1,1]{edits}}}"#
        ))
    };
    expect_error(
        &keyed(r#","keys":{"KeyR":{"press":[["animation","Roll"]]}}"#),
        &["keys", "KeyR", "нет скелета"],
    );
    expect_error(
        &keyed(r#","on_click":[["animation","Roll"]]"#),
        &["on_click", "нет скелета"],
    );
    let ok = Setup::new(actor(KEYED));
    assert!(ok.load().is_ok(), "{:?}", ok.load().err());
}

#[test]
fn the_animation_of_a_created_object_is_checked_against_the_model_of_its_template() {
    let spawn = |template: &str, pick_one: &str| {
        format!(
            r#"{{"rules":[{{"kind":"spawn","when":{{"fewer_than":{{"count":1,"of":{{"has":["actor"]}}}}}},"where":"random_cell","template":{template}{pick_one}}}]}}"#
        )
    };
    let mut setup = Setup::new(actor(""));
    let good = spawn(
        r#"{"model":"rig","actor":true,"size":[0.6,0.6],"animation":"Roll"}"#,
        "",
    );
    setup.rules = &good;
    assert!(setup.load().is_ok(), "{:?}", setup.load().err());
    let house = spawn(
        r#"{"model":"house","actor":true,"size":[1,1],"animation":"Roll"}"#,
        "",
    );
    setup.rules = &house;
    expect_error(
        &setup,
        &["rules[0]", "template", "animation", "нет скелета"],
    );
    let variant = spawn(
        r#"{"model":"rig","actor":true,"size":[0.6,0.6],"animation":"Roll"}"#,
        r#","pick_one":[{"model":"house","cells":[[3,3]]}]"#,
    );
    setup.rules = &variant;
    expect_error(&setup, &["pick_one", "[0]", "animation", "нет скелета"]);
}

fn skeleton_of(game: &Game) -> engine::core::skeleton::Skeleton {
    model_of(game, "rig")
        .skeleton
        .clone()
        .expect("у модели rig есть скелет")
}

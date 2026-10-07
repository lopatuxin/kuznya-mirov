//! Фаза 30 — свечение, ветер и качание: разбор `glow`, `wind` и `sway` и их проверка перед запуском,
//! что `glow` и наклон доходят до списка рисования, часы движения и кадры качающихся объектов,
//! `wind` в коде, `set_wind_particles` с записью партии и её повтором. Формулы ветра, пружины и
//! изгиба — в `render::wind`'s own `mod tests`; сама видеокарта и браузер — только QA на стенде.

use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::property;
use engine::core::scene::{CellRange, LayerView};
use engine::core::screens::{Fill, ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageDecl, ImageVerdict, load_game_from_texts_with_code, load_rest, read_entry,
};
use engine::data::session::{self, PlaySession};
use engine::render::atlas::{
    AtlasImage, AtlasRect, RectPaint, compose_ground_paints, compose_world_paints, fill_paint,
    pack, sway_objects,
};
use engine::render::wind::{Motion, wind_at};
use serde_json::json;

const VIEWPORT: [f32; 2] = [800.0, 600.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"mark":"flag","hits":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;

fn game_json(scene_extra: &str, files_extra: &str) -> String {
    sized_game_json(40, 20, scene_extra, files_extra)
}

fn sized_game_json(width: u32, height: u32, scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

fn ok_pixels(width: u32, height: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![255u8; (width * height * 4) as usize],
    }
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn load_full(
    game: &str,
    scene: &str,
    rules: &str,
    code: Option<&str>,
    images: &[(&str, ImageVerdict)],
) -> Result<Loaded, LoadFailure> {
    let (config, _warnings) = read_entry(game).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = images
        .iter()
        .map(|(name, verdict)| (name.to_string(), verdict.clone()))
        .collect();
    load_rest(
        game,
        config,
        Some(PROPS),
        Some(scene),
        Some(rules),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &image_data,
        code,
        false,
    )
}

fn load_flat(scene: &str, rules: &str, code: Option<&str>) -> Result<Loaded, LoadFailure> {
    let files = if code.is_some() {
        r#","code":"code.lua""#
    } else {
        ""
    };
    load_full(&game_json("", files), scene, rules, code, &[])
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
// `glow`: разбор
// -------------------------------------------------------------------------------------------

#[test]
fn glow_parses_true_and_false_and_defaults_to_false() {
    let game = game_json(
        "",
        r#","images":{"a":{"path":"a.png","glow":true},"b":{"path":"b.png","glow":false},
        "c":{"path":"c.png"}}"#,
    );
    let (config, _warnings) = read_entry(&game).expect("glow true/false/отсутствие — не ошибка");
    let glow = |name: &str| {
        config
            .files
            .images
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("нет картинки {name}"))
            .glow
    };
    assert!(glow("a"));
    assert!(!glow("b"));
    assert!(!glow("c"), "по умолчанию false");
}

#[test]
fn glow_that_is_not_a_flag_is_reported_with_the_images_own_name() {
    let game = game_json("", r#","images":{"lamp":{"path":"lamp.png","glow":"yes"}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("glow не true/false — ошибка");
    assert_error(&errors, "game.json", "lamp", "признак");
    assert!(errors.iter().any(|e| e.path.contains("glow")), "{errors:?}");
}

// -------------------------------------------------------------------------------------------
// `wind`: разбор и проверка
// -------------------------------------------------------------------------------------------

#[test]
fn wind_loads_as_the_scenes_flat_wind_and_is_calm_without_the_key() {
    let (game, ..) = load_flat(r#"{"objects":[],"wind":[1.5,-0.25]}"#, NO_RULES, None)
        .expect("wind парой чисел — не ошибка");
    assert_eq!(game.wind(), [1.5, -0.25]);
    let (calm, ..) = load_flat(r#"{"objects":[]}"#, NO_RULES, None).unwrap();
    assert_eq!(calm.wind(), [0.0, 0.0]);
}

#[test]
fn wind_that_is_not_a_pair_of_numbers_is_reported_with_file_and_place() {
    for wind in ["[1]", "[1,2,3]", r#""east""#, r#"["a",0]"#] {
        let scene = format!(r#"{{"objects":[],"wind":{wind}}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert!(
            errors
                .iter()
                .any(|e| e.file == "scene.json" && e.path.starts_with("wind")),
            "{wind}: {errors:?}"
        );
    }
}

#[test]
fn wind_in_a_three_dimensional_scene_is_an_error() {
    let game = game_json(CAMERA_3D, "");
    let errors = errors_of(load_full(
        &game,
        r#"{"objects":[],"wind":[1,0]}"#,
        NO_RULES,
        None,
        &[],
    ));
    assert_error(
        &errors,
        "scene.json",
        "wind",
        "wind есть только в плоской сцене: у scene в game.json есть camera",
    );
}

// -------------------------------------------------------------------------------------------
// `sway`: проверка
// -------------------------------------------------------------------------------------------

#[test]
fn sway_below_zero_or_not_a_number_is_an_error() {
    let object = |sway: &str| {
        format!(
            r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff","sway":{sway}}}]}}"##
        )
    };
    let errors = errors_of(load_flat(&object("-1"), NO_RULES, None));
    assert_error(&errors, "scene.json", "sway", "меньше нуля");
    let errors = errors_of(load_flat(&object(r#""a""#), NO_RULES, None));
    assert_error(&errors, "scene.json", "sway", "число");
    assert!(load_flat(&object("0"), NO_RULES, None).is_ok());
}

#[test]
fn sway_in_a_three_dimensional_scene_is_an_error_wherever_it_can_be_written() {
    let game = game_json(CAMERA_3D, "");
    let in_object =
        r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","sway":0.5}]}"##;
    let errors = errors_of(load_full(&game, in_object, NO_RULES, None, &[]));
    assert_error(&errors, "scene.json", "sway", "только в плоской сцене");

    let plain = r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","mark":true}]}"##;
    for rule in [
        r#"{"kind":"check","for":{"has":["mark"]},"do":[["set","sway",0.5]]}"#,
        r#"{"kind":"check","for":{"has":["mark"]},"do":[["add","sway",0.5]]}"#,
    ] {
        let rules = format!(r#"{{"rules":[{rule}]}}"#);
        let errors = errors_of(load_full(&game, plain, &rules, None, &[]));
        assert_error(
            &errors,
            "rules.json",
            "",
            "sway есть только в плоской сцене",
        );
    }
}

#[test]
fn sway_in_a_three_dimensional_scene_is_an_error_in_keys_clicks_and_templates_too() {
    let game = game_json(CAMERA_3D, "");
    for object in [
        r##"{"position":[1,1],"size":[1,1],"color":"#ffffff","keys":{"Space":{"press":[["sway",0.5]]}}}"##,
        r##"{"position":[1,1],"size":[1,1],"color":"#ffffff","on_click":[["sway",0.5]]}"##,
    ] {
        let scene = format!(r#"{{"objects":[{object}]}}"#);
        let errors = errors_of(load_full(&game, &scene, NO_RULES, None, &[]));
        assert_error(
            &errors,
            "scene.json",
            "",
            "sway есть только в плоской сцене",
        );
    }
    let plain = r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","mark":true}]}"##;
    let spawn = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"sway":0.5}}]}"#;
    let errors = errors_of(load_full(&game, plain, spawn, None, &[]));
    assert_error(
        &errors,
        "rules.json",
        "",
        "sway есть только в плоской сцене",
    );
}

#[test]
fn a_rule_may_not_set_a_negative_sway_in_the_data() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","mark":true}]}"##;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","sway",-0.5]]}]}"#;
    let errors = errors_of(load_flat(scene, rules, None));
    assert_error(&errors, "rules.json", "", "sway не может быть меньше нуля");
}

// -------------------------------------------------------------------------------------------
// `wind` и `sway` в коде
// -------------------------------------------------------------------------------------------

const CODE_SCENE: &str = r#"{"objects":[{"position":[1,1],"size":[1,1],"mark":true,"sway":0.3}],
"wind":[1.5,-0.25]}"#;
const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;

fn load_code(code: &str) -> Game {
    let (game, ..) = load_flat(CODE_SCENE, CODE_RULES, Some(code)).expect("должно загрузиться");
    game
}

#[test]
fn code_reads_the_flat_wind_and_sees_a_change_made_in_the_party() {
    let mut game = load_code(r#"function tick(obj) print(wind.x, wind.y) end"#);
    step(&mut game, 1);
    assert_eq!(game.messages().last().unwrap(), "print: 1.5\t-0.25");
    game.set_wind([3.0, 0.0]).expect("плоская сцена");
    step(&mut game, 1);
    assert_eq!(game.messages().last().unwrap(), "print: 3.0\t0.0");
}

#[test]
fn writing_to_wind_is_a_code_error_that_stops_the_game() {
    let mut game = load_code(r#"function tick(obj) wind.x = 3 end"#);
    step(&mut game, 1);
    let error = game.code_error().expect("запись в wind — ошибка кода");
    assert!(
        error.message.contains("ветер только для чтения"),
        "{error:?}"
    );
    assert_eq!(game.wind(), [1.5, -0.25], "ветер не изменился");

    let mut game = load_code(r#"function tick(obj) wind = 5 end"#);
    step(&mut game, 1);
    let error = game
        .code_error()
        .expect("запись в глобальную — ошибка кода");
    assert!(
        error.message.contains("запись в глобальную переменную"),
        "{error:?}"
    );
}

#[test]
fn code_reads_zero_wind_in_a_three_dimensional_game() {
    let game = game_json(CAMERA_3D, r#","code":"code.lua""#);
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let (mut game, ..) = load_full(
        &game,
        scene,
        CODE_RULES,
        Some(r#"function tick(obj) print(wind.x, wind.y) end"#),
        &[],
    )
    .expect("должно загрузиться");
    step(&mut game, 1);
    assert_eq!(game.messages().last().unwrap(), "print: 0.0\t0.0");
    assert_eq!(
        game.set_wind([1.0, 0.0]),
        Err("ветер есть только в плоской сцене".to_string())
    );
}

#[test]
fn code_reads_and_writes_sway_as_a_number_and_a_negative_one_counts_as_zero() {
    let mut game = load_code(
        r#"function tick(obj)
    if obj.sway < 0.35 then obj.sway = obj.sway + 0.2 else obj.sway = -1 end
end"#,
    );
    step(&mut game, 1);
    assert_eq!(game.world.number_like(0, property::SWAY), Some(0.5));
    step(&mut game, 1);
    assert_eq!(game.world.number_like(0, property::SWAY), Some(-1.0));
    assert_eq!(
        sway_objects(&game.world, &[]).count(),
        0,
        "меньше нуля — объект не качается"
    );
}

// -------------------------------------------------------------------------------------------
// Рисование: `glow`, наклон и кадры
// -------------------------------------------------------------------------------------------

const IMAGES: &str = r#","images":{
    "lamp":{"path":"lamp.png","glow":true},
    "plain":{"path":"plain.png"},
    "tiles":{"path":"tiles.png","frames":2,"glow":true},
    "leaf":{"path":"leaf.png","frames":8,"frame_time":0.125},
    "leaf_by":{"path":"leaf_by.png","frames":8,"frame_by":"hits"},
    "grass":{"path":"grass.png","size":[2,4],"anchor":"bottom"}}"#;

fn picture_verdicts() -> Vec<(&'static str, ImageVerdict)> {
    vec![
        ("lamp", ok_pixels(8, 8)),
        ("plain", ok_pixels(8, 8)),
        ("tiles", ok_pixels(16, 8)),
        ("leaf", ok_pixels(64, 8)),
        ("leaf_by", ok_pixels(64, 8)),
        ("grass", ok_pixels(8, 16)),
    ]
}

struct Pictures {
    game: Game,
    images: Vec<ImageDecl>,
    rects: Vec<AtlasRect>,
}

fn load_pictures(scene: &str) -> Pictures {
    load_pictures_in(&game_json("", IMAGES), scene)
}

/// Картинки в атласе лежат в порядке, в котором их отдал загрузчик (`ImageId`).
fn load_pictures_in(game: &str, scene: &str) -> Pictures {
    let (game, _screens, _warnings, images) =
        load_full(game, scene, NO_RULES, None, &picture_verdicts()).expect("должно загрузиться");
    let atlas_images: Vec<AtlasImage> = images
        .iter()
        .map(|decl| {
            let (_, verdict) = picture_verdicts()
                .into_iter()
                .find(|(name, _)| *name == decl.name)
                .expect("картинка теста");
            match verdict {
                ImageVerdict::Ok {
                    width,
                    height,
                    pixels,
                } => AtlasImage {
                    width,
                    height,
                    pixels,
                },
                _ => unreachable!("все картинки теста — ok"),
            }
        })
        .collect();
    let rects = pack(&atlas_images).expect("умещаются").rects;
    Pictures {
        game,
        images,
        rects,
    }
}

impl Pictures {
    fn image_id(&self, name: &str) -> usize {
        self.images
            .iter()
            .position(|decl| decl.name == name)
            .unwrap_or_else(|| panic!("нет картинки {name}"))
    }

    /// Номер кадра, который нарисован у объекта `id`, в ленте картинки `strip` из `frames` кадров.
    fn frame_of(&self, motion: &Motion, id: u32, strip: &str, frames: u32) -> u32 {
        let whole = self.rects[self.image_id(strip)];
        let rect = self.paint_of(motion, id).atlas_rect;
        (rect.x - whole.x) / (whole.w / frames)
    }
}

impl Pictures {
    fn paints(&self, motion: &Motion) -> Vec<RectPaint> {
        compose_world_paints(
            &self.game.world,
            &self.game.scene,
            self.game.world.ids(),
            motion,
            &self.images,
            &self.rects,
            &LayerView::default(),
        )
    }

    fn paint_of(&self, motion: &Motion, id: u32) -> RectPaint {
        self.paints(motion)
            .into_iter()
            .find(|paint| paint.object == Some(id))
            .unwrap_or_else(|| panic!("объект {id} не нарисован"))
    }

    /// Часы движения доведены до `motion` и наклоны посчитаны.
    fn settle(&self, motion: &mut Motion) {
        motion.update(
            self.game.wind()[0],
            sway_objects(&self.game.world, &self.images),
        );
    }
}

#[test]
fn glow_reaches_the_paint_of_an_object_a_ground_tile_and_a_panel() {
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[2,2],"image":"lamp"},
        {"position":[4,1],"size":[2,2],"image":"plain"},
        {"position":[7,1],"size":[2,2],"color":"#ffffff"}],
      "ground":[{"image":"tiles","cells":[[0,1],[1,0]]}]}"##;
    let pictures = load_pictures_in(&sized_game_json(2, 2, "", IMAGES), scene);
    let motion = Motion::default();
    let glow_of = |id| pictures.paint_of(&motion, id).glow;
    assert!(glow_of(0), "картинка со glow");
    assert!(!glow_of(1), "картинка без glow");
    assert!(!glow_of(2), "заливка цветом не светится");

    let visible = CellRange {
        x0: 0,
        y0: 0,
        x1: 2,
        y1: 2,
    };
    let tiles = compose_ground_paints(
        &pictures.game.ground,
        visible,
        &pictures.images,
        &pictures.rects,
    );
    assert_eq!(tiles.len(), 4);
    assert!(tiles.iter().all(|tile| tile.glow), "плитки светятся");

    for (name, expected) in [("lamp", true), ("plain", false)] {
        let fill = Fill::Image {
            image: pictures.image_id(name),
            opacity: 0.5,
        };
        let paint = fill_paint(&fill, 0.0, &pictures.images, &pictures.rects);
        assert_eq!(paint.glow, expected, "панель или кнопка, картинка {name}");
    }
    let color = fill_paint(
        &Fill::Color([1.0; 4]),
        0.0,
        &pictures.images,
        &pictures.rects,
    );
    assert!(!color.glow);
}

#[test]
fn a_swaying_object_leans_with_the_wind_and_its_rectangle_stays() {
    let scene = r##"{"objects":[
        {"position":[10,4],"size":[2,4],"color":"#336633","sway":0.3},
        {"position":[14,4],"size":[2,4],"color":"#336633"}],
      "wind":[1.5,0]}"##;
    let pictures = load_pictures(scene);
    let mut motion = Motion::default();
    for _ in 0..30 {
        motion.tick(None, 0.1);
        pictures.settle(&mut motion);
    }
    let swaying = pictures.paint_of(&motion, 0);
    assert!(swaying.lean > 0.05, "качается вправо по ветру: {swaying:?}");
    assert_eq!(swaying.position, [10.0, 4.0]);
    assert_eq!(swaying.size, [2.0, 4.0], "нижний край и размеры стоят");
    assert_eq!(
        pictures.paint_of(&motion, 1).lean,
        0.0,
        "без sway не качается"
    );
    assert_eq!(
        pictures.game.world.vec2(0, property::POSITION),
        Some([10.0, 4.0]),
        "прямоугольник объекта — тот же"
    );
}

#[test]
fn the_wind_pushes_the_other_way_when_it_blows_the_other_way() {
    let scene = r##"{"objects":[{"position":[10,4],"size":[2,4],"color":"#336633","sway":0.3}],
      "wind":[-2,0]}"##;
    let pictures = load_pictures(scene);
    let mut motion = Motion::default();
    for _ in 0..30 {
        motion.tick(None, 0.1);
        pictures.settle(&mut motion);
    }
    assert!(pictures.paint_of(&motion, 0).lean < -0.05);
}

#[test]
fn repeat_copies_lean_with_their_object() {
    let scene = r##"{"objects":[{"position":[10,4],"size":[2,4],"color":"#336633","sway":0.3,
        "repeat_x":true,"parallax":0.5}],"wind":[1.5,0]}"##;
    let pictures = load_pictures(scene);
    let mut motion = Motion::default();
    for _ in 0..30 {
        motion.tick(None, 0.1);
        pictures.settle(&mut motion);
    }
    let (scale, offset) = pictures.game.camera_frame(VIEWPORT);
    let layers = LayerView::of_frame(&pictures.game.scene, scale, offset, VIEWPORT);
    let paints = compose_world_paints(
        &pictures.game.world,
        &pictures.game.scene,
        pictures.game.world.ids(),
        &motion,
        &pictures.images,
        &pictures.rects,
        &layers,
    );
    assert!(paints.len() > 1, "копии есть: {paints:?}");
    let lean = paints[0].lean;
    assert!(lean > 0.0);
    assert!(paints.iter().all(|paint| paint.lean == lean), "{paints:?}");
}

#[test]
fn the_swayed_height_is_the_drawn_height_after_rotation_and_the_middle_is_of_the_recorded_rectangle()
 {
    let scene = r##"{"objects":[
        {"position":[10,4],"size":[2,1],"image":"grass","sway":0.3},
        {"position":[20,4],"size":[2,1],"image":"grass","sway":0.3,"rotation":90},
        {"position":[30,4],"size":[2,1],"color":"#ffffff","sway":0.3}]}"##;
    let pictures = load_pictures(scene);
    let objects: Vec<_> = sway_objects(&pictures.game.world, &pictures.images).collect();
    assert_eq!(objects.len(), 3);
    assert_eq!(
        (objects[0].x, objects[0].height),
        (11.0, 4.0),
        "картинка 2 × 4"
    );
    assert_eq!(
        (objects[1].x, objects[1].height),
        (21.0, 2.0),
        "повёрнутая картинка: нарисованная высота 2"
    );
    assert_eq!(
        (objects[2].x, objects[2].height),
        (31.0, 1.0),
        "заливка — по размеру"
    );
    assert!(objects.iter().all(|o| o.parallax == 1.0 && o.sway == 0.3));
}

#[test]
fn an_object_without_position_or_size_is_neither_drawn_nor_swayed() {
    let scene = r##"{"objects":[{"size":[2,4],"color":"#336633","sway":0.3},
        {"position":[1,1],"color":"#336633","sway":0.3},
        {"position":[1,1],"size":[1,1],"sway":0.3}]}"##;
    let pictures = load_pictures(scene);
    assert_eq!(
        sway_objects(&pictures.game.world, &pictures.images).count(),
        0
    );
}

#[test]
fn swaying_objects_start_their_frames_from_their_own_place_and_the_others_share_one() {
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[1,1],"image":"leaf","sway":0.3},
        {"position":[3,1],"size":[1,1],"image":"leaf","sway":0.3},
        {"position":[5,1],"size":[1,1],"image":"leaf"},
        {"position":[7,1],"size":[1,1],"image":"leaf"},
        {"position":[9,1],"size":[1,1],"image":"leaf_by","hits":3}]}"##;
    let pictures = load_pictures(scene);
    let frame_of = |motion: &Motion, id| {
        let strip = if id == 4 { "leaf_by" } else { "leaf" };
        pictures.frame_of(motion, id, strip, 8)
    };
    let mut motion = Motion::default();
    pictures.settle(&mut motion);
    assert_eq!(
        frame_of(&motion, 0),
        0,
        "объект номер 0 начинает с начала цикла"
    );
    assert_eq!(frame_of(&motion, 1), 4, "объект номер 1 — с 0,618 цикла");
    assert_eq!(
        frame_of(&motion, 2),
        frame_of(&motion, 3),
        "без sway — один и тот же кадр"
    );
    assert_eq!(frame_of(&motion, 4), 3, "frame_by выбирает свойство");

    // Кадры идут: у объекта без sway — по общим часам, у качающегося — по своим, а frame_by стоит.
    for _ in 0..4 {
        motion.tick(None, 0.1);
        pictures.settle(&mut motion);
    }
    assert_eq!(frame_of(&motion, 2), frame_of(&motion, 3));
    assert_eq!(frame_of(&motion, 4), 3);
    assert_ne!(
        (frame_of(&motion, 0), frame_of(&motion, 1)),
        (0, 4),
        "качающиеся объекты сменили кадр"
    );
}

#[test]
fn frames_by_time_follow_the_editor_clock_outside_a_party_and_the_world_steps_in_one() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"image":"leaf"}]}"##;
    let pictures = load_pictures(scene);
    let frame = |motion: &Motion| pictures.frame_of(motion, 0, "leaf", 8);
    // Вне партии: кадр длится 8 шагов (0,125 с → 7,5 → 8), часы идут по `draw(dt)`.
    let mut motion = Motion::default();
    assert_eq!(frame(&motion), 0);
    motion.tick(None, 0.1);
    assert_eq!(frame(&motion), 0, "6 шагов — ещё первый кадр");
    motion.tick(None, 0.1);
    assert_eq!(frame(&motion), 1, "12 шагов — второй кадр");
    motion.tick(None, 100.0);
    assert!(
        (motion.clock_steps() - 18.0).abs() < 1e-9,
        "кадр редактора двигает часы не больше чем на 0,1 с"
    );

    // В партии: по шагам мира, на паузе стоят, `dt` не учитывается.
    let mut motion = Motion::default();
    motion.tick(Some(16), 5.0);
    assert_eq!(frame(&motion), 2);
    for _ in 0..30 {
        motion.tick(Some(16), 0.1);
    }
    assert_eq!(frame(&motion), 2, "мир стоит — кадры стоят");
    motion.tick(Some(24), 0.0);
    assert_eq!(frame(&motion), 3);
}

// -------------------------------------------------------------------------------------------
// Часы: качание начинается заново
// -------------------------------------------------------------------------------------------

#[test]
fn a_clock_that_went_back_or_far_ahead_restarts_the_lean_without_a_jerk() {
    let scene = r##"{"objects":[{"position":[10,4],"size":[2,4],"color":"#336633","sway":0.3}],
      "wind":[1.5,0]}"##;
    let pictures = load_pictures(scene);
    for jump_to in [10_u64, 900] {
        let mut motion = Motion::default();
        motion.tick(Some(100), 0.0);
        pictures.settle(&mut motion);
        motion.tick(Some(130), 0.0);
        pictures.settle(&mut motion);
        motion.tick(Some(jump_to), 0.0);
        pictures.settle(&mut motion);
        let at = jump_to as f64 / 60.0;
        let target = engine::render::wind::lean_target(0.3, wind_at(1.5, 11.0, at), 4.0);
        let lean = f64::from(pictures.paint_of(&motion, 0).lean);
        assert!(
            (lean - target).abs() < 1e-5,
            "{jump_to}: {lean} против {target}"
        );
    }
}

/// Требование 16: движок не сбрасывает наклоны при перечитывании сцены, но мир, собранный заново,
/// раздаёт номера по порядку и метки у всех 0 — объект, сдвинувшийся на номер удалённого, начинает с
/// цели, а не берёт чужой наклон.
#[test]
fn an_object_that_slid_into_a_deleted_ones_number_starts_on_its_target_not_on_its_lean() {
    let swaying = r##"{"position":[10,4],"size":[2,4],"color":"#336633","sway":0.3}"##;
    let other = r##"{"position":[30,4],"size":[2,4],"image":"grass","sway":0.3}"##;
    let wind = r##""wind":[1.5,0]"##;
    let both = load_pictures(&format!(r#"{{"objects":[{swaying},{other}],{wind}}}"#));
    let mut motion = Motion::default();
    for _ in 0..30 {
        motion.tick(None, 0.1);
        both.settle(&mut motion);
    }
    let first_lean = both.paint_of(&motion, 0).lean;
    let second_lean = both.paint_of(&motion, 1).lean;

    let reread = load_pictures(&format!(r#"{{"objects":[{swaying},{other}],{wind}}}"#));
    motion.world_rebuilt();
    reread.settle(&mut motion);
    assert_eq!(reread.paint_of(&motion, 0).lean, first_lean);
    assert_eq!(reread.paint_of(&motion, 1).lean, second_lean);

    let without_first = load_pictures(&format!(r#"{{"objects":[{other}],{wind}}}"#));
    motion.world_rebuilt();
    without_first.settle(&mut motion);
    let at = motion.clock_steps() / 60.0;
    let target = engine::render::wind::lean_target(0.3, wind_at(1.5, 31.0, at), 4.0);
    let lean = f64::from(without_first.paint_of(&motion, 0).lean);
    assert!((lean - target).abs() < 1e-5, "{lean} против {target}");
    assert!(
        (f64::from(first_lean) - target).abs() > 1e-3,
        "иначе тест ничего не различает: {first_lean} и {target}"
    );
}

/// «Крайние случаи»: правило или код ставит `sway` в нуль в партии — наклон догоняет нулевую цель
/// пружиной, а не пропадает за один кадр.
#[test]
fn a_sway_set_to_zero_in_a_party_straightens_the_plant_by_the_spring() {
    let scene = r##"{"objects":[{"position":[10,4],"size":[2,4],"color":"#336633","sway":0.3}],
      "wind":[1.5,0]}"##;
    let mut pictures = load_pictures(scene);
    let mut motion = Motion::default();
    for _ in 0..30 {
        motion.tick(None, 0.1);
        pictures.settle(&mut motion);
    }
    let before = pictures.paint_of(&motion, 0).lean;
    assert!(before.abs() > 0.05, "{before}");

    pictures.game.world.set_number(0, property::SWAY, 0.0);
    let mut leans = Vec::new();
    for _ in 0..200 {
        motion.tick(None, 0.1);
        pictures.settle(&mut motion);
        leans.push(pictures.paint_of(&motion, 0).lean);
    }
    assert!(
        leans[0] != 0.0 && (leans[0] - before).abs() < before.abs() * 0.5,
        "наклон не прыгает в нуль: {before} -> {}",
        leans[0]
    );
    assert!(leans.last().is_some_and(|lean| lean.abs() < 1e-3));
    let rest = leans.iter().position(|lean| lean.abs() < 1e-3).unwrap();
    assert!(rest > 10, "пружина идёт дольше секунды: кадр {rest}");
}

// -------------------------------------------------------------------------------------------
// `set_wind_particles`, запись партии и повтор
// -------------------------------------------------------------------------------------------

fn live_game() -> (Game, ScreensConfig) {
    let scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"mark":true}],"wind":[1.5,0]}"#;
    let (game, screens, _warnings, _images) = load_full(
        &game_json("", r#","code":"code.lua""#),
        scene,
        CODE_RULES,
        Some(r#"function tick(obj) print(wind.x) end"#),
        &[],
    )
    .expect("должно загрузиться");
    (game, screens)
}

fn advance(
    session: &mut PlaySession,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.step_once(&mut queue, &mut mouse, game, config, state, VIEWPORT, &[]);
}

fn seek(
    session: &mut PlaySession,
    target: u64,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.seek(
        target,
        &mut queue,
        &mut mouse,
        game,
        config,
        state,
        VIEWPORT,
        &[],
    );
}

#[test]
fn outside_a_party_the_wind_goes_to_the_built_world_and_nothing_is_recorded() {
    let (mut game, config) = live_game();
    assert_eq!(
        session::set_wind(None, &mut game, &json!([-2, 0.5])),
        Ok(())
    );
    assert_eq!(game.wind(), [-2.0, 0.5]);

    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    assert_eq!(game.wind(), [1.5, 0.0], "«Запуск» берёт ветер файла");
    live.end(&mut game);
    assert_eq!(
        session::set_wind(Some(&mut live), &mut game, &json!([4, 0])),
        Ok(())
    );
    assert!(
        !live.recording_text(&game).contains("wind"),
        "после «Стопа» ветер не пишется в запись"
    );
    game.show_scene();
    assert_eq!(game.wind(), [1.5, 0.0], "«Стоп» возвращает ветер файла");
}

#[test]
fn a_wind_that_is_not_a_pair_of_finite_numbers_is_refused() {
    let (mut game, _config) = live_game();
    for wind in [
        json!([1]),
        json!("east"),
        json!(null),
        json!([1, null]),
        json!([1, "a"]),
        json!([1, 2, 3]),
    ] {
        assert_eq!(
            session::set_wind(None, &mut game, &wind),
            Err("wind должен быть парой конечных чисел".to_string()),
            "{wind}"
        );
    }
    assert_eq!(game.wind(), [1.5, 0.0]);
}

#[test]
fn the_wind_set_in_a_party_is_recorded_and_the_replay_gives_the_same_wind_at_the_same_step() {
    let (mut game, config) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    for _ in 0..3 {
        advance(&mut live, &mut game, &config, &mut state);
    }
    assert_eq!(
        session::set_wind(Some(&mut live), &mut game, &json!([3, 0])),
        Ok(())
    );
    assert_eq!(game.wind(), [3.0, 0.0]);
    for _ in 0..2 {
        advance(&mut live, &mut game, &config, &mut state);
    }
    let live_messages: Vec<String> = game.messages().to_vec();
    assert_eq!(
        live_messages,
        [
            "print: 1.5",
            "print: 1.5",
            "print: 1.5",
            "print: 3.0",
            "print: 3.0"
        ],
        "код видит новый ветер со следующего шага"
    );
    let text = live.recording_text(&game);
    assert!(
        text.contains(r#""step":3"#) && text.contains(r#""wind":[3.0,0.0]"#),
        "{text}"
    );

    let (mut replayed, config2) = live_game();
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut replayed, &config2, &mut state2).expect("запись");
    let mut seen = Vec::new();
    for _ in 0..5 {
        advance(&mut replay, &mut replayed, &config2, &mut state2);
        seen.push(replayed.wind());
    }
    assert_eq!(
        seen,
        [[1.5, 0.0], [1.5, 0.0], [1.5, 0.0], [3.0, 0.0], [3.0, 0.0]],
        "ветер встаёт на том же шаге"
    );
    assert_eq!(replayed.messages(), live_messages, "повтор печатает то же");

    seek(&mut replay, 3, &mut replayed, &config2, &mut state2);
    assert_eq!(replayed.wind(), [1.5, 0.0], "шаг 3 — ещё прежний ветер");
    seek(&mut replay, 4, &mut replayed, &config2, &mut state2);
    assert_eq!(replayed.wind(), [3.0, 0.0]);
    seek(&mut replay, 5, &mut replayed, &config2, &mut state2);
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    replay.step_back(
        &mut queue,
        &mut mouse,
        &mut replayed,
        &config2,
        &mut state2,
        VIEWPORT,
        &[],
    );
    assert_eq!(
        replayed.wind(),
        [3.0, 0.0],
        "шаг назад на шаг 4: ветер уже новый"
    );
    seek(&mut replay, 0, &mut replayed, &config2, &mut state2);
    assert_eq!(replayed.wind(), [1.5, 0.0]);
}

#[test]
fn in_a_replay_the_world_is_not_edited() {
    let (mut game, config) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    advance(&mut live, &mut game, &config, &mut state);
    let text = live.recording_text(&game);

    let (mut replayed, config2) = live_game();
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut replayed, &config2, &mut state2).unwrap();
    assert_eq!(
        session::set_wind(Some(&mut replay), &mut replayed, &json!([3, 0])),
        Err("в повторе мир не правится".to_string())
    );
    assert_eq!(replayed.wind(), [1.5, 0.0]);
}

#[test]
fn a_recording_without_wind_events_reads_as_before() {
    let (mut game, config) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    advance(&mut live, &mut game, &config, &mut state);
    let text = live.recording_text(&game);
    assert!(!text.contains("wind"));
    let (mut replayed, config2) = live_game();
    let mut state2 = ScreenState::new(config2.start_screen);
    PlaySession::begin_replay(&text, &mut replayed, &config2, &mut state2).expect("читается");
}

#[test]
fn a_wind_event_in_a_three_dimensional_game_is_skipped_and_the_replay_goes_on() {
    let game_text = game_json(CAMERA_3D, r#","code":"code.lua""#);
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let load3d = || {
        load_full(
            &game_text,
            scene,
            CODE_RULES,
            Some(r#"function tick(obj) print(wind.x) end"#),
            &[],
        )
        .map(|(game, screens, ..)| (game, screens))
        .expect("должно загрузиться")
    };
    let recording = r#"{"format":1,"steps":3,"events":[{"step":1,"wind":[3.0,0.0]}]}"#;
    let (mut game, config) = load3d();
    let mut state = ScreenState::new(config.start_screen);
    let mut replay = PlaySession::begin_replay(recording, &mut game, &config, &mut state)
        .expect("запись с событием ветра читается");
    for _ in 0..3 {
        advance(&mut replay, &mut game, &config, &mut state);
    }
    assert_eq!(game.session_step_count(), 3, "повтор идёт");
    assert_eq!(game.wind(), [0.0, 0.0]);
    assert!(game.code_error().is_none());

    let (mut game, _config) = load3d();
    assert_eq!(
        session::set_wind(None, &mut game, &json!([1, 0])),
        Err("ветер есть только в плоской сцене".to_string())
    );
}

#[test]
fn a_wind_edit_does_not_change_the_course_of_the_party_without_code() {
    let scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"mark":true,"velocity":[60,0]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let run = |wind: bool| {
        let (mut game, ..) = load_flat(scene, rules, None).unwrap();
        for index in 0..10 {
            if wind && index == 4 {
                game.set_wind([5.0, 5.0]).unwrap();
            }
            game.step(StepInput::empty());
        }
        game.world.vec2(0, property::POSITION)
    };
    assert_eq!(run(true), run(false));
}

#[test]
fn the_prestart_check_accepts_sway_and_glow_in_a_flat_game_without_warnings() {
    let game = load_game_from_texts_with_code(
        &game_json("", ""),
        r#"{"properties":{}}"#,
        r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","sway":0.25}],"wind":[0,0]}"##,
        NO_RULES,
        SCREENS,
        None,
    );
    let (_game, _screens, warnings) = game.expect("должно загрузиться");
    assert_eq!(warnings, Vec::new());
}

//! Фаза 33 — видео на объекте: разбор `.mp4` в `files.images`, ошибки файла и использования видео,
//! место видео в атласе, шейдер переноса кадра. Сам проигрыватель, видеокарта и часы кадров в
//! браузере — только QA на стенде; часы движения — в `render::wind`'s own `mod tests`.

use engine::core::game::Game;
use engine::core::scene::LayerView;
use engine::core::screens::ScreensConfig;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest, read_entry};
use engine::render::atlas::{
    ATLAS_SIZE, AtlasImage, SHEET_BYTES, WHITE_PIXEL, compose_world_paints, fill_sheet, pack,
};
use engine::render::wind::Motion;

const PROPS: &str = r#"{"properties":{}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const NO_OBJECTS: &str = r#"{"objects":[]}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;

fn game_json(scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"fonts/ui.ttf"}}{files_extra}}}}}"##
    )
}

/// Файл видео двойной высоты: оба слоя вместе.
fn video(width: u32, height: u32) -> ImageVerdict {
    ImageVerdict::Video { width, height }
}

fn picture(width: u32, height: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn load_in(
    game: &str,
    scene: &str,
    screens: &str,
    images: &[(&str, ImageVerdict)],
) -> Result<Loaded, LoadFailure> {
    let (config, _warnings) = read_entry(game).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = images
        .iter()
        .map(|(name, verdict)| (name.to_string(), verdict.clone()))
        .collect();
    let fonts = vec![("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))];
    load_rest(
        game,
        config,
        Some(PROPS),
        Some(scene),
        Some(NO_RULES),
        Some(screens),
        &fonts,
        &[],
        &[],
        &image_data,
        None,
        false,
    )
}

fn load_flat(
    files_extra: &str,
    scene: &str,
    images: &[(&str, ImageVerdict)],
) -> Result<Loaded, LoadFailure> {
    load_in(&game_json("", files_extra), scene, SCREENS, images)
}

fn errors_of(result: Result<Loaded, LoadFailure>) -> Vec<GameError> {
    result.expect_err("загрузка должна провалиться").errors
}

fn assert_error(errors: &[GameError], path: &str, message: &str) {
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains(path) && e.message.contains(message)),
        "нет ошибки {path}: {message}: {errors:?}"
    );
}

const GRASS: &str = r#","images":{"grass":{"path":"images/grass.mp4"}}"#;
const GRASS_SCENE: &str = r#"{"objects":[{"position":[2,2],"size":[4,2],"image":"grass"}]}"#;

// -------------------------------------------------------------------------------------------
// Разбор
// -------------------------------------------------------------------------------------------

#[test]
fn an_mp4_path_is_a_video_and_a_png_path_is_a_picture() {
    let game = game_json(
        "",
        r#","images":{"grass":{"path":"images/grass.mp4"},"loud":{"path":"images/LOUD.MP4"},
        "head":{"path":"images/head.png"}}"#,
    );
    let (config, _warnings) = read_entry(&game).expect("видео и картинка вместе — не ошибка");
    let is_video = |name: &str| {
        config
            .files
            .images
            .iter()
            .find(|decl| decl.name == name)
            .unwrap_or_else(|| panic!("нет картинки {name}"))
            .video
    };
    assert!(is_video("grass"));
    assert!(is_video("loud"), "окончание без учёта регистра");
    assert!(!is_video("head"));
}

#[test]
fn a_video_has_one_motionless_frame_of_its_own_and_takes_the_picture_keys() {
    let game = game_json(
        "",
        r#","images":{"grass":{"path":"images/grass.mp4","size":[8,4],"anchor":"bottom",
        "offset":[0.5,0],"smooth":true,"glow":true}}"#,
    );
    let (config, _warnings) = read_entry(&game).expect("ключи видео — не ошибка");
    let decl = &config.files.images[0];
    assert_eq!((decl.frames, decl.columns, decl.frame_by), (1, None, None));
    assert!(!decl.animated);
    assert_eq!(decl.size, Some([8.0, 4.0]));
    assert_eq!(decl.offset, [0.5, 0.0]);
    assert!(decl.smooth && decl.glow);
}

#[test]
fn frame_keys_on_a_video_are_errors_each_under_its_own_key() {
    for (key, value) in [
        ("frames", "4"),
        ("columns", "2"),
        ("frame_time", "0.1"),
        ("frame_by", r#""hits""#),
    ] {
        let game = game_json(
            "",
            &format!(r#","images":{{"grass":{{"path":"images/grass.mp4","{key}":{value}}}}}"#),
        );
        let LoadFailure { errors, .. } =
            read_entry(&game).expect_err("ключ кадров у видео — ошибка");
        assert_error(&errors, &format!("grass → {key}"), "у видео не бывает");
    }
}

#[test]
fn every_frame_key_of_a_video_is_reported_in_one_pass() {
    let game = game_json(
        "",
        r#","images":{"grass":{"path":"images/grass.mp4","frames":4,"columns":2,
        "frame_time":0.1}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("ключи кадров у видео");
    assert_eq!(errors.len(), 3, "{errors:?}");
}

#[test]
fn a_path_that_is_neither_png_nor_mp4_names_both_endings() {
    let game = game_json("", r#","images":{"anim":{"path":"images/anim.gif"}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("чужое окончание — ошибка");
    assert_error(&errors, "anim → path", "anim.gif");
    assert_error(&errors, "anim → path", ".mp4");
}

// -------------------------------------------------------------------------------------------
// Ошибки файла
// -------------------------------------------------------------------------------------------

#[test]
fn a_video_the_page_found_and_the_browser_plays_loads_without_warnings() {
    let (game, _screens, warnings, images) =
        load_flat(GRASS, GRASS_SCENE, &[("grass", video(1280, 996))]).expect("видео грузится");
    assert_eq!(warnings, Vec::new());
    assert!(images[0].video);
    assert_eq!(game.world.image(0, engine::core::property::IMAGE), Some(0));
}

#[test]
fn a_missing_or_rejected_video_file_is_a_file_error() {
    let errors = errors_of(load_flat(
        GRASS,
        NO_OBJECTS,
        &[("grass", ImageVerdict::Missing)],
    ));
    assert_error(&errors, "files → images → grass", "не найден");
    assert_error(&errors, "files → images → grass", "видео MP4");
    let errors = errors_of(load_flat(
        GRASS,
        NO_OBJECTS,
        &[("grass", ImageVerdict::Rejected)],
    ));
    assert_error(&errors, "files → images → grass", "не берётся играть");
    let errors = errors_of(load_flat(GRASS, NO_OBJECTS, &[]));
    assert_error(&errors, "files → images → grass", "не найден");
}

#[test]
fn an_odd_file_height_is_a_file_error() {
    let errors = errors_of(load_flat(GRASS, NO_OBJECTS, &[("grass", video(1280, 997))]));
    assert_error(&errors, "files → images → grass", "нечётная");
}

#[test]
fn a_frame_wider_or_taller_than_a_sheet_is_a_file_error() {
    for (width, height) in [(2050, 996), (1280, 4098)] {
        let errors = errors_of(load_flat(
            GRASS,
            NO_OBJECTS,
            &[("grass", video(width, height))],
        ));
        assert_error(&errors, "files → images → grass", "шире или выше 2048");
    }
}

#[test]
fn a_frame_of_exactly_one_sheet_is_allowed() {
    let result = load_flat(GRASS, GRASS_SCENE, &[("grass", video(2048, 4096))]);
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn a_video_of_no_size_is_a_file_error() {
    let errors = errors_of(load_flat(GRASS, NO_OBJECTS, &[("grass", video(0, 0))]));
    assert_error(&errors, "files → images → grass", "больше нуля");
}

#[test]
fn the_wrong_kind_of_answer_for_a_file_is_a_file_error() {
    let errors = errors_of(load_flat(GRASS, NO_OBJECTS, &[("grass", picture(8, 8))]));
    assert_error(&errors, "files → images → grass", "картинкой");
    let files = r#","images":{"head":{"path":"images/head.png"}}"#;
    let errors = errors_of(load_flat(files, NO_OBJECTS, &[("head", video(8, 8))]));
    assert_error(&errors, "files → images → head", "видео");
}

#[test]
fn a_declared_video_nobody_names_is_a_warning_like_a_picture() {
    let (_game, _screens, warnings, _images) =
        load_flat(GRASS, NO_OBJECTS, &[("grass", video(1280, 996))]).expect("видео грузится");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("grass") && w.message.contains("не называет")),
        "{warnings:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Ошибки использования
// -------------------------------------------------------------------------------------------

fn screens_with(element: &str) -> String {
    format!(r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[{element}]}}]}}"#)
}

#[test]
fn a_video_on_a_panel_is_a_usage_error() {
    let screens = screens_with(
        r#"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[10,10],"image":"grass"}"#,
    );
    let errors = errors_of(load_in(
        &game_json("", GRASS),
        NO_OBJECTS,
        &screens,
        &[("grass", video(1280, 996))],
    ));
    assert_error(&errors, "image", "панель и кнопка её не берут");
}

#[test]
fn a_video_in_any_of_the_three_button_images_is_a_usage_error() {
    let files =
        r#","images":{"grass":{"path":"images/grass.mp4"},"head":{"path":"images/head.png"}}"#;
    for fields in [
        r#""image":"grass""#,
        r#""image":"head","image_hover":"grass""#,
        r#""image":"head","image_pressed":"grass""#,
    ] {
        let screens = screens_with(&format!(
            r#"{{"kind":"button","anchor":"top_left","offset":[0,0],"size":[10,10],
            "text":"Играть","font":"ui","on_click":["quit"],{fields}}}"#
        ));
        let errors = errors_of(load_in(
            &game_json("", files),
            NO_OBJECTS,
            &screens,
            &[("grass", video(1280, 996)), ("head", picture(8, 8))],
        ));
        assert_error(&errors, "image", "панель и кнопка её не берут");
    }
}

#[test]
fn a_video_as_a_ground_tile_set_is_a_usage_error() {
    let scene = r#"{"objects":[],"ground":[{"image":"grass","cells":[[0]]}]}"#;
    let game = r##"{"name":"T","scene":{"width":1,"height":1,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"fonts/ui.ttf"},
         "images":{"grass":{"path":"images/grass.mp4"}}}}"##;
    let errors = errors_of(load_in(
        game,
        scene,
        SCREENS,
        &[("grass", video(1280, 996))],
    ));
    assert_error(&errors, "ground[0]", "земля берёт только набор плиток");
}

#[test]
fn any_video_in_a_three_dimensional_scene_is_a_usage_error() {
    let errors = errors_of(load_in(
        &game_json(CAMERA_3D, GRASS),
        NO_OBJECTS,
        SCREENS,
        &[("grass", video(1280, 996))],
    ));
    assert_error(&errors, "files → images → grass", "трёхмерной сцене");
}

// -------------------------------------------------------------------------------------------
// Место в атласе
// -------------------------------------------------------------------------------------------

#[test]
fn a_video_takes_the_place_of_one_frame_not_of_the_file() {
    let atlas = pack(&[
        AtlasImage::video_frame(1280, 498),
        AtlasImage {
            width: 64,
            height: 32,
            pixels: vec![9u8; 64 * 32 * 4],
        },
    ])
    .expect("умещаются");
    assert_eq!((atlas.rects[0].w, atlas.rects[0].h), (1280, 498));
    assert_eq!(atlas.sheet_count, 1);
}

#[test]
fn the_place_of_a_video_stays_transparent_and_its_neighbours_are_drawn() {
    let images = [
        AtlasImage::video_frame(8, 6),
        AtlasImage {
            width: 4,
            height: 4,
            pixels: [200u8, 100, 50, 255].repeat(16),
        },
    ];
    let atlas = pack(&images).expect("умещаются");
    let mut sheet = vec![1u8; SHEET_BYTES];
    fill_sheet(&atlas, &images, 0, &mut sheet);
    let at = |x: u32, y: u32| {
        let start = ((y * ATLAS_SIZE + x) * 4) as usize;
        sheet[start..start + 4].to_vec()
    };
    let place = atlas.rects[0];
    let drawn = atlas.rects[1];
    for (x, y) in [
        (place.x, place.y),
        (place.x + place.w - 1, place.y + place.h - 1),
    ] {
        assert_eq!(at(x, y), vec![0, 0, 0, 0], "место видео {x}×{y}");
    }
    assert_eq!(at(drawn.x, drawn.y), vec![200, 100, 50, 255]);
    assert_eq!(at(WHITE_PIXEL.x, WHITE_PIXEL.y), vec![255, 255, 255, 255]);
}

#[test]
fn videos_and_pictures_together_fit_the_sixteen_sheets_and_not_one_more() {
    let sheet = || AtlasImage::video_frame(ATLAS_SIZE, ATLAS_SIZE);
    let mut images = vec![AtlasImage {
        width: ATLAS_SIZE,
        height: ATLAS_SIZE,
        pixels: vec![5u8; SHEET_BYTES],
    }];
    images.extend((0..13).map(|_| sheet()));
    images.push(AtlasImage::video_frame(1280, 498));
    let atlas = pack(&images).expect("шестнадцать листов умещаются");
    assert_eq!(atlas.sheet_count, 16);

    images.push(sheet());
    let message = pack(&images).expect_err("семнадцатый лист не умещается");
    assert!(message.contains("16"), "{message}");
}

#[test]
fn an_object_draws_the_whole_frame_place_of_its_video() {
    let (game, _screens, _warnings, images) =
        load_flat(GRASS, GRASS_SCENE, &[("grass", video(1280, 996))]).expect("видео грузится");
    let atlas = pack(&[AtlasImage::video_frame(1280, 498)]).expect("умещается");
    let paints = compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        &Motion::default(),
        &images,
        &atlas.rects,
        &LayerView::default(),
    );
    assert_eq!(paints.len(), 1);
    assert_eq!(paints[0].atlas_rect, atlas.rects[0]);
    assert_eq!(
        paints[0].size,
        [4.0, 2.0],
        "размер объекта без size картинки"
    );
}

#[test]
fn a_video_with_its_own_size_is_drawn_at_that_size() {
    let files =
        r#","images":{"grass":{"path":"images/grass.mp4","size":[12.5,6],"anchor":"bottom"}}"#;
    let (game, _screens, _warnings, images) =
        load_flat(files, GRASS_SCENE, &[("grass", video(1280, 996))]).expect("видео грузится");
    let atlas = pack(&[AtlasImage::video_frame(1280, 498)]).expect("умещается");
    let paints = compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        &Motion::default(),
        &images,
        &atlas.rects,
        &LayerView::default(),
    );
    assert_eq!(paints[0].size, [12.5, 6.0]);
    assert_eq!(paints[0].atlas_rect, atlas.rects[0]);
}

// -------------------------------------------------------------------------------------------
// Шейдер
// -------------------------------------------------------------------------------------------

#[test]
fn the_video_shader_validates_and_translates_to_glsl_es_300() {
    use naga::back::glsl;
    let source = include_str!("../shaders/video.wgsl");
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
        let mut binding_map = glsl::BindingMap::default();
        binding_map.insert(
            naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            0,
        );
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
            entry_point: entry.to_string(),
            multiview: None,
        };
        let mut out = String::new();
        glsl::Writer::new(
            &mut out,
            &module,
            &info,
            &options,
            &pipeline,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .unwrap_or_else(|e| panic!("{entry}: писатель GLSL не создан: {e}"))
        .write()
        .unwrap_or_else(|e| panic!("{entry}: GLSL не записан: {e}"));
        assert!(out.starts_with("#version 300 es"), "{entry}: {out}");
    }
}

#[test]
fn the_video_shader_takes_the_mask_brightness_not_one_channel() {
    let shader = include_str!("../shaders/video.wgsl");
    assert!(
        shader.contains("vec3<f32>(0.299, 0.587, 0.114)"),
        "{shader}"
    );
    assert!(
        shader.contains("color.rgb * alpha"),
        "цвет умножен на яркость маски"
    );
}

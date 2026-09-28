//! Фаза 13 — крупные картинки, кадры сеткой и земля из плиток (требования 1–24). Игр в `games/`
//! не заводит — каждый тест собирает свою маленькую игру в коде, как
//! `tests/phase12_world_labels_tables.rs`. Unit-тесты самого расчёта (сетка кадров, прямоугольник
//! своего размера и его поворот, плитки земли) живут в `render::atlas`'s own `mod tests`; здесь —
//! сквозная проверка через весь конвейер: разбор `game.json`/`scene.json`, проверки перед запуском,
//! предупреждения, и (в одном тесте) настоящая сборка атласа.

use std::time::Instant;

use engine::core::game::Game;
use engine::core::scene::{CellRange, GroundLayer};
use engine::core::screens::ScreensConfig;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageVerdict, load_rest, read_entry};
use engine::render::atlas::{self, AtlasImage, AtlasRect};

fn game_json(width: u32, height: u32, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_MAIN: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

fn ok_pixels(width: u32, height: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

#[allow(clippy::too_many_arguments)]
fn load(
    game_json: &str,
    scene: &str,
    images: &[(&str, ImageVerdict)],
) -> Result<
    (
        Game,
        ScreensConfig,
        Vec<GameError>,
        Vec<engine::data::load::ImageDecl>,
    ),
    LoadFailure,
> {
    let (config, _entry_warnings) = read_entry(game_json).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = images
        .iter()
        .map(|(name, verdict)| (name.to_string(), verdict.clone()))
        .collect();
    load_rest(
        game_json,
        config,
        Some(PROPS_EMPTY),
        Some(scene),
        Some(RULES_EMPTY),
        Some(SCREENS_MAIN),
        &[],
        &[],
        &[],
        &image_data,
        None,
        false,
    )
}

fn err_messages(result: &Result<impl std::fmt::Debug, LoadFailure>) -> Vec<String> {
    match result {
        Err(f) => f.errors.iter().map(|e| e.message.clone()).collect(),
        Ok(_) => Vec::new(),
    }
}

fn located_errors(
    result: &Result<impl std::fmt::Debug, LoadFailure>,
) -> Vec<(String, String, String)> {
    match result {
        Err(f) => f
            .errors
            .iter()
            .map(|e| (e.file.clone(), e.path.clone(), e.message.clone()))
            .collect(),
        Ok(_) => Vec::new(),
    }
}

// -------------------------------------------------------------------------------------------
// Описание картинки: columns, size, anchor, offset — проверка перед запуском (game.json уровень,
// read_entry уже видит эти ошибки)
// -------------------------------------------------------------------------------------------

#[test]
fn columns_not_an_integer_from_one_is_reported() {
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","frames":4,"columns":0}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("columns 0 — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("columns")),
        "{errors:?}"
    );
}

#[test]
fn columns_greater_than_frames_is_reported() {
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","frames":4,"columns":5}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("columns > frames — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("columns") && e.message.contains("frames")),
        "{errors:?}"
    );
}

#[test]
fn columns_without_frames_is_reported() {
    let game = game_json(4, 4, r#","images":{"g":{"path":"g.png","columns":2}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("columns без frames — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("frames")),
        "{errors:?}"
    );
}

#[test]
fn size_not_a_pair_of_positive_numbers_is_reported() {
    for size in ["[0, 1]", "[1, -1]", "[1]", "\"x\""] {
        let game = game_json(
            4,
            4,
            &format!(r#","images":{{"g":{{"path":"g.png","size":{size}}}}}"#),
        );
        let LoadFailure { errors, .. } = read_entry(&game).expect_err("плохой size — ошибка");
        assert!(!errors.is_empty(), "size={size}: {errors:?}");
    }
}

#[test]
fn unknown_anchor_on_an_image_is_reported() {
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","size":[1,1],"anchor":"middle"}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("неизвестный anchor — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("middle")),
        "{errors:?}"
    );
}

#[test]
fn offset_not_a_pair_of_numbers_on_an_image_is_reported() {
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","size":[1,1],"offset":[1]}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("плохой offset — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("пара")),
        "{errors:?}"
    );
}

#[test]
fn anchor_without_size_is_reported() {
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","anchor":"center"}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("anchor без size — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("size")),
        "{errors:?}"
    );
}

#[test]
fn offset_without_size_is_reported() {
    let game = game_json(4, 4, r#","images":{"g":{"path":"g.png","offset":[0,0]}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("offset без size — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("size")),
        "{errors:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Делимость по сетке — нужна настоящая картинка, значит полный load
// -------------------------------------------------------------------------------------------

#[test]
fn width_not_divisible_by_columns_is_reported() {
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","frames":4,"columns":3}}"#,
    );
    let scene = r#"{"objects":[]}"#;
    let result = load(&game, scene, &[("g", ok_pixels(10, 10))]);
    let errors = err_messages(&result);
    assert!(
        errors
            .iter()
            .any(|m| m.contains("columns") && m.contains("делится")),
        "{errors:?}"
    );
}

#[test]
fn height_not_divisible_by_grid_rows_is_reported() {
    // frames:4, columns:2 → 2 строки; высота 15 на 2 не делится.
    let game = game_json(
        4,
        4,
        r#","images":{"g":{"path":"g.png","frames":4,"columns":2}}"#,
    );
    let scene = r#"{"objects":[]}"#;
    let result = load(&game, scene, &[("g", ok_pixels(10, 15))]);
    let errors = err_messages(&result);
    assert!(
        errors
            .iter()
            .any(|m| m.contains("строк") && m.contains("делится")),
        "{errors:?}"
    );
}

#[test]
fn size_on_a_panel_image_is_reported_the_same_way_as_frame_by() {
    let game = game_json(4, 4, r#","images":{"g":{"path":"g.png","size":[1,1]}}"#);
    let scene = r#"{"objects":[]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[10,10],"image":"g"}
    ]}]}"##;
    let (config, _w) = read_entry(&game).expect("должен разбираться");
    let result = load_rest(
        &game,
        config,
        Some(PROPS_EMPTY),
        Some(scene),
        Some(RULES_EMPTY),
        Some(screens),
        &[],
        &[],
        &[],
        &[("g".to_string(), ok_pixels(10, 10))],
        None,
        false,
    );
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("size")), "{errors:?}");
}

// -------------------------------------------------------------------------------------------
// scene.json → ground: разбор и проверка
// -------------------------------------------------------------------------------------------

const TILESET: &str = r#","images":{"terrain":{"path":"terrain.png","frames":4,"columns":2}}"#;

fn tiles_ok() -> Vec<(&'static str, ImageVerdict)> {
    vec![("terrain", ok_pixels(20, 20))]
}

#[test]
fn unknown_root_key_in_scene_json_is_reported() {
    let game = game_json(2, 2, TILESET);
    let scene = r#"{"objects":[],"bogus":[]}"#;
    let result = load(&game, scene, &tiles_ok());
    let errors = located_errors(&result);
    assert!(
        errors
            .iter()
            .any(|(f, _, m)| f == "scene.json" && m.contains("bogus")),
        "{errors:?}"
    );
}

#[test]
fn ground_not_a_list_is_reported() {
    let game = game_json(2, 2, TILESET);
    let scene = r#"{"objects":[],"ground":{}}"#;
    let result = load(&game, scene, &tiles_ok());
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("массив")), "{errors:?}");
}

#[test]
fn ground_layer_missing_image_or_cells_is_reported() {
    let game = game_json(2, 2, TILESET);
    for layer in ["{\"cells\":[[0,0],[0,0]]}", "{\"image\":\"terrain\"}"] {
        let scene = format!(r#"{{"objects":[],"ground":[{layer}]}}"#);
        let result = load(&game, &scene, &tiles_ok());
        let errors = located_errors(&result);
        assert!(
            errors
                .iter()
                .any(|(f, p, _)| f == "scene.json" && p == "ground[0]"),
            "{layer}: {errors:?}"
        );
    }
}

#[test]
fn ground_layer_undeclared_image_is_reported() {
    let game = game_json(2, 2, TILESET);
    let scene = r#"{"objects":[],"ground":[{"image":"ghost","cells":[[0,0],[0,0]]}]}"#;
    let result = load(&game, scene, &tiles_ok());
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("ghost")), "{errors:?}");
}

#[test]
fn ground_layer_image_with_frame_time_or_size_is_reported() {
    let game = game_json(
        2,
        2,
        r#","images":{
            "animated":{"path":"a.png","frames":2,"frame_time":0.1},
            "sized":{"path":"s.png","size":[2,2]}
        }"#,
    );
    let images = vec![
        ("animated", ok_pixels(20, 10)),
        ("sized", ok_pixels(10, 10)),
    ];
    for name in ["animated", "sized"] {
        let scene =
            format!(r#"{{"objects":[],"ground":[{{"image":"{name}","cells":[[0,0],[0,0]]}}]}}"#);
        let result = load(&game, &scene, &images);
        let errors = err_messages(&result);
        assert!(
            errors.iter().any(|m| m.contains(name)),
            "{name}: {errors:?}"
        );
    }
}

#[test]
fn ground_cells_row_and_column_count_must_match_the_scene() {
    let game = game_json(2, 2, TILESET);
    // Одна строка вместо двух.
    let too_few_rows = r#"{"objects":[],"ground":[{"image":"terrain","cells":[[0,0]]}]}"#;
    let errors = err_messages(&load(&game, too_few_rows, &tiles_ok()));
    assert!(errors.iter().any(|m| m.contains("строк")), "{errors:?}");
    // Одно число в строке вместо двух.
    let too_few_cols = r#"{"objects":[],"ground":[{"image":"terrain","cells":[[0],[0,0]]}]}"#;
    let errors = err_messages(&load(&game, too_few_cols, &tiles_ok()));
    assert!(errors.iter().any(|m| m.contains("чисел")), "{errors:?}");
}

/// «Проверка перед запуском», требование 24: сообщение называет файл и точное место.
#[test]
fn a_tile_number_out_of_range_is_located_precisely() {
    let game = game_json(2, 2, TILESET);
    let scene = r#"{"objects":[],"ground":[{"image":"terrain","cells":[[0,0],[0,9]]}]}"#;
    let result = load(&game, scene, &tiles_ok());
    let errors = located_errors(&result);
    assert!(
        errors
            .iter()
            .any(|(f, p, _)| f == "scene.json" && p == "ground[0] → cells[1][1]"),
        "{errors:?}"
    );
}

#[test]
fn a_tile_number_below_minus_one_or_not_an_integer_is_reported() {
    for cell in ["-2", "1.5"] {
        let game = game_json(2, 2, TILESET);
        let scene = format!(
            r#"{{"objects":[],"ground":[{{"image":"terrain","cells":[[0,{cell}],[0,0]]}}]}}"#
        );
        let errors = err_messages(&load(&game, &scene, &tiles_ok()));
        assert!(!errors.is_empty(), "cell={cell}: {errors:?}");
    }
}

// -------------------------------------------------------------------------------------------
// Загружается без ошибок: без ground всё как сейчас, а с ним земля разбирается, попадает в Game и
// не считается неиспользуемой картинкой
// -------------------------------------------------------------------------------------------

#[test]
fn a_scene_without_ground_loads_exactly_as_before() {
    let game = game_json(2, 2, "");
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ff0000"}]}"##;
    let (loaded, _s, warnings, _images) = load(&game, scene, &[]).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert!(loaded.ground.is_empty());
}

/// «Картинки» → «Загрузка и проверка»: картинка, названная только в `ground`, не получает
/// предупреждения «объявлена и нигде не названа», а никем не названная картинка всё равно его
/// получает.
#[test]
fn an_image_named_only_by_ground_is_not_reported_as_unused() {
    let game = game_json(
        2,
        2,
        r#","images":{
            "terrain":{"path":"terrain.png","frames":4,"columns":2},
            "orphan":{"path":"orphan.png"}
        }"#,
    );
    let scene = r#"{"objects":[],"ground":[{"image":"terrain","cells":[[0,0],[0,0]]}]}"#;
    let (_g, _s, warnings, _images) = load(
        &game,
        scene,
        &[("terrain", ok_pixels(20, 20)), ("orphan", ok_pixels(1, 1))],
    )
    .expect("должно загрузиться");
    assert!(
        warnings
            .iter()
            .all(|w| !w.message.contains("terrain") || !w.message.contains("не называет")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("orphan") && w.message.contains("не называет")),
        "{warnings:?}"
    );
}

/// Требование 19: земля не объект — не входит в мир и не меняет число живых объектов.
#[test]
fn ground_adds_no_object_to_the_world() {
    let game = game_json(2, 2, TILESET);
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ff0000"}],"ground":[{"image":"terrain","cells":[[0,0],[0,0]]}]}"##;
    let (loaded, _s, warnings, _images) =
        load(&game, scene, &tiles_ok()).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert_eq!(loaded.world.alive_count(), 1, "только объявленный объект");
    assert_eq!(loaded.ground.len(), 1);
}

// -------------------------------------------------------------------------------------------
// Сквозная сборка: JSON → Game.ground → атлас → плитки на своих клетках
// -------------------------------------------------------------------------------------------

#[test]
fn ground_end_to_end_through_a_real_atlas_lands_tiles_on_their_own_cells() {
    let game = game_json(2, 2, TILESET);
    let scene = r#"{"objects":[],"ground":[{"image":"terrain","cells":[[0,1],[2,3]]}]}"#;
    let (loaded, _s, warnings, images) =
        load(&game, scene, &tiles_ok()).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    let atlas_images = vec![AtlasImage {
        width: 20,
        height: 20,
        pixels: vec![0u8; 20 * 20 * 4],
    }];
    let atlas = atlas::pack(&atlas_images).expect("одна картинка всегда умещается");
    let whole = atlas.rects[0];

    let visible = CellRange {
        x0: 0,
        y0: 0,
        x1: 2,
        y1: 2,
    };
    let paints = atlas::compose_ground_paints(&loaded.ground, visible, &images, &atlas.rects);
    assert_eq!(paints.len(), 4);
    let at = |x: f32, y: f32| {
        paints
            .iter()
            .find(|p| p.position == [x, y])
            .unwrap_or_else(|| panic!("нет плитки в ({x}, {y}): {paints:?}"))
    };
    // Сетка 2×2 (frames:4, columns:2): кадр n — строка n/2, столбец n%2.
    let frame_w = whole.w / 2;
    let frame_h = whole.h / 2;
    let rect_of = |n: u32| AtlasRect {
        x: whole.x + (n % 2) * frame_w,
        y: whole.y + (n / 2) * frame_h,
        w: frame_w,
        h: frame_h,
    };
    assert_eq!(at(0.0, 0.0).atlas_rect, rect_of(0));
    assert_eq!(at(1.0, 0.0).atlas_rect, rect_of(1));
    assert_eq!(at(0.0, 1.0).atlas_rect, rect_of(2));
    assert_eq!(at(1.0, 1.0).atlas_rect, rect_of(3));
    for p in &paints {
        assert_eq!(p.size, [1.0, 1.0]);
    }
}

// -------------------------------------------------------------------------------------------
// Картинка своего размера не влияет на столкновение, object_at, on_click, y_sort — только на
// рисование, требование 10
// -------------------------------------------------------------------------------------------

#[test]
fn own_size_image_does_not_change_object_at_on_click_or_layer_order() {
    let game = game_json(
        10,
        10,
        r#","images":{"a":{"path":"a.png","size":[5,5],"anchor":"center"},"b":{"path":"b.png"}}"#,
    );
    // Same rectangle, same layer — object 1 (the later one) must still win object_at, by number,
    // regardless of which one has the oversized picture.
    let scene = r##"{"objects":[
        {"position":[2,2],"size":[1,1],"image":"a","on_click":[["layer",1]]},
        {"position":[2,2],"size":[1,1],"image":"b","on_click":[["layer",2]]}
    ]}"##;
    let (loaded, _s, warnings, _images) = load(
        &game,
        scene,
        &[("a", ok_pixels(10, 10)), ("b", ok_pixels(10, 10))],
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let picked =
        engine::core::scene::object_at(&loaded.world, &loaded.scene, [25.0, 25.0], [100.0, 100.0]);
    assert_eq!(
        picked,
        Some(1),
        "прямоугольник, а не картинка, решает щелчок"
    );
    let clicked = engine::core::scene::on_click_target(&loaded.world, &loaded.scene, [2.5, 2.5]);
    assert_eq!(clicked, Some(1));
}

// -------------------------------------------------------------------------------------------
// Нефункциональное требование: расчёт земли для трёх слоёв 64×40 при виде всей сцены — не дольше
// 1 мс в --release (`cargo test --release -- --nocapture`, как у `replay_seek_performance.rs`).
// -------------------------------------------------------------------------------------------

#[test]
fn ground_computation_for_three_layers_of_a_64_by_40_scene_is_fast() {
    let width = 64u32;
    let height = 40u32;
    let mut cells = Vec::with_capacity(height as usize);
    for y in 0..height {
        cells.push((0..width).map(|x| ((x + y) % 5) as i32 - 1).collect());
    }
    let ground: Vec<GroundLayer> = (0..3)
        .map(|_| GroundLayer {
            image: 0,
            cells: cells.clone(),
        })
        .collect();
    let images = vec![{
        let (config, _w) = read_entry(&game_json(
            width,
            height,
            r#","images":{"t":{"path":"t.png","frames":4,"columns":2}}"#,
        ))
        .unwrap();
        config.files.images[0].clone()
    }];
    let atlas_images = vec![AtlasImage {
        width: 20,
        height: 20,
        pixels: vec![0u8; 20 * 20 * 4],
    }];
    let atlas = atlas::pack(&atlas_images).unwrap();
    let visible = CellRange {
        x0: 0,
        y0: 0,
        x1: width,
        y1: height,
    };

    let started = Instant::now();
    let paints = atlas::compose_ground_paints(&ground, visible, &images, &atlas.rects);
    let elapsed = started.elapsed();
    println!("compose_ground_paints(3×{width}×{height}) занял {elapsed:?}");
    assert!(!paints.is_empty());

    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_secs_f64() < 0.001,
        "заняло {elapsed:?} — дольше 1 мс"
    );
}

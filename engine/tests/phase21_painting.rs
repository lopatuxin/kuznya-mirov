//! Фаза 21 — покраска: вызов движка `set_covers` (слои и маски покрытий, проверки, мир не собирается
//! заново, карта цвета остаётся), сборка слоя масок и таблицы слоёв для видеокарты `CoverUpdate` и
//! `terrain_readings` — рельеф игры без видеокарты. Игры собираются в коде теста, как в
//! `phase19_light_materials.rs`. Настоящая деревня — в `rpg_game.rs`.

use engine::core::game::Game;
use engine::core::property;
use engine::core::terrain::Cover;
use engine::data::edit::{self, TerrainHeights};
use engine::data::error::GameError;
use engine::data::load::{
    CoverMask, ImageVerdict, MaterialDecl, load_rest_with_stamps, read_entry,
};
use engine::render::materials::{CoverUpdate, MapView};
use serde_json::{Value as Json, json};

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

const PROPS: &str = r#"{"properties":{}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const OBJECTS: &str = r#"{"objects":[{"position":[0.5,0.5]}]}"#;
const EARTH_MASK: &str = "terrain/earth.png";
const ROCK_MASK: &str = "terrain/rock.png";
const TINT: &str = "terrain/tint.png";
const WEDGE: &str = r#"{"heights":[[0,0.5],[0.5,1]]}"#;
const WEDGE_MOUNTAIN: &str = r#"{"stamp":"wedge","position":[0,0],"size":[2,2],"height":3}"#;

fn material_json(name: &str) -> String {
    format!(
        r#""{name}":{{"size":2,"color":"{name}/c.jpg","normal":"{name}/n.jpg","roughness":"{name}/r.jpg","height":"{name}/h.jpg"}}"#
    )
}

fn ok_image(side: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width: side,
        height: side,
        pixels: vec![0; (side * side * 4) as usize],
    }
}

fn material_maps() -> Vec<(String, ImageVerdict)> {
    ["grass", "earth", "rock"]
        .iter()
        .flat_map(|name| {
            ["c.jpg", "n.jpg", "r.jpg", "h.jpg"]
                .iter()
                .map(move |file| (format!("{name}/{file}"), ok_image(512)))
        })
        .collect()
}

/// Игра 2 × 2 клетки из трёх материалов, `grass`, `earth` и `rock`, и штампа `wedge`.
struct Setup {
    camera: bool,
    terrain_file: bool,
    covers: String,
    tint: bool,
    stamps: bool,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup {
            camera: true,
            terrain_file: true,
            covers: format!(
                r#"[{{"material":"grass"}},{{"material":"earth","mask":"{EARTH_MASK}"}}]"#
            ),
            tint: false,
            stamps: false,
        }
    }
}

impl Setup {
    fn game_json(&self) -> String {
        let camera = if self.camera {
            r#","view_height":12,"camera":{"pitch":55}"#
        } else {
            ""
        };
        let terrain = if self.terrain_file {
            r#","terrain":"terrain.json""#
        } else {
            ""
        };
        format!(
            r##"{{"name":"T","scene":{{"width":2,"height":2,"background":"#4f8a3c"{camera}}},
"random_seed":1,"start_screen":"main","max_objects":10,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{terrain},
         "materials":{{{},{},{}}},"stamps":{{"wedge":"stamps/wedge.json"}}}}}}"##,
            material_json("grass"),
            material_json("earth"),
            material_json("rock"),
        )
    }

    fn terrain(&self) -> String {
        let row = "[0,0,0,0,0]";
        let tint = if self.tint {
            format!(r#""tint":"{TINT}","#)
        } else {
            String::new()
        };
        let stamps = if self.stamps {
            format!(r#""stamps":[{WEDGE_MOUNTAIN}],"#)
        } else {
            String::new()
        };
        format!(
            r#"{{"covers":{},{tint}{stamps}"heights":[{row},{row},{row},{row},{row}]}}"#,
            self.covers
        )
    }

    fn game(&self) -> Game {
        let game_json = self.game_json();
        let (config, _) = read_entry(&game_json).expect("game.json");
        let masks = [EARTH_MASK, ROCK_MASK, TINT].map(|path| (path.to_string(), ok_image(8)));
        let texts = [("wedge".to_string(), Some(WEDGE.to_string()))];
        let terrain = self.terrain_file.then(|| self.terrain());
        let (game, _screens, _warnings, _images) = load_rest_with_stamps(
            &game_json,
            config,
            Some(PROPS),
            Some(OBJECTS),
            Some(NO_RULES),
            Some(SCREENS),
            &[],
            &[],
            &[],
            &[],
            None,
            false,
            &[],
            terrain.as_deref(),
            &material_maps(),
            &masks,
            &texts,
        )
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors));
        game
    }

    fn materials(&self) -> Vec<MaterialDecl> {
        read_entry(&self.game_json())
            .expect("game.json")
            .0
            .files
            .materials
    }
}

fn mask(width: u32, height: u32, value: u8) -> CoverMask {
    CoverMask {
        width,
        height,
        pixels: vec![value; (width * height) as usize],
    }
}

fn layers(masks: &[&str]) -> Json {
    let mut layers = vec![json!({"material": "grass"})];
    layers.extend(
        masks
            .iter()
            .map(|path| json!({"material": "earth", "mask": path})),
    );
    Json::Array(layers)
}

fn cover(material: usize, mask: Option<usize>, slope: Option<f64>) -> Cover {
    Cover {
        material,
        mask,
        slope,
    }
}

// -------------------------------------------------------------------------------------------
// set_covers
// -------------------------------------------------------------------------------------------

#[test]
fn set_covers_puts_the_layers_in_the_loaded_game() {
    let setup = Setup::default();
    let mut game = setup.game();
    let covers = json!([
        {"material": "grass"},
        {"material": "earth", "mask": EARTH_MASK},
        {"material": "rock", "slope": 44},
        {"material": "rock", "slope": 36, "mask": ROCK_MASK},
    ]);
    edit::set_covers(
        &mut game,
        &setup.materials(),
        &covers,
        &[mask(8, 8, 0), mask(8, 8, 255)],
    )
    .expect("покрытия поставлены");
    assert_eq!(
        game.world.terrain().covers(),
        [
            cover(0, None, None),
            cover(1, Some(0), None),
            cover(2, None, Some(44.0)),
            cover(2, Some(1), Some(36.0)),
        ]
    );
}

#[test]
fn set_covers_keeps_the_heights_the_mountains_the_water_and_the_color_map() {
    let setup = Setup {
        tint: true,
        stamps: true,
        ..Setup::default()
    };
    let mut game = setup.game();
    let before = game.world.terrain().clone();
    edit::set_covers(
        &mut game,
        &setup.materials(),
        &json!([{"material": "rock"}]),
        &[],
    )
    .expect("покрытия поставлены");
    let after = game.world.terrain();
    assert!(after.has_tint(), "карта цвета осталась названной");
    assert_eq!(after.covers(), [cover(2, None, None)]);
    assert_eq!(after.heights(), before.heights());
    assert_eq!(after.base_heights(), before.base_heights());
    assert_eq!(after.mountains().len(), 1);
    assert_eq!(after.water(), before.water());
}

#[test]
fn set_covers_does_not_rebuild_the_world() {
    let setup = Setup::default();
    let mut game = setup.game();
    game.show_scene();
    game.move_object(0, [1.5, 1.5], None);
    edit::set_covers(
        &mut game,
        &setup.materials(),
        &layers(&[EARTH_MASK]),
        &[mask(8, 8, 9)],
    )
    .expect("покрытия поставлены");
    assert_eq!(game.world.vec2(0, property::POSITION), Some([1.5, 1.5]));
}

#[test]
fn set_terrain_after_set_covers_keeps_the_covers_that_were_set() {
    let setup = Setup::default();
    let mut game = setup.game();
    edit::set_covers(
        &mut game,
        &setup.materials(),
        &json!([{"material": "grass"}, {"material": "rock", "slope": 40}]),
        &[],
    )
    .expect("покрытия поставлены");
    edit::set_terrain(&mut game, &[0.5; 25], None, &[]).expect("рельеф поставлен");
    assert_eq!(
        game.world.terrain().covers(),
        [cover(0, None, None), cover(2, None, Some(40.0))]
    );
    assert_eq!(game.world.terrain().heights()[12], 0.5);
}

/// Что `set_covers` ответил на `covers` и `masks`; игра при этом не должна измениться.
fn refusal(covers: &Json, masks: &[CoverMask]) -> String {
    let setup = Setup::default();
    let mut game = setup.game();
    let before = game.world.terrain().clone();
    let text = edit::set_covers(&mut game, &setup.materials(), covers, masks)
        .expect_err("покрытия должны не встать");
    assert_eq!(
        game.world.terrain(),
        &before,
        "{text}: рельеф не должен меняться"
    );
    text
}

fn assert_refused(covers: &Json, masks: &[CoverMask], fragments: &[&str]) {
    let text = refusal(covers, masks);
    assert!(
        fragments.iter().all(|fragment| text.contains(fragment)),
        "нет слов {fragments:?}: {text}"
    );
}

#[test]
fn a_layer_that_load_would_refuse_is_refused_with_its_place() {
    assert_refused(
        &json!({"material": "grass"}),
        &[],
        &["covers", "ожидался массив"],
    );
    assert_refused(&json!([]), &[], &["covers пуст"]);
    assert_refused(
        &json!([{"material": "grass", "opacity": 1}]),
        &[],
        &["covers[0] → opacity", "неизвестное поле"],
    );
    assert_refused(
        &json!([{"material": "snow"}]),
        &[],
        &["covers[0] → material", "snow", "не объявлен"],
    );
    assert_refused(
        &json!([{"material": "grass", "mask": EARTH_MASK}]),
        &[mask(8, 8, 0)],
        &["covers[0] → mask", "у нижнего слоя маски нет"],
    );
    assert_refused(
        &json!([{"material": "grass", "slope": 30}]),
        &[],
        &["covers[0] → slope", "у нижнего слоя"],
    );
    assert_refused(
        &json!([{"material": "grass"}, {"material": "earth"}]),
        &[],
        &["covers[1]", "mask", "slope"],
    );
    assert_refused(
        &json!([{"material": "grass"}, {"material": "earth", "slope": 91}]),
        &[],
        &["covers[1] → slope", "от 0 до 90"],
    );
    assert_refused(
        &layers(&["terrain/earth.jpg"]),
        &[mask(8, 8, 0)],
        &["covers[1] → mask", "только из PNG"],
    );
}

#[test]
fn eight_layers_are_fine_and_nine_are_an_error() {
    let setup = Setup::default();
    let mut game = setup.game();
    let eight = layers(&[EARTH_MASK; 7]);
    edit::set_covers(
        &mut game,
        &setup.materials(),
        &eight,
        &vec![mask(8, 8, 1); 7],
    )
    .expect("восемь слоёв");
    assert_eq!(game.world.terrain().covers().len(), 8);
    assert_refused(
        &layers(&[EARTH_MASK; 8]),
        &vec![mask(8, 8, 1); 8],
        &["covers", "больше 8"],
    );
}

#[test]
fn masks_that_are_not_one_per_masked_layer_are_an_error() {
    assert_refused(
        &layers(&[EARTH_MASK, ROCK_MASK]),
        &[mask(8, 8, 0)],
        &["masks", "1 масок", "слоёв с маской 2"],
    );
    assert_refused(
        &layers(&[EARTH_MASK]),
        &[mask(8, 8, 0), mask(8, 8, 0)],
        &["masks", "2 масок", "слоёв с маской 1"],
    );
    assert_refused(&layers(&[EARTH_MASK]), &[], &["masks", "0 масок"]);
}

#[test]
fn a_mask_of_zero_size_or_with_the_wrong_number_of_bytes_is_an_error() {
    let covers = layers(&[EARTH_MASK]);
    assert_refused(
        &covers,
        &[mask(0, 8, 0)],
        &["masks[0]", "больше нуля", "0×8"],
    );
    assert_refused(
        &covers,
        &[mask(8, 0, 0)],
        &["masks[0]", "больше нуля", "8×0"],
    );
    let short = CoverMask {
        width: 8,
        height: 8,
        pixels: vec![0; 63],
    };
    assert_refused(&covers, &[short], &["masks[0]", "63 байт", "8×8", "64"]);
    let long = CoverMask {
        width: 4,
        height: 2,
        pixels: vec![0; 64],
    };
    assert_refused(&covers, &[long], &["masks[0]", "64 байт", "4×2", "8"]);
}

#[test]
fn a_mask_of_any_size_is_accepted() {
    let setup = Setup::default();
    let mut game = setup.game();
    for (width, height) in [(1, 1), (7, 3), (512, 384)] {
        edit::set_covers(
            &mut game,
            &setup.materials(),
            &layers(&[EARTH_MASK]),
            &[mask(width, height, 5)],
        )
        .unwrap_or_else(|text| panic!("{width}×{height}: {text}"));
    }
}

#[test]
fn set_covers_in_a_flat_scene_in_a_session_or_without_a_terrain_file_is_an_error() {
    let mut game = flat_game();
    let text = edit::set_covers(&mut game, &Setup::default().materials(), &layers(&[]), &[])
        .expect_err("сцена плоская");
    assert!(text.contains("трёхмерной"), "{text}");

    let setup = Setup::default();
    let mut game = setup.game();
    game.begin_session();
    let before = game.world.terrain().clone();
    let text = edit::set_covers(&mut game, &setup.materials(), &layers(&[]), &[])
        .expect_err("идёт партия");
    assert!(text.contains("партия"), "{text}");
    assert_eq!(game.world.terrain(), &before);

    let bare = Setup {
        terrain_file: false,
        ..Setup::default()
    };
    let mut game = bare.game();
    let text =
        edit::set_covers(&mut game, &bare.materials(), &layers(&[]), &[]).expect_err("рельефа нет");
    assert!(text.contains("файла рельефа"), "{text}");
}

/// Игра плоской сцены без рельефа и без материалов.
fn flat_game() -> Game {
    let plain = r##"{"name":"T","scene":{"width":2,"height":2,"background":"#4f8a3c"},
"random_seed":1,"start_screen":"main","max_objects":10,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{}}}"##;
    let (config, _) = read_entry(plain).expect("game.json");
    load_rest_with_stamps(
        plain,
        config,
        Some(PROPS),
        Some(OBJECTS),
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
    )
    .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
    .0
}

// -------------------------------------------------------------------------------------------
// CoverUpdate
// -------------------------------------------------------------------------------------------

fn masked(count: usize) -> Vec<Cover> {
    std::iter::once(cover(0, None, None))
        .chain((0..count).map(|index| cover(1, Some(index), None)))
        .collect()
}

#[test]
fn an_update_writes_each_mask_into_its_own_channel_of_the_layer_by_four() {
    let materials = Setup::default().materials();
    let masks: Vec<CoverMask> = (1..=5).map(|value| mask(2, 2, value * 10)).collect();
    let update = CoverUpdate::new(&masked(5), &materials, [2, 2], &masks, None);
    assert_eq!((update.masks.width, update.masks.height), (2, 2));
    assert_eq!(update.masks.layers.len(), 2);
    for point in 0..4 {
        for index in 0..4 {
            assert_eq!(
                update.masks.layers[0][point * 4 + index],
                (index as u8 + 1) * 10
            );
        }
        assert_eq!(update.masks.layers[1][point * 4], 50);
        assert_eq!(update.masks.layers[1][point * 4 + 1], 0);
    }
}

#[test]
fn an_update_keeps_the_points_of_a_mask_where_they_are() {
    let materials = Setup::default().materials();
    let masks = [CoverMask {
        width: 3,
        height: 2,
        pixels: vec![1, 2, 3, 4, 5, 6],
    }];
    let update = CoverUpdate::new(&masked(1), &materials, [2, 2], &masks, None);
    let red: Vec<u8> = update.masks.layers[0]
        .chunks(4)
        .map(|point| point[0])
        .collect();
    assert_eq!(red, [1, 2, 3, 4, 5, 6]);
}

#[test]
fn an_update_brings_masks_of_different_sizes_to_the_largest() {
    let materials = Setup::default().materials();
    let masks = [mask(2, 2, 200), mask(4, 4, 100)];
    let update = CoverUpdate::new(&masked(2), &materials, [2, 2], &masks, None);
    assert_eq!((update.masks.width, update.masks.height), (4, 4));
    let layer = &update.masks.layers[0];
    assert!(
        layer
            .chunks(4)
            .all(|point| point[0] == 200 && point[1] == 100)
    );
}

#[test]
fn the_table_of_an_update_numbers_layers_materials_masks_and_slopes() {
    let materials = Setup::default().materials();
    let covers = [
        cover(0, None, None),
        cover(1, Some(0), None),
        cover(2, None, Some(44.0)),
        cover(2, Some(1), Some(36.0)),
    ];
    let update = CoverUpdate::new(
        &covers,
        &materials,
        [2, 3],
        &[mask(2, 2, 0), mask(2, 2, 0)],
        None,
    );
    assert_eq!(update.table[0], [4.0, 2.0, 3.0, 0.0]);
    assert_eq!(update.table[1], [0.0, 0.5, -1.0, -1.0]);
    assert_eq!(update.table[2], [1.0, 0.5, 0.0, -1.0]);
    assert_eq!(update.table[3], [2.0, 0.5, -1.0, 44.0]);
    assert_eq!(update.table[4], [2.0, 0.5, 1.0, 36.0]);
    assert_eq!(update.table[5], [0.0; 4]);
}

#[test]
fn the_color_map_stays_after_the_masks_and_moves_up_when_a_layer_of_masks_is_added() {
    let materials = Setup::default().materials();
    let pixels: Vec<u8> = (0..16).collect();
    let tint = MapView {
        width: 2,
        height: 2,
        pixels: &pixels,
    };
    let few = CoverUpdate::new(
        &masked(2),
        &materials,
        [2, 2],
        &[mask(2, 2, 7), mask(2, 2, 8)],
        Some(&tint),
    );
    assert_eq!(few.table[0][3], 2.0, "карта цвета во втором слое массива");
    assert_eq!(few.masks.layers.len(), 2);
    assert_eq!(few.masks.layers[1], pixels);

    let many_masks: Vec<CoverMask> = (0..5).map(|_| mask(2, 2, 7)).collect();
    let many = CoverUpdate::new(&masked(5), &materials, [2, 2], &many_masks, Some(&tint));
    assert_eq!(
        many.table[0][3], 3.0,
        "пять масок заняли два слоя, карта цвета — третий"
    );
    assert_eq!(many.masks.layers.len(), 3);
    assert_eq!(many.masks.layers[2], pixels);

    let none = CoverUpdate::new(&masked(0), &materials, [2, 2], &[], Some(&tint));
    assert_eq!(none.table[0][3], 2.0);
    assert_eq!(none.masks.layers[1], pixels);
}

#[test]
fn without_a_color_map_the_table_names_none_and_there_is_always_a_layer_of_masks() {
    let materials = Setup::default().materials();
    let update = CoverUpdate::new(&masked(0), &materials, [2, 2], &[], None);
    assert_eq!(update.table[0][3], 0.0);
    assert_eq!(update.masks.layers.len(), 1);
}

// -------------------------------------------------------------------------------------------
// terrain_readings
// -------------------------------------------------------------------------------------------

fn texts() -> Vec<(String, Option<String>)> {
    vec![("wedge".to_string(), Some(WEDGE.to_string()))]
}

fn readings(setup: &Setup) -> Result<TerrainHeights, Vec<GameError>> {
    let terrain = setup.terrain_file.then(|| setup.terrain());
    edit::terrain_readings(&setup.game_json(), terrain.as_deref(), &texts())
}

fn error_text(errors: &[GameError]) -> String {
    errors
        .iter()
        .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn terrain_readings_give_what_terrain_heights_give_after_a_load() {
    for setup in [
        Setup {
            stamps: true,
            ..Setup::default()
        },
        Setup::default(),
    ] {
        let loaded = edit::terrain_heights(&setup.game()).expect("сцена трёхмерная");
        let read = readings(&setup).expect("рельеф читается");
        assert_eq!(
            (read.density, read.columns, read.rows),
            (loaded.density, loaded.columns, loaded.rows)
        );
        assert_eq!(read.heights, loaded.heights);
        assert_eq!(read.effective, loaded.effective);
        assert_eq!(read.water, loaded.water);
    }
}

#[test]
fn terrain_readings_lift_the_effective_heights_by_the_mountains_and_not_the_heights() {
    let setup = Setup {
        stamps: true,
        ..Setup::default()
    };
    let read = readings(&setup).expect("рельеф читается");
    assert_eq!((read.columns, read.rows), (5, 5));
    assert!(read.heights.iter().all(|&h| h == 0.0));
    assert!(
        read.effective.iter().any(|&h| h > 0.0),
        "гора поднимает землю"
    );
}

#[test]
fn terrain_readings_carry_the_water() {
    let setup = Setup::default();
    let terrain = setup.terrain().replace(
        r#"{"covers""#,
        r##"{"water":{"level":-1.5,"color":"#3f7fd0"},"covers""##,
    );
    let read = edit::terrain_readings(&setup.game_json(), Some(&terrain), &texts())
        .expect("рельеф читается");
    assert_eq!(read.water, Some((-1.5, "#3f7fd0".to_string())));
}

#[test]
fn terrain_readings_of_a_game_without_a_terrain_file_are_zeros_of_the_scene_grid() {
    let bare = Setup {
        terrain_file: false,
        ..Setup::default()
    };
    let read = readings(&bare).expect("рельефа нет — не ошибка");
    assert_eq!((read.density, read.columns, read.rows), (2, 5, 5));
    assert_eq!(read.heights, vec![0.0; 25]);
    assert_eq!(read.effective, vec![0.0; 25]);
    assert_eq!(read.water, None);
    let loaded = edit::terrain_heights(&bare.game()).expect("сцена трёхмерная");
    assert_eq!(read.heights, loaded.heights);
}

#[test]
fn terrain_readings_do_not_need_the_files_of_the_covers() {
    let setup = Setup {
        tint: true,
        covers: layers(&[EARTH_MASK, ROCK_MASK]).to_string(),
        ..Setup::default()
    };
    readings(&setup).expect("маски и карту цвета читает страница, а не рельеф");
}

#[test]
fn terrain_readings_of_a_broken_terrain_file_is_an_error_naming_it() {
    let setup = Setup::default();
    let game_json = setup.game_json();
    for (text, fragments) in [
        ("{", vec!["terrain.json"]),
        (r#"{"heights":[[0]]}"#, vec!["terrain.json", "heights"]),
        (r#"{"heights":[]}"#, vec!["terrain.json", "heights"]),
        (
            r#"{"covers":[{"material":"snow"}],"heights":[[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0]]}"#,
            vec!["terrain.json", "covers[0] → material", "snow"],
        ),
    ] {
        let errors = edit::terrain_readings(&game_json, Some(text), &texts())
            .expect_err("рельеф не должен читаться");
        let message = error_text(&errors);
        assert!(
            fragments.iter().all(|fragment| message.contains(fragment)),
            "{text}: нет слов {fragments:?}: {message}"
        );
    }
}

#[test]
fn terrain_readings_with_an_unknown_stamp_or_a_missing_stamp_file_is_an_error() {
    let setup = Setup {
        stamps: true,
        ..Setup::default()
    };
    let game_json = setup.game_json();
    let terrain = setup
        .terrain()
        .replace(r#""stamp":"wedge""#, r#""stamp":"cone""#);
    let errors = edit::terrain_readings(&game_json, Some(&terrain), &texts())
        .expect_err("штамп не объявлен");
    let message = error_text(&errors);
    assert!(
        message.contains("cone") && message.contains("не объявлен"),
        "{message}"
    );

    let errors = edit::terrain_readings(&game_json, Some(&setup.terrain()), &[])
        .expect_err("файла штампа нет");
    assert!(error_text(&errors).contains("stamps/wedge.json"));

    let errors = edit::terrain_readings(
        &game_json,
        Some(&setup.terrain()),
        &[("wedge".to_string(), Some("{".to_string()))],
    )
    .expect_err("штамп не разбирается");
    assert!(error_text(&errors).contains("stamps/wedge.json"));
}

#[test]
fn terrain_readings_name_a_declared_terrain_file_that_was_not_given() {
    let setup = Setup::default();
    let errors = edit::terrain_readings(&setup.game_json(), None, &texts())
        .expect_err("файл назван, а текста нет");
    assert!(error_text(&errors).contains("terrain.json"));
}

#[test]
fn terrain_readings_of_a_flat_scene_or_a_broken_game_json_is_an_error() {
    let flat = Setup {
        camera: false,
        ..Setup::default()
    };
    let errors = edit::terrain_readings(&flat.game_json(), None, &[]).expect_err("сцена плоская");
    assert!(error_text(&errors).contains("трёхмерной"));

    let errors = edit::terrain_readings("{", None, &[]).expect_err("game.json не разбирается");
    assert!(error_text(&errors).contains("game.json"));
}

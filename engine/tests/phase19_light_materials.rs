//! Фаза 19 — свет и материалы рельефа. Игры собираются в коде теста, как в `phase17_terrain.rs`:
//! поля света `sun_color` и `sky_color`, данные кадра со светом, `files.materials`, `covers` файла
//! рельефа и все проверки перед запуском, заход `read_texts`, упаковка карт и масок для видеокарты.
//! Настоящая деревня с настоящими картами и масками — в `rpg_game.rs`, шейдер — в
//! `phase15_3d_scene_shapes.rs`.

use engine::core::game::Game;
use engine::core::scene::LightConfig;
use engine::core::terrain::Cover;
use engine::core::value::parse_color;
use engine::data::edit;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageVerdict, MaterialDecl, load_rest_with_materials, read_entry, read_texts,
};
use engine::render::atlas::webgl2_safe_layer_count;
use engine::render::materials::{
    MapView, MaterialMaps, PackedMaterial, Relief, level_count, level_side, pack_masks,
    pack_material,
};
use engine::render::scene3d::{compose_frame3d, light_colors};

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

const PROPS: &str = r#"{"properties":{}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const NO_OBJECTS: &str = r#"{"objects":[]}"#;
const MASK: &str = "terrain/earth.png";

fn material_json(name: &str) -> String {
    format!(
        r#""{name}":{{"size":2,"color":"{name}/color.jpg","normal":"{name}/normal.jpg","roughness":"{name}/roughness.png","height":"{name}/height.jpg","ao":"{name}/ao.jpg"}}"#
    )
}

fn ok_map(side: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width: side,
        height: side,
        pixels: vec![0; (side * side * 4) as usize],
    }
}

fn maps_of(name: &str, side: u32) -> Vec<(String, ImageVerdict)> {
    [
        "color.jpg",
        "normal.jpg",
        "roughness.png",
        "height.jpg",
        "ao.jpg",
    ]
    .iter()
    .map(|file| (format!("{name}/{file}"), ok_map(side)))
    .collect()
}

/// Файл рельефа сцены 2 × 2 клетки, ровный, с `covers`, если они даны.
fn terrain_text(covers: Option<&str>) -> String {
    let row = "[0,0,0,0,0]";
    let covers = covers.map_or_else(String::new, |covers| format!(r#""covers":{covers},"#));
    format!(r#"{{{covers}"heights":[{row},{row},{row},{row},{row}]}}"#)
}

/// Игра из двух материалов, `grass` и `earth`, и рельефа с двумя слоями — по умолчанию исправная;
/// каждый тест портит своё.
struct Setup {
    camera: bool,
    light: String,
    materials: Option<String>,
    terrain_file: bool,
    covers: Option<String>,
    maps: Vec<(String, ImageVerdict)>,
    masks: Vec<(String, ImageVerdict)>,
}

impl Default for Setup {
    fn default() -> Setup {
        let mut maps = maps_of("grass", 512);
        maps.extend(maps_of("earth", 512));
        Setup {
            camera: true,
            light: String::new(),
            materials: Some(format!(
                "{},{}",
                material_json("grass"),
                material_json("earth")
            )),
            terrain_file: true,
            covers: Some(format!(
                r#"[{{"material":"grass"}},{{"material":"earth","mask":"{MASK}"}}]"#
            )),
            maps,
            masks: vec![(MASK.to_string(), ok_map(4))],
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
        let materials = self
            .materials
            .as_ref()
            .map_or_else(String::new, |table| format!(r#","materials":{{{table}}}"#));
        format!(
            r##"{{"name":"T","scene":{{"width":2,"height":2,"background":"#4f8a3c"{camera}{}}},
"random_seed":1,"start_screen":"main","max_objects":10,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{terrain}{materials}}}}}"##,
            self.light
        )
    }

    fn terrain(&self) -> Option<String> {
        self.terrain_file
            .then(|| terrain_text(self.covers.as_deref()))
    }

    fn load(&self) -> Result<(Game, Vec<GameError>), LoadFailure> {
        let game_json = self.game_json();
        let (config, mut warnings) = read_entry(&game_json)?;
        let terrain = self.terrain();
        match load_rest_with_materials(
            &game_json,
            config,
            Some(PROPS),
            Some(NO_OBJECTS),
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
            &self.maps,
            &self.masks,
        ) {
            Ok((game, _screens, more, _images)) => {
                warnings.extend(more);
                Ok((game, warnings))
            }
            Err(failure) => Err(failure),
        }
    }

    fn game(&self) -> Game {
        self.load()
            .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
            .0
    }
}

fn errors_of(result: Result<(Game, Vec<GameError>), LoadFailure>) -> Vec<String> {
    match result {
        Ok(_) => panic!("игра должна была не загрузиться"),
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
            .collect(),
    }
}

fn expect_error(result: Result<(Game, Vec<GameError>), LoadFailure>, fragments: &[&str]) {
    let errors = errors_of(result);
    assert!(
        errors
            .iter()
            .any(|e| fragments.iter().all(|f| e.contains(f))),
        "нет ошибки со словами {fragments:?}: {errors:#?}"
    );
}

fn material_warnings(warnings: &[GameError]) -> Vec<&GameError> {
    warnings
        .iter()
        .filter(|w| w.message.contains("материал"))
        .collect()
}

#[test]
fn a_whole_setup_loads_without_errors_or_warnings() {
    let (game, warnings) = Setup::default().load().expect("исправная игра");
    assert_eq!(warnings, Vec::new());
    assert_eq!(game.world.terrain().covers().len(), 2);
}

// -------------------------------------------------------------------------------------------
// Свет: цвета
// -------------------------------------------------------------------------------------------

#[test]
fn the_sun_and_the_sky_default_to_a_warm_white_and_a_pale_blue() {
    let light = Setup::default().game().scene.light;
    let rgb = |text: &str| {
        let [r, g, b, _] = parse_color(text).expect("цвет");
        [r, g, b]
    };
    assert_eq!(light.sun_color, rgb("#fff2dc"));
    assert_eq!(light.sky_color, rgb("#a9c8ee"));
    assert_eq!(LightConfig::default().sun_color, rgb("#fff2dc"));
    assert_eq!(LightConfig::default().sky_color, rgb("#a9c8ee"));
}

#[test]
fn sun_color_and_sky_color_are_read_from_the_light() {
    let setup = Setup {
        light: r##","light":{"sun_color":"#ff8000","sky_color":"#0080ff"}"##.to_string(),
        ..Setup::default()
    };
    let light = setup.game().scene.light;
    assert_eq!(light.sun_color, [1.0, 128.0 / 255.0, 0.0]);
    assert_eq!(light.sky_color, [0.0, 128.0 / 255.0, 1.0]);
}

#[test]
fn a_sun_color_or_sky_color_that_is_not_a_color_is_an_error_with_its_own_place() {
    for (key, value) in [
        ("sun_color", r#""yellow""#),
        ("sun_color", r##""#fff""##),
        ("sun_color", "7"),
        ("sky_color", r##""#a9c8ee80""##),
        ("sky_color", "true"),
    ] {
        let setup = Setup {
            light: format!(r#","light":{{"{key}":{value}}}"#),
            ..Setup::default()
        };
        expect_error(setup.load(), &[&format!("scene → light → {key}")]);
    }
}

#[test]
fn an_unknown_light_key_is_an_error_naming_it() {
    let setup = Setup {
        light: r##","light":{"sun_colour":"#ffffff"}"##.to_string(),
        ..Setup::default()
    };
    expect_error(setup.load(), &["sun_colour", "неизвестное поле"]);
}

// -------------------------------------------------------------------------------------------
// Свет: данные кадра
// -------------------------------------------------------------------------------------------

fn luminance(color: [f32; 3]) -> f32 {
    0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 1e-4, "{what}: {a} против {b}");
}

/// Ровная земля на солнце получает свет 1, в тени — `1 − shadow`: солнце светит по косинусу к
/// нормали земли (`sin` высоты солнца), небо — с множителем `(1 + nz) / 2` = 1 сверху.
#[test]
fn flat_ground_gets_light_one_in_the_sun_and_one_minus_shadow_in_the_shade() {
    for shadow in [0.0, 0.25, 0.4, 1.0] {
        for sun_height in [10.0, 50.0, 90.0] {
            let light = LightConfig {
                shadow,
                sun_height,
                ..LightConfig::default()
            };
            let (sun, sky) = light_colors(&light);
            let on_flat_ground = luminance(sun) * light.direction()[2] as f32;
            let sky_on_top = luminance(sky);
            near(sky_on_top, (1.0 - shadow) as f32, "тень");
            near(on_flat_ground + sky_on_top, 1.0, "солнце");
        }
    }
}

#[test]
fn a_light_color_sets_only_the_hue_it_is_divided_by_its_own_luminance() {
    let light = LightConfig {
        sun_color: [1.0, 0.5, 0.25],
        sky_color: [0.1, 0.2, 0.9],
        ..LightConfig::default()
    };
    let (sun, sky) = light_colors(&light);
    let strength = light.shadow as f32 / light.direction()[2] as f32;
    near(luminance(sun), strength, "сила солнца");
    near(luminance(sky), 1.0 - light.shadow as f32, "сила неба");
    let linear = |c: f32| ((c + 0.055) / 1.055).powf(2.4);
    near(
        sun[0] / sun[1],
        1.0 / linear(0.5),
        "оттенок солнца сохранён",
    );
    near(
        sky[2] / sky[0],
        linear(0.9) / linear(0.1),
        "оттенок неба сохранён",
    );
}

#[test]
fn a_black_light_source_does_not_shine() {
    let both_black = LightConfig {
        sun_color: [0.0; 3],
        sky_color: [0.0; 3],
        ..LightConfig::default()
    };
    assert_eq!(light_colors(&both_black), ([0.0; 3], [0.0; 3]));
    let no_sun = LightConfig {
        sun_color: [0.0; 3],
        ..LightConfig::default()
    };
    let (sun, sky) = light_colors(&no_sun);
    assert_eq!(sun, [0.0; 3]);
    assert!(luminance(sky) > 0.0, "небо светит");
    let no_sky = LightConfig {
        sky_color: [0.0; 3],
        ..LightConfig::default()
    };
    let (sun, sky) = light_colors(&no_sky);
    assert_eq!(sky, [0.0; 3]);
    assert!(luminance(sun) > 0.0, "солнце светит");
}

#[test]
fn a_shadow_of_zero_leaves_only_the_sky_and_a_shadow_of_one_leaves_only_the_sun() {
    let only_sky = LightConfig {
        shadow: 0.0,
        ..LightConfig::default()
    };
    let (sun, sky) = light_colors(&only_sky);
    assert_eq!(sun, [0.0; 3]);
    near(luminance(sky), 1.0, "небо");
    let only_sun = LightConfig {
        shadow: 1.0,
        ..LightConfig::default()
    };
    let (sun, sky) = light_colors(&only_sun);
    assert_eq!(sky, [0.0; 3]);
    assert!(luminance(sun) > 0.0);
}

#[test]
fn the_frame_carries_the_eye_and_the_light_of_the_scene() {
    let game = Setup::default().game();
    let camera = game.camera_3d([1920.0, 1080.0]).expect("камера");
    let frame = compose_frame3d(&game, &camera, 0.0, &[], &[]);
    assert_eq!(frame.eye, camera.eye.map(|c| c as f32));
    let (sun, sky) = light_colors(&game.scene.light);
    assert_eq!((frame.sun_light, frame.sky_light), (sun, sky));
}

// -------------------------------------------------------------------------------------------
// files.materials
// -------------------------------------------------------------------------------------------

#[test]
fn a_material_table_names_the_materials_in_the_order_they_are_written() {
    let setup = Setup {
        materials: Some(format!(
            "{},{}",
            material_json("rock"),
            material_json("grass")
        )),
        covers: Some(r#"[{"material":"rock"}]"#.to_string()),
        maps: [maps_of("rock", 512), maps_of("grass", 512)].concat(),
        masks: Vec::new(),
        ..Setup::default()
    };
    let (config, _) = read_entry(&setup.game_json()).expect("game.json разбирается");
    let names: Vec<&str> = config
        .files
        .materials
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["rock", "grass"], "порядок объявления, а не алфавит");
    let rock = &config.files.materials[0];
    assert_eq!(rock.size, 2.0);
    assert_eq!(rock.ao.as_deref(), Some("rock/ao.jpg"));
}

#[test]
fn a_material_without_ao_is_fine() {
    let table = r#""grass":{"size":2,"color":"g/c.jpg","normal":"g/n.jpg","roughness":"g/r.jpg","height":"g/h.jpg"}"#;
    let setup = Setup {
        materials: Some(table.to_string()),
        covers: Some(r#"[{"material":"grass"}]"#.to_string()),
        maps: ["c", "n", "r", "h"]
            .iter()
            .map(|file| (format!("g/{file}.jpg"), ok_map(512)))
            .collect(),
        masks: Vec::new(),
        ..Setup::default()
    };
    let (_, warnings) = setup.load().expect("ao необязательна");
    assert_eq!(warnings, Vec::new());
}

#[test]
fn files_materials_in_a_flat_scene_is_an_error() {
    let setup = Setup {
        camera: false,
        terrain_file: false,
        covers: None,
        ..Setup::default()
    };
    expect_error(
        setup.load(),
        &["files → materials", "только в трёхмерной сцене"],
    );
}

/// Таблица материалов `grass` и `earth`, у `grass` правится объект описания.
fn edited_table(edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>)) -> String {
    let mut table: serde_json::Value = serde_json::from_str(&format!(
        "{{{},{}}}",
        material_json("grass"),
        material_json("earth")
    ))
    .expect("таблица разбирается");
    edit(table["grass"].as_object_mut().expect("описание — объект"));
    let text = table.to_string();
    text[1..text.len() - 1].to_string()
}

fn entry_errors(setup: &Setup) -> Vec<GameError> {
    read_entry(&setup.game_json())
        .expect_err("game.json должен не разобраться")
        .errors
}

#[test]
fn a_material_table_that_is_not_a_table_or_has_an_empty_name_is_an_error() {
    let table = Setup::default().materials.expect("таблица");
    let with = |replacement: &str| Setup::default().game_json().replace(&table, replacement);
    assert!(read_entry(&with("")).is_ok(), "пустая таблица — не ошибка");
    let not_a_table = Setup::default()
        .game_json()
        .replace(r#""materials":{"#, r#""materials":[1],"x":{"#);
    let errors = read_entry(&not_a_table)
        .expect_err("таблица — массив")
        .errors;
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("ожидалась таблица")),
        "{errors:?}"
    );
    let errors = read_entry(&with(r#""":{"size":1}"#))
        .expect_err("пустое имя")
        .errors;
    assert!(
        errors.iter().any(|e| e.message.contains("пустое имя")),
        "{errors:?}"
    );
}

#[test]
fn an_unknown_key_in_a_material_is_an_error_naming_it() {
    let setup = Setup {
        materials: Some(edited_table(|grass| {
            grass.insert("sheen".to_string(), 1.into());
        })),
        ..Setup::default()
    };
    expect_error(
        setup.load(),
        &["files → materials → grass → sheen", "неизвестное поле"],
    );
}

#[test]
fn a_material_without_a_required_map_is_an_error_naming_the_map() {
    for key in ["color", "normal", "roughness", "height"] {
        let setup = Setup {
            materials: Some(edited_table(|grass| {
                grass.remove(key);
            })),
            ..Setup::default()
        };
        let errors = entry_errors(&setup);
        assert!(
            errors.iter().any(|e| e.path == "files → materials → grass"
                && e.message.contains(&format!("\"{key}\""))
                && e.message.contains("отсутствует обязательная настройка")),
            "{key}: {errors:?}"
        );
    }
}

#[test]
fn a_size_that_is_not_a_number_above_zero_is_an_error() {
    for size in ["0", "-1", r#""wide""#, "null"] {
        let setup = Setup {
            materials: Some(edited_table(|grass| {
                grass.insert(
                    "size".to_string(),
                    serde_json::from_str(size).expect("значение"),
                );
            })),
            ..Setup::default()
        };
        let errors = entry_errors(&setup);
        assert!(
            errors
                .iter()
                .any(|e| e.path == "files → materials → grass → size"),
            "{size}: {errors:?}"
        );
    }
}

#[test]
fn a_map_that_is_not_png_or_jpg_is_an_error_and_the_extension_may_be_upper_case() {
    for path in ["grass/color.tga", "grass/color.jpeg", "grass/color"] {
        let table = material_json("grass").replace("grass/color.jpg", path);
        let setup = Setup {
            materials: Some(table),
            ..Setup::default()
        };
        let failure = read_entry(&setup.game_json()).expect_err(path);
        assert!(
            failure
                .errors
                .iter()
                .any(|e| e.path == "files → materials → grass → color" && e.message.contains(path)),
            "{path}: {:?}",
            failure.errors
        );
    }
    let upper = material_json("grass").replace("grass/color.jpg", "grass/color.JPG");
    let setup = Setup {
        materials: Some(format!("{upper},{}", material_json("earth"))),
        maps: {
            let mut maps = maps_of("grass", 512);
            maps[0].0 = "grass/color.JPG".to_string();
            maps.extend(maps_of("earth", 512));
            maps
        },
        ..Setup::default()
    };
    setup.load().expect("заглавные буквы не мешают");
}

#[test]
fn a_map_file_that_is_missing_or_was_not_decoded_is_an_error_naming_the_file() {
    let mut missing = Setup::default();
    missing.maps.retain(|(path, _)| path != "grass/normal.jpg");
    expect_error(
        missing.load(),
        &[
            "files → materials → grass → normal",
            "grass/normal.jpg",
            "не найден",
        ],
    );

    let mut declared_missing = Setup::default();
    declared_missing.maps[1].1 = ImageVerdict::Missing;
    expect_error(declared_missing.load(), &["grass/normal.jpg", "не найден"]);

    let mut rejected = Setup::default();
    rejected.maps[2].1 = ImageVerdict::Rejected;
    expect_error(
        rejected.load(),
        &[
            "files → materials → grass → roughness",
            "не берётся разжимать",
        ],
    );
}

#[test]
fn a_map_that_is_not_a_square_of_512_1024_or_2048_is_an_error() {
    for (width, height) in [(512, 256), (256, 256), (4096, 4096), (600, 600)] {
        let mut setup = Setup::default();
        setup.maps[3].1 = ImageVerdict::Ok {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        };
        expect_error(
            setup.load(),
            &[
                "files → materials → grass → height",
                "нужен квадрат 512, 1024 или 2048",
            ],
        );
    }
}

#[test]
fn maps_of_different_sizes_are_an_error() {
    let mut setup = Setup::default();
    setup.maps[6].1 = ok_map(1024);
    expect_error(setup.load(), &["earth", "одного размера"]);
    let mut all_big = Setup::default();
    all_big
        .maps
        .iter_mut()
        .for_each(|(_, verdict)| *verdict = ok_map(1024));
    all_big.load().expect("все карты одного размера — 1024");
}

#[test]
fn pixels_that_do_not_match_the_size_are_an_error() {
    let mut setup = Setup::default();
    setup.maps[0].1 = ImageVerdict::Ok {
        width: 512,
        height: 512,
        pixels: vec![0; 10],
    };
    expect_error(setup.load(), &["grass/color.jpg", "байт точек"]);
}

// -------------------------------------------------------------------------------------------
// covers
// -------------------------------------------------------------------------------------------

fn with_covers(covers: &str) -> Setup {
    Setup {
        covers: Some(covers.to_string()),
        ..Setup::default()
    }
}

#[test]
fn covers_that_are_not_a_list_or_are_empty_are_an_error() {
    expect_error(
        with_covers(r#"{"material":"grass"}"#).load(),
        &["covers", "ожидался массив"],
    );
    expect_error(with_covers("[]").load(), &["covers пуст"]);
}

#[test]
fn an_unknown_key_in_a_cover_layer_is_an_error_naming_it() {
    expect_error(
        with_covers(r#"[{"material":"grass","opacity":0.5}]"#).load(),
        &["covers[0] → opacity", "неизвестное поле"],
    );
}

#[test]
fn a_cover_layer_of_an_undeclared_material_is_an_error() {
    expect_error(
        with_covers(r#"[{"material":"snow"}]"#).load(),
        &["covers[0] → material", "snow", "не объявлен"],
    );
    let layer_without_material = with_covers(r#"[{}]"#);
    expect_error(layer_without_material.load(), &["covers[0]", "material"]);
}

#[test]
fn the_first_layer_takes_no_mask_and_the_next_ones_need_one() {
    expect_error(
        with_covers(&format!(r#"[{{"material":"grass","mask":"{MASK}"}}]"#)).load(),
        &["covers[0] → mask", "у нижнего слоя маски нет"],
    );
    expect_error(
        with_covers(r#"[{"material":"grass"},{"material":"earth"}]"#).load(),
        &["covers[1]", "mask"],
    );
}

#[test]
fn a_mask_that_is_not_a_png_is_an_error() {
    expect_error(
        with_covers(r#"[{"material":"grass"},{"material":"earth","mask":"terrain/earth.jpg"}]"#)
            .load(),
        &["covers[1] → mask", "terrain/earth.jpg", "только из PNG"],
    );
}

#[test]
fn a_mask_file_that_is_missing_or_was_not_decoded_is_an_error_naming_the_file() {
    let mut missing = Setup::default();
    missing.masks.clear();
    expect_error(missing.load(), &["covers[1] → mask", MASK, "не найден"]);
    let mut rejected = Setup::default();
    rejected.masks[0].1 = ImageVerdict::Rejected;
    expect_error(
        rejected.load(),
        &["covers[1] → mask", "не берётся разжимать"],
    );
}

#[test]
fn more_than_eight_cover_layers_are_an_error_and_eight_are_fine() {
    let layers = |count: usize| {
        let masked = (1..count).map(|_| format!(r#"{{"material":"earth","mask":"{MASK}"}}"#));
        let all: Vec<String> = std::iter::once(r#"{"material":"grass"}"#.to_string())
            .chain(masked)
            .collect();
        format!("[{}]", all.join(","))
    };
    with_covers(&layers(8)).load().expect("восемь слоёв");
    expect_error(with_covers(&layers(9)).load(), &["covers", "больше 8"]);
}

#[test]
fn a_mask_of_any_size_including_one_by_one_is_accepted() {
    for (width, height) in [(1, 1), (7, 3), (512, 384)] {
        let mut setup = Setup::default();
        setup.masks[0].1 = ImageVerdict::Ok {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        };
        setup
            .load()
            .unwrap_or_else(|f| panic!("{width}×{height}: {:?}", f.errors));
    }
}

#[test]
fn covers_number_their_materials_by_declaration_and_their_masks_by_layer() {
    let setup = Setup {
        covers: Some(format!(
            r#"[{{"material":"earth"}},{{"material":"grass","mask":"{MASK}"}},{{"material":"earth","mask":"{MASK}"}}]"#
        )),
        ..Setup::default()
    };
    let game = setup.game();
    assert_eq!(
        game.world.terrain().covers(),
        [
            Cover {
                material: 1,
                mask: None
            },
            Cover {
                material: 0,
                mask: Some(0)
            },
            Cover {
                material: 1,
                mask: Some(1)
            },
        ]
    );
}

#[test]
fn a_terrain_without_covers_has_none_and_a_one_layer_terrain_has_no_masks() {
    let bare = Setup {
        covers: None,
        ..Setup::default()
    };
    assert_eq!(bare.game().world.terrain().covers(), []);
    let single = Setup {
        covers: Some(r#"[{"material":"grass"}]"#.to_string()),
        masks: Vec::new(),
        ..Setup::default()
    };
    let game = single.game();
    assert_eq!(
        game.world.terrain().covers(),
        [Cover {
            material: 0,
            mask: None
        }]
    );
    let texts = read_texts(
        &read_entry(&single.game_json()).expect("game.json").0,
        None,
        None,
        single.terrain().as_deref(),
    );
    assert_eq!(texts.masks, Vec::<String>::new());
}

#[test]
fn a_brush_stroke_changes_the_heights_and_the_water_and_keeps_the_covers() {
    let mut game = Setup::default().game();
    let covers = game.world.terrain().covers().to_vec();
    assert_eq!(covers.len(), 2);
    let mut heights = vec![0.0; 25];
    heights[12] = 1.5;
    edit::set_terrain(&mut game, &heights, Some((-1.0, "#3f7fd0"))).expect("правка рельефа");
    let terrain = game.world.terrain();
    assert_eq!(terrain.covers(), covers);
    assert_eq!(terrain.heights()[12], 1.5);
    assert!(terrain.water().is_some());
    edit::set_terrain(&mut game, &heights, None).expect("правка без воды");
    assert_eq!(game.world.terrain().covers(), covers);
}

#[test]
fn a_brush_stroke_on_a_terrain_without_covers_does_not_make_any() {
    let mut game = Setup {
        covers: None,
        ..Setup::default()
    }
    .game();
    edit::set_terrain(&mut game, &[0.5; 25], None).expect("правка рельефа");
    assert_eq!(game.world.terrain().covers(), []);
}

// -------------------------------------------------------------------------------------------
// Предупреждение
// -------------------------------------------------------------------------------------------

#[test]
fn a_material_that_no_layer_names_is_a_warning_and_the_game_runs() {
    let setup = Setup {
        covers: Some(r#"[{"material":"grass"}]"#.to_string()),
        masks: Vec::new(),
        ..Setup::default()
    };
    let (_, warnings) = setup.load().expect("предупреждение не мешает игре");
    let unused = material_warnings(&warnings);
    assert_eq!(unused.len(), 1, "{warnings:?}");
    assert_eq!(unused[0].path, "files → materials → earth");
    assert!(
        unused[0]
            .message
            .contains("не назван ни в одном слое covers")
    );
}

#[test]
fn materials_without_a_terrain_file_or_without_covers_warn_about_each_material() {
    let no_file = Setup {
        terrain_file: false,
        covers: None,
        ..Setup::default()
    };
    let (_, warnings) = no_file.load().expect("игра идёт");
    assert_eq!(material_warnings(&warnings).len(), 2, "{warnings:?}");
    let no_covers = Setup {
        covers: None,
        ..Setup::default()
    };
    let (_, warnings) = no_covers.load().expect("игра идёт");
    assert_eq!(material_warnings(&warnings).len(), 2, "{warnings:?}");
}

#[test]
fn the_maps_of_a_material_that_no_layer_names_are_still_checked() {
    let mut setup = Setup {
        covers: Some(r#"[{"material":"grass"}]"#.to_string()),
        masks: Vec::new(),
        ..Setup::default()
    };
    setup.maps.retain(|(path, _)| path != "earth/height.jpg");
    expect_error(
        setup.load(),
        &["files → materials → earth → height", "не найден"],
    );
}

// -------------------------------------------------------------------------------------------
// read_texts
// -------------------------------------------------------------------------------------------

#[test]
fn read_texts_lists_every_map_of_every_material_and_every_mask_in_order() {
    let setup = Setup {
        materials: Some(format!(
            "{},{}",
            material_json("rock"),
            material_json("grass")
        )),
        covers: Some(
            r#"[{"material":"grass"},{"material":"rock","mask":"terrain/a.png"},{"material":"rock","mask":"terrain/b.png"}]"#
                .to_string(),
        ),
        ..Setup::default()
    };
    let (config, _) = read_entry(&setup.game_json()).expect("game.json");
    let texts = read_texts(&config, None, None, setup.terrain().as_deref());
    assert_eq!(
        texts.materials,
        [
            "rock/color.jpg",
            "rock/normal.jpg",
            "rock/roughness.png",
            "rock/height.jpg",
            "rock/ao.jpg",
            "grass/color.jpg",
            "grass/normal.jpg",
            "grass/roughness.png",
            "grass/height.jpg",
            "grass/ao.jpg",
        ]
    );
    assert_eq!(texts.masks, ["terrain/a.png", "terrain/b.png"]);
}

#[test]
fn read_texts_lists_no_masks_without_a_terrain_text_or_with_one_it_cannot_parse() {
    let setup = Setup::default();
    let (config, _) = read_entry(&setup.game_json()).expect("game.json");
    assert_eq!(
        read_texts(&config, None, None, None).masks,
        Vec::<String>::new()
    );
    assert_eq!(
        read_texts(&config, None, None, Some("{ not json")).masks,
        Vec::<String>::new()
    );
    assert_eq!(
        read_texts(&config, None, None, Some(&terrain_text(None))).masks,
        Vec::<String>::new()
    );
    assert_eq!(
        read_texts(&config, None, None, Some(&terrain_text(Some(r#"{"a":1}"#)))).masks,
        Vec::<String>::new()
    );
    let listed = read_texts(&config, None, None, setup.terrain().as_deref());
    assert_eq!(listed.masks, [MASK]);
    assert_eq!(listed.materials.len(), 10);
}

#[test]
fn a_game_without_materials_asks_for_no_maps() {
    let setup = Setup {
        materials: None,
        terrain_file: false,
        covers: None,
        maps: Vec::new(),
        masks: Vec::new(),
        ..Setup::default()
    };
    let (config, _) = read_entry(&setup.game_json()).expect("game.json");
    let texts = read_texts(&config, None, None, None);
    assert_eq!((texts.materials, texts.masks), (Vec::new(), Vec::new()));
}

// -------------------------------------------------------------------------------------------
// Упаковка карт и масок
// -------------------------------------------------------------------------------------------

struct Maps {
    color: Vec<u8>,
    normal: Vec<u8>,
    roughness: Vec<u8>,
    height: Vec<u8>,
    ao: Option<Vec<u8>>,
}

impl Maps {
    /// Карты `side × side`: каждая точка — `fill(номер точки)`.
    fn new(side: u32, ao: bool, fill: impl Fn(usize) -> [[u8; 4]; 5]) -> Maps {
        let points = (side * side) as usize;
        let mut maps = Maps {
            color: Vec::new(),
            normal: Vec::new(),
            roughness: Vec::new(),
            height: Vec::new(),
            ao: ao.then(Vec::new),
        };
        for point in 0..points {
            let [color, normal, roughness, height, occlusion] = fill(point);
            maps.color.extend(color);
            maps.normal.extend(normal);
            maps.roughness.extend(roughness);
            maps.height.extend(height);
            if let Some(ao) = maps.ao.as_mut() {
                ao.extend(occlusion);
            }
        }
        maps
    }

    fn pack(&self, side: u32) -> PackedMaterial {
        pack_material(&MaterialMaps {
            color: map_view(side, side, &self.color),
            normal: map_view(side, side, &self.normal),
            roughness: map_view(side, side, &self.roughness),
            height: map_view(side, side, &self.height),
            ao: self
                .ao
                .as_deref()
                .map(|pixels| map_view(side, side, pixels)),
        })
    }
}

/// Байт нормали для наклона `tilt` от −1 до 1.
fn tilt_byte(tilt: f32) -> u8 {
    ((tilt * 0.5 + 0.5) * 255.0).round() as u8
}

#[test]
fn the_packed_level_zero_puts_each_map_in_its_own_channel() {
    let maps = Maps::new(4, true, |point| {
        let p = point as u8;
        [
            [10 + p, 20 + p, 30 + p, 99],
            [40 + p, 50 + p, 60, 99],
            [70 + p, 1, 2, 3],
            [100 + p, 4, 5, 6],
            [130 + p, 7, 8, 9],
        ]
    });
    let packed = maps.pack(4);
    assert_eq!(packed.side, 4);
    for point in 0..16usize {
        let p = point as u8;
        assert_eq!(
            packed.color[0][point * 4..point * 4 + 4],
            [10 + p, 20 + p, 30 + p, 100 + p],
            "цвет и высота"
        );
        assert_eq!(
            packed.data[0][point * 4..point * 4 + 4],
            [40 + p, 50 + p, 70 + p, 130 + p],
            "нормаль xy, шероховатость и затенение"
        );
    }
}

#[test]
fn without_an_ao_map_the_occlusion_is_255() {
    let maps = Maps::new(4, false, |_| {
        [
            [1, 2, 3, 255],
            [4, 5, 6, 255],
            [7, 8, 9, 255],
            [10, 11, 12, 255],
            [0; 4],
        ]
    });
    let packed = maps.pack(4);
    assert!(packed.data[0].chunks(4).all(|point| point[3] == 255));
}

#[test]
fn a_gray_map_takes_its_red_channel_even_with_alpha_or_color() {
    let maps = Maps::new(4, true, |_| {
        [
            [1, 2, 3, 255],
            [128, 128, 255, 255],
            [200, 9, 9, 0],
            [50, 250, 250, 3],
            [90, 1, 2, 0],
        ]
    });
    let packed = maps.pack(4);
    assert_eq!(packed.data[0][2], 200, "шероховатость — красный канал");
    assert_eq!(packed.color[0][3], 50, "высота — красный канал");
    assert_eq!(packed.data[0][3], 90, "затенение — красный канал");
}

#[test]
fn the_levels_go_down_to_one_by_one() {
    assert_eq!(level_count(1024), 11);
    assert_eq!(level_count(4), 3);
    assert_eq!(level_count(1), 1);
    assert_eq!(
        (level_side(1024, 0), level_side(1024, 10), level_side(4, 2)),
        (1024, 1, 1)
    );
    let maps = Maps::new(4, true, |_| [[9; 4]; 5]);
    let packed = maps.pack(4);
    let sizes: Vec<usize> = packed.color.iter().map(Vec::len).collect();
    assert_eq!(sizes, [4 * 4 * 4, 2 * 2 * 4, 4]);
    assert_eq!(packed.data.iter().map(Vec::len).collect::<Vec<_>>(), sizes);
}

#[test]
fn a_smaller_level_averages_color_in_linear_light_and_height_as_it_is() {
    // Блок 2 × 2 в левом верхнем углу: две чёрные и две белые точки, высоты 0 и 255.
    let maps = Maps::new(4, true, |point| {
        let (x, y) = (point % 4, point / 4);
        let white = x < 2 && y < 2 && (x + y) % 2 == 0;
        let (value, height) = if x < 2 && y < 2 && white {
            (255, 255)
        } else {
            (0, 0)
        };
        [
            [value, value, value, 255],
            [128, 128, 255, 255],
            [0; 4],
            [height, 0, 0, 255],
            [255; 4],
        ]
    });
    let packed = maps.pack(4);
    let level_one = &packed.color[1];
    let red = i32::from(level_one[0]);
    assert!(
        (red - 188).abs() <= 1,
        "средний серый в линейной яркости — 188, не 128: {red}"
    );
    let height = i32::from(level_one[3]);
    assert!(
        (height - 128).abs() <= 1,
        "высота усреднена как есть: {height}"
    );
    assert_eq!(&level_one[4..8], [0, 0, 0, 0], "соседний блок чёрный");
}

#[test]
fn averaged_normals_are_unit_again_and_opposite_tilts_cancel() {
    // Блок 2 × 2: две нормали с наклоном 0,6 по x и две ровные.
    let tilted = |point: usize| {
        let (x, y) = (point % 4, point / 4);
        let normal = if x < 2 && y < 2 && x == 0 {
            [tilt_byte(0.6), tilt_byte(0.0), 0, 255]
        } else {
            [tilt_byte(0.0), tilt_byte(0.0), 0, 255]
        };
        [[0, 0, 0, 255], normal, [0; 4], [0, 0, 0, 255], [255; 4]]
    };
    let level = Maps::new(4, true, tilted).pack(4).data[1].clone();
    let x = f32::from(level[0]) / 255.0 * 2.0 - 1.0;
    let y = f32::from(level[1]) / 255.0 * 2.0 - 1.0;
    // Сумма (0,6; 0; 0,8) и (0; 0; 1), по два раза, приведена к единице: наклон 1,2 / |(1,2; 0; 3,6)|.
    let expected = 1.2 / (1.2f32 * 1.2 + 3.6 * 3.6).sqrt();
    assert!(
        (x - expected).abs() < 0.01,
        "{x} против {expected}: обычное среднее дало бы 0,3"
    );
    assert!(y.abs() < 0.01);
    assert!(
        x * x + y * y <= 1.0,
        "z восстанавливается, нормаль единичная"
    );

    let opposite = |point: usize| {
        let sign = if point.is_multiple_of(2) { 0.6 } else { -0.6 };
        [
            [0, 0, 0, 255],
            [tilt_byte(sign), tilt_byte(0.0), 0, 255],
            [0; 4],
            [0, 0, 0, 255],
            [255; 4],
        ]
    };
    let flat = Maps::new(4, true, opposite).pack(4).data[1].clone();
    assert!((i32::from(flat[0]) - 128).abs() <= 1 && (i32::from(flat[1]) - 128).abs() <= 1);
}

fn gray(width: u32, height: u32, value: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .flat_map(|(x, y)| [value(x, y), 9, 9, 255])
        .collect()
}

fn map_view(width: u32, height: u32, pixels: &[u8]) -> MapView<'_> {
    MapView {
        width,
        height,
        pixels,
    }
}

#[test]
fn masks_are_brought_to_the_size_of_the_biggest_and_packed_four_to_a_layer() {
    let small = gray(2, 2, |_, _| 10);
    let big = gray(4, 4, |_, _| 20);
    let single = gray(1, 1, |_, _| 200);
    let third = gray(4, 2, |_, _| 30);
    let fourth = gray(4, 4, |_, _| 40);
    let fifth = gray(3, 3, |_, _| 50);
    let packed = pack_masks(
        &[
            map_view(2, 2, &small),
            map_view(4, 4, &big),
            map_view(1, 1, &single),
            map_view(4, 2, &third),
            map_view(4, 4, &fourth),
            map_view(3, 3, &fifth),
        ],
        None,
    );
    assert_eq!((packed.width, packed.height), (4, 4));
    assert_eq!(packed.layers.len(), 2, "шесть масок — два слоя");
    let point = |layer: usize, x: usize, y: usize| {
        &packed.layers[layer][(y * 4 + x) * 4..(y * 4 + x) * 4 + 4]
    };
    assert_eq!(
        point(0, 2, 3),
        [10, 20, 200, 30],
        "маски 0–3 — каналы первого слоя"
    );
    assert_eq!(
        point(1, 1, 1),
        [40, 50, 0, 0],
        "маски 4 и 5 — первые каналы второго, остальные пусты"
    );
}

#[test]
fn a_smaller_mask_is_stretched_bilinearly_over_the_whole_scene() {
    let ramp = gray(2, 1, |x, _| if x == 0 { 0 } else { 255 });
    let packed = pack_masks(
        &[map_view(2, 1, &ramp), map_view(4, 1, &gray(4, 1, |_, _| 1))],
        None,
    );
    let row: Vec<u8> = (0..4).map(|x| packed.layers[0][x * 4]).collect();
    assert_eq!(row, [0, 64, 191, 255]);
}

#[test]
fn a_mask_that_is_one_by_one_is_a_whole_scene_of_one_value() {
    let one = gray(1, 1, |_, _| 77);
    let packed = pack_masks(&[map_view(1, 1, &one)], None);
    assert_eq!((packed.width, packed.height), (1, 1));
    assert_eq!(packed.layers[0][0], 77);
}

#[test]
fn no_masks_still_make_a_layer_and_a_huge_mask_is_capped_to_what_the_card_holds() {
    let none = pack_masks(&[], None);
    assert_eq!((none.width, none.height, none.layers.len()), (1, 1, 1));
    let huge = gray(4096, 2, |x, _| (x % 250) as u8);
    let capped = pack_masks(&[map_view(4096, 2, &huge)], None);
    assert_eq!((capped.width, capped.height), (2048, 2));
}

#[test]
fn the_layer_counts_of_the_arrays_avoid_one_and_multiples_of_six_on_webgl2() {
    for (needed, safe) in [(0, 2), (1, 2), (2, 2), (5, 5), (6, 7), (12, 13)] {
        assert_eq!(webgl2_safe_layer_count(needed), safe, "{needed}");
    }
}

#[test]
fn a_relief_needs_every_map_and_every_mask() {
    let decls: Vec<MaterialDecl> = read_entry(&Setup::default().game_json())
        .expect("game.json")
        .0
        .files
        .materials;
    let setup = Setup::default();
    let covers = setup.game().world.terrain().covers().to_vec();
    let paths = vec![MASK.to_string()];
    let whole = Relief::new(
        &decls,
        &setup.maps,
        &covers,
        false,
        &paths,
        &setup.masks,
        [2, 2],
    )
    .expect("все карты и маски на месте");
    assert_eq!(
        (whole.side, whole.materials.len(), whole.masks.len()),
        (512, 2, 1)
    );
    assert_eq!(whole.table[0], [2.0, 2.0, 2.0, 0.0]);
    assert_eq!(
        whole.table[2],
        [1.0, 0.5, 0.0, 0.0],
        "второй слой — earth, size 2"
    );

    let mut lacking = Setup::default();
    lacking.maps.retain(|(path, _)| path != "earth/ao.jpg");
    assert!(
        Relief::new(
            &decls,
            &lacking.maps,
            &covers,
            false,
            &paths,
            &setup.masks,
            [2, 2]
        )
        .is_none()
    );
    assert!(Relief::new(&decls, &setup.maps, &covers, false, &paths, &[], [2, 2]).is_none());
}

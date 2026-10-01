//! Фаза 20 — горы-штампы. Игры собираются в коде теста, как в `phase19_light_materials.rs`:
//! `files.stamps` и файлы штампов, горы `stamps` файла рельефа и все проверки перед запуском, итоговая
//! высота земли и всё, что её читает, правило крутизны `slope` у слоя покрытий, вызовы редактора
//! `set_terrain`, `terrain_heights`, `stamp_at`, заход `read_entry` и `read_texts`. Настоящая деревня
//! с горами — в `rpg_game.rs`, шейдер — в `phase15_3d_scene_shapes.rs`.

use engine::core::camera::Camera3d;
use engine::core::game::Game;
use engine::core::scene::{stamp_hit, terrain_hit};
use engine::core::terrain::{Cover, Terrain};
use engine::data::edit::{self, TerrainHeights};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageVerdict, load_rest_with_stamps, read_entry, read_texts};
use engine::render::materials::{COVER_TABLE_LEN, Relief};
use serde_json::{Value as Json, json};

const WINDOW: [f64; 2] = [1920.0, 1080.0];

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

const PROPS: &str = r#"{"properties":{}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const MASK: &str = "terrain/mask.png";

/// Клин: от нуля в северо-западном углу до единицы в юго-восточном.
const WEDGE: &str = r#"{"heights":[[0,0.5],[0.5,1]]}"#;
/// Пирамида с вершиной посередине.
const PYRAMID: &str = r#"{"heights":[[0,0,0],[0,1,0],[0,0,0]]}"#;

fn material_json(name: &str) -> String {
    format!(
        r#""{name}":{{"size":2,"color":"{name}/c.jpg","normal":"{name}/n.jpg","roughness":"{name}/r.jpg","height":"{name}/h.jpg"}}"#
    )
}

fn ok_map(side: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width: side,
        height: side,
        pixels: vec![0; (side * side * 4) as usize],
    }
}

fn maps_of(name: &str) -> Vec<(String, ImageVerdict)> {
    ["c.jpg", "n.jpg", "r.jpg", "h.jpg"]
        .iter()
        .map(|file| (format!("{name}/{file}"), ok_map(512)))
        .collect()
}

/// Файл рельефа сцены 20 × 12 клеток: точки через полклетки, высота `base` везде.
fn terrain_text(base: f64, covers: Option<&str>, stamps: Option<&str>) -> String {
    let row = format!("[{}]", vec![base.to_string(); 41].join(","));
    let covers = covers.map_or_else(String::new, |covers| format!(r#""covers":{covers},"#));
    let stamps = stamps.map_or_else(String::new, |stamps| format!(r#""stamps":{stamps},"#));
    format!(
        r#"{{{covers}{stamps}"heights":[{}]}}"#,
        vec![row; 25].join(",")
    )
}

fn mountain(stamp: &str, position: [f64; 2], size: [f64; 2], height: f64) -> String {
    format!(
        r#"{{"stamp":"{stamp}","position":[{},{}],"size":[{},{}],"height":{height}}}"#,
        position[0], position[1], size[0], size[1]
    )
}

fn list(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

/// Игра 20 × 12 клеток из двух штампов, `wedge` и `pyramid`, двух материалов и рельефа с покрытиями
/// и по горе каждого штампа — по умолчанию исправная; каждый тест портит своё.
struct Setup {
    camera: bool,
    /// Таблица `files.stamps` в тексте `game.json`; `None` — ключа нет.
    declared: Option<String>,
    texts: Vec<(String, Option<String>)>,
    covers: Option<String>,
    /// Значение `stamps` файла рельефа; `None` — ключа нет.
    stamps: Option<String>,
    base: f64,
    objects: String,
    masks: Vec<(String, ImageVerdict)>,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup {
            camera: true,
            declared: Some(r#""wedge":"stamps/wedge.json","pyramid":"stamps/pyramid.json""#.into()),
            texts: vec![
                ("wedge".into(), Some(WEDGE.into())),
                ("pyramid".into(), Some(PYRAMID.into())),
            ],
            covers: Some(r#"[{"material":"grass"},{"material":"earth","slope":30}]"#.into()),
            stamps: Some(list(&[
                mountain("wedge", [4.0, 6.0], [4.0, 4.0], 3.0),
                mountain("pyramid", [14.0, 6.0], [8.0, 8.0], 4.0),
            ])),
            base: 0.0,
            objects: String::new(),
            masks: vec![(MASK.to_string(), ok_map(4))],
        }
    }
}

impl Setup {
    fn with_stamp(mut self, name: &str, text: &str) -> Setup {
        let slot = self
            .texts
            .iter_mut()
            .find(|(n, _)| n == name)
            .expect("штамп");
        slot.1 = Some(text.to_string());
        self
    }

    fn with_mountains(mut self, mountains: &str) -> Setup {
        self.stamps = Some(mountains.to_string());
        self
    }

    fn with_covers(mut self, covers: &str) -> Setup {
        self.covers = Some(covers.to_string());
        self
    }

    fn with_objects(mut self, objects: &str) -> Setup {
        self.objects = objects.to_string();
        self
    }

    fn game_json(&self) -> String {
        let camera = if self.camera {
            r#","view_height":12,"camera":{"pitch":55}"#
        } else {
            ""
        };
        let stamps = self
            .declared
            .as_ref()
            .map_or_else(String::new, |table| format!(r#","stamps":{{{table}}}"#));
        format!(
            r##"{{"name":"T","scene":{{"width":20,"height":12,"background":"#4f8a3c"{camera}}},
"random_seed":1,"start_screen":"main","max_objects":10,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"terrain":"terrain.json",
         "materials":{{{},{}}}{stamps}}}}}"##,
            material_json("grass"),
            material_json("earth")
        )
    }

    fn terrain(&self) -> String {
        terrain_text(self.base, self.covers.as_deref(), self.stamps.as_deref())
    }

    fn load(&self) -> Result<(Game, Vec<GameError>), LoadFailure> {
        let game_json = self.game_json();
        let (config, mut warnings) = read_entry(&game_json)?;
        let mut maps = maps_of("grass");
        maps.extend(maps_of("earth"));
        let scene = format!(r#"{{"objects":[{}]}}"#, self.objects);
        let terrain = self.terrain();
        let loaded = load_rest_with_stamps(
            &game_json,
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
            Some(&terrain),
            &maps,
            &self.masks,
            &self.texts,
        );
        let (game, _screens, more, _images) = loaded?;
        warnings.extend(more);
        Ok((game, warnings))
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

/// Тест на ошибку: игра из `$setup` не грузится, и ошибка называет всё из `$fragments`.
macro_rules! refused {
    ($name:ident, $setup:expr, $fragments:expr) => {
        #[test]
        fn $name() {
            expect_error($setup.load(), &$fragments);
        }
    };
}

fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{what}: {actual} вместо {expected}"
    );
}

/// Высота итоговой земли в месте сцены.
fn at(game: &Game, x: f64, y: f64) -> f64 {
    game.world.terrain().height_at(x, y)
}

#[test]
fn a_whole_setup_loads_without_errors_or_warnings() {
    let (game, warnings) = Setup::default().load().expect("исправная игра");
    assert_eq!(warnings, Vec::new());
    assert_eq!(game.world.terrain().mountains().len(), 2);
}

// -------------------------------------------------------------------------------------------
// files.stamps
// -------------------------------------------------------------------------------------------

#[test]
fn read_entry_lists_the_stamps_in_the_order_they_are_declared_not_alphabetically() {
    let setup = Setup {
        declared: Some(r#""wedge":"stamps/w.json","pyramid":"stamps/p.json""#.into()),
        ..Setup::default()
    };
    let (config, _) = read_entry(&setup.game_json()).expect("game.json");
    assert_eq!(
        config.files.stamps,
        [
            ("wedge".to_string(), "stamps/w.json".to_string()),
            ("pyramid".to_string(), "stamps/p.json".to_string()),
        ]
    );
}

#[test]
fn a_game_without_files_stamps_lists_none() {
    let setup = Setup {
        declared: None,
        stamps: None,
        ..Setup::default()
    };
    let (config, warnings) = read_entry(&setup.game_json()).expect("game.json");
    assert_eq!(config.files.stamps, []);
    assert_eq!(warnings, Vec::new());
}

refused!(
    files_stamps_in_a_flat_scene_is_an_error,
    Setup {
        camera: false,
        ..Setup::default()
    },
    ["files → stamps", "трёхмерной сцене"]
);

refused!(
    a_stamp_file_that_is_missing_is_an_error_naming_it,
    Setup {
        texts: vec![("pyramid".into(), Some(PYRAMID.into()))],
        ..Setup::default()
    },
    [
        "stamps/wedge.json",
        "файл не найден",
        "files → stamps → wedge"
    ]
);

refused!(
    a_stamp_file_that_is_not_json_is_an_error,
    Setup::default().with_stamp("wedge", "{ not json"),
    ["stamps/wedge.json", "не разбирается как JSON"]
);

refused!(
    an_unknown_key_in_a_stamp_file_is_an_error_naming_it,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0,1],[1,0]],"scale":2}"#),
    ["stamps/wedge.json", "scale", "неизвестное поле"]
);

refused!(
    a_stamp_file_without_heights_is_an_error,
    Setup::default().with_stamp("wedge", "{}"),
    ["stamps/wedge.json", "heights"]
);

refused!(
    stamp_rows_of_different_length_are_an_error,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0,1],[1,0,0]]}"#),
    ["stamps/wedge.json", "heights[1]", "одной длины"]
);

refused!(
    a_stamp_of_fewer_than_two_by_two_points_is_an_error,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0,1]]}"#),
    ["stamps/wedge.json", "heights", "не меньше двух на две"]
);

refused!(
    a_stamp_with_a_single_column_is_an_error,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0],[1]]}"#),
    ["stamps/wedge.json", "не меньше двух на две"]
);

refused!(
    a_stamp_height_that_is_not_a_number_is_an_error_with_its_own_place,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0,1],[1,"high"]]}"#),
    ["stamps/wedge.json", "heights[1][1]", "ожидалось число"]
);

refused!(
    a_stamp_height_above_one_is_an_error,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0,1],[1,1.5]]}"#),
    ["stamps/wedge.json", "heights[1][1]", "от 0 до 1"]
);

refused!(
    a_stamp_height_below_zero_is_an_error,
    Setup::default().with_stamp("wedge", r#"{"heights":[[0,-0.1],[1,1]]}"#),
    ["stamps/wedge.json", "heights[0][1]", "от 0 до 1"]
);

#[test]
fn a_stamp_of_two_by_two_is_a_wedge_and_a_stamp_of_zeros_lifts_nothing() {
    let wedge = one_mountain(
        "wedge",
        WEDGE,
        mountain("wedge", [10.0, 6.0], [4.0, 4.0], 2.0),
    );
    near(at(&wedge, 12.0, 8.0), 2.0, "юго-восточный угол клина");
    near(at(&wedge, 10.0, 6.0), 1.0, "середина клина");
    let zeros = one_mountain(
        "pyramid",
        r#"{"heights":[[0,0],[0,0]]}"#,
        mountain("pyramid", [10.0, 6.0], [4.0, 4.0], 5.0),
    );
    assert!(zeros.world.terrain().heights().iter().all(|&h| h == 0.0));
}

#[test]
fn a_broken_stamp_does_not_add_a_second_error_for_the_mountains_standing_on_it() {
    let errors = errors_of(Setup::default().with_stamp("wedge", "{}").load());
    assert!(
        errors.iter().all(|e| !e.contains("не объявлен")),
        "{errors:#?}"
    );
}

// -------------------------------------------------------------------------------------------
// stamps файла рельефа
// -------------------------------------------------------------------------------------------

refused!(
    stamps_that_are_not_a_list_are_an_error,
    Setup::default().with_mountains(r#"{"stamp":"wedge"}"#),
    ["stamps", "ожидался массив"]
);

refused!(
    an_unknown_key_in_a_mountain_is_an_error_naming_it,
    Setup::default().with_mountains(
        r#"[{"stamp":"wedge","position":[4,6],"size":[4,4],"height":3,"scale":2}]"#
    ),
    ["stamps[0] → scale", "неизвестное поле"]
);

refused!(
    a_mountain_without_a_stamp_is_an_error,
    Setup::default().with_mountains(r#"[{"position":[4,6],"size":[4,4],"height":3}]"#),
    ["stamps[0]", "stamp"]
);

refused!(
    a_mountain_without_a_position_is_an_error,
    Setup::default().with_mountains(r#"[{"stamp":"wedge","size":[4,4],"height":3}]"#),
    ["stamps[0]", "position"]
);

refused!(
    a_mountain_without_a_size_is_an_error,
    Setup::default().with_mountains(r#"[{"stamp":"wedge","position":[4,6],"height":3}]"#),
    ["stamps[0]", "size"]
);

refused!(
    a_mountain_without_a_height_is_an_error,
    Setup::default().with_mountains(r#"[{"stamp":"wedge","position":[4,6],"size":[4,4]}]"#),
    ["stamps[0]", "height"]
);

refused!(
    a_mountain_of_an_undeclared_stamp_is_an_error,
    Setup::default().with_mountains(&list(&[
        mountain("wedge", [4.0, 6.0], [4.0, 4.0], 3.0),
        mountain("alp", [4.0, 6.0], [4.0, 4.0], 3.0),
    ])),
    ["stamps[1] → stamp", "alp", "не объявлен"]
);

refused!(
    a_mountain_position_that_is_not_two_numbers_is_an_error,
    Setup::default()
        .with_mountains(r#"[{"stamp":"wedge","position":[4],"size":[4,4],"height":3}]"#),
    ["stamps[0] → position", "пара чисел"]
);

refused!(
    a_mountain_position_with_a_text_is_an_error,
    Setup::default()
        .with_mountains(r#"[{"stamp":"wedge","position":[4,"y"],"size":[4,4],"height":3}]"#),
    ["stamps[0] → position → [1]", "ожидалось число"]
);

refused!(
    a_mountain_size_that_is_not_two_numbers_is_an_error,
    Setup::default().with_mountains(r#"[{"stamp":"wedge","position":[4,6],"size":4,"height":3}]"#),
    ["stamps[0] → size", "ожидался массив"]
);

refused!(
    a_mountain_size_with_a_zero_side_is_an_error,
    Setup::default()
        .with_mountains(r#"[{"stamp":"wedge","position":[4,6],"size":[4,0],"height":3}]"#),
    ["stamps[0] → size", "больше нуля"]
);

refused!(
    a_mountain_height_that_is_not_a_number_is_an_error,
    Setup::default()
        .with_mountains(r#"[{"stamp":"wedge","position":[4,6],"size":[4,4],"height":"tall"}]"#),
    ["stamps[0] → height", "ожидалось число"]
);

refused!(
    a_mountain_height_of_zero_is_an_error,
    Setup::default()
        .with_mountains(r#"[{"stamp":"wedge","position":[4,6],"size":[4,4],"height":0}]"#),
    ["stamps[0] → height", "больше нуля"]
);

refused!(
    a_mountain_rotation_that_is_not_a_number_is_an_error,
    Setup::default().with_mountains(
        r#"[{"stamp":"wedge","position":[4,6],"size":[4,4],"height":3,"rotation":"left"}]"#
    ),
    ["stamps[0] → rotation", "ожидалось число"]
);

refused!(
    a_stamp_that_is_removed_from_files_stamps_while_a_mountain_stands_on_it_is_an_error,
    Setup {
        declared: Some(r#""pyramid":"stamps/pyramid.json""#.into()),
        ..Setup::default()
    },
    ["stamps[0] → stamp", "wedge", "не объявлен"]
);

#[test]
fn a_mountain_with_any_rotation_and_an_empty_list_of_mountains_are_fine() {
    let turned = Setup::default().with_mountains(
        r#"[{"stamp":"wedge","position":[4,6],"size":[4,4],"height":3,"rotation":-725.5}]"#,
    );
    let (game, _) = turned.load().expect("поворот — любое число");
    assert_eq!(game.world.terrain().mountains().len(), 1);
    let (game, warnings) = Setup::default()
        .with_mountains("[]")
        .load()
        .expect("пустой список гор");
    assert!(game.world.terrain().mountains().is_empty());
    assert!(
        warnings.iter().any(|w| w.message.contains("не поставлен")),
        "{warnings:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Предупреждение
// -------------------------------------------------------------------------------------------

#[test]
fn a_stamp_that_is_declared_and_never_placed_is_a_warning_naming_it() {
    let setup =
        Setup::default().with_mountains(&list(&[mountain("wedge", [4.0, 6.0], [4.0, 4.0], 3.0)]));
    let (_, warnings) = setup.load().expect("игра идёт");
    let unused: Vec<&GameError> = warnings
        .iter()
        .filter(|w| w.message.contains("не поставлен"))
        .collect();
    assert_eq!(unused.len(), 1, "{warnings:?}");
    assert_eq!(unused[0].path, "files → stamps → pyramid");
    assert!(unused[0].message.contains("pyramid"));
}

#[test]
fn a_game_without_stamps_in_the_terrain_file_warns_about_every_declared_stamp() {
    let setup = Setup {
        stamps: None,
        ..Setup::default()
    };
    let (_, warnings) = setup.load().expect("игра идёт");
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.message.contains("не поставлен"))
            .count(),
        2
    );
}

// -------------------------------------------------------------------------------------------
// Высота гор
// -------------------------------------------------------------------------------------------

/// Игра с одной горой: штамп `stamp` переписан текстом `text`.
fn one_mountain(stamp: &str, text: &str, mountain: String) -> Game {
    Setup::default()
        .with_stamp(stamp, text)
        .with_mountains(&list(&[mountain]))
        .game()
}

#[test]
fn the_middle_of_a_stamp_the_corner_and_a_point_outside_the_rectangle() {
    let game = one_mountain(
        "pyramid",
        PYRAMID,
        mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 4.0),
    );
    near(at(&game, 10.0, 6.0), 4.0, "середина штампа — вершина");
    near(at(&game, 6.0, 2.0), 0.0, "угол прямоугольника");
    near(at(&game, 1.0, 1.0), 0.0, "вне прямоугольника");
    near(at(&game, 14.5, 6.0), 0.0, "за краем прямоугольника");
}

#[test]
fn a_stamp_is_read_between_its_points() {
    let game = one_mountain(
        "pyramid",
        PYRAMID,
        mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 4.0),
    );
    near(at(&game, 8.0, 6.0), 2.0, "посреди пути от края до вершины");
    near(at(&game, 10.0, 4.0), 2.0, "то же по другой оси");
}

#[test]
fn a_mountain_turned_by_ninety_degrees_stands_on_the_turned_rectangle() {
    let turned =
        r#"{"stamp":"wedge","position":[10,6],"size":[8,4],"height":4,"rotation":90}"#.to_string();
    let game = one_mountain("wedge", WEDGE, turned);
    near(
        at(&game, 8.0, 10.0),
        4.0,
        "юго-восточный угол штампа ушёл на юго-запад",
    );
    near(
        at(&game, 12.0, 2.0),
        0.0,
        "северо-западный — на северо-восток",
    );
    near(
        at(&game, 12.0, 10.0),
        2.0,
        "северо-восточный — на юго-восток",
    );
    near(at(&game, 14.0, 6.0), 0.0, "вне повёрнутого прямоугольника");
}

#[test]
fn overlapping_mountains_merge_into_the_taller_one_rather_than_adding_up() {
    let game = Setup::default()
        .with_mountains(&list(&[
            mountain("pyramid", [8.0, 6.0], [8.0, 8.0], 4.0),
            mountain("pyramid", [11.0, 6.0], [8.0, 8.0], 3.0),
        ]))
        .game();
    near(at(&game, 9.5, 6.0), 2.5, "наибольшая, а не сумма 4,375");
    near(
        at(&game, 8.0, 6.0),
        4.0,
        "вершина первой выше склона второй",
    );
    near(
        at(&game, 11.0, 6.0),
        3.0,
        "вершина второй выше склона первой",
    );
}

#[test]
fn mountains_stand_on_top_of_the_heights_of_the_file() {
    let game = Setup {
        base: 1.0,
        ..Setup::default()
    }
    .game();
    near(at(&game, 14.0, 6.0), 5.0, "высота файла плюс гора");
    near(at(&game, 1.0, 1.0), 1.0, "вне гор — высота файла");
    let terrain = game.world.terrain();
    assert!(terrain.base_heights().iter().all(|&h| h == 1.0));
}

#[test]
fn a_mountain_beyond_the_edge_lifts_only_the_points_of_the_scene() {
    let game = Setup::default()
        .with_mountains(&list(&[mountain("pyramid", [0.0, 0.0], [8.0, 8.0], 4.0)]))
        .game();
    near(at(&game, 0.0, 0.0), 4.0, "вершина на углу сцены");
    near(at(&game, 4.0, 0.0), 0.0, "край горы на краю сцены");
    let terrain = game.world.terrain();
    assert_eq!(terrain.heights().len(), terrain.base_heights().len());
}

#[test]
fn a_mountain_wholly_beyond_the_scene_changes_nothing_and_stamp_at_does_not_find_it() {
    let game = Setup::default()
        .with_mountains(&list(&[mountain("pyramid", [60.0, 6.0], [8.0, 8.0], 4.0)]))
        .game();
    assert!(game.world.terrain().heights().iter().all(|&h| h == 0.0));
    assert_eq!(game.world.terrain().mountain_at(10.0, 6.0), None);
}

#[test]
fn a_terrain_without_mountains_and_without_slope_keeps_the_heights_of_the_file() {
    let setup = Setup {
        stamps: None,
        base: 0.5,
        covers: Some(r#"[{"material":"grass"}]"#.into()),
        ..Setup::default()
    };
    let (game, _) = setup.load().expect("игра идёт");
    let terrain = game.world.terrain();
    assert_eq!(terrain.heights(), terrain.base_heights());
    assert!(terrain.heights().iter().all(|&h| h == 0.5));
    assert!(terrain.mountains().is_empty());
}

// -------------------------------------------------------------------------------------------
// Кто читает итоговые высоты
// -------------------------------------------------------------------------------------------

const BARREL: &str = r##"{"name":"barrel","position":[9.7,5.7],"size":[0.6,0.6],"shape":"cylinder","height":1,"color":"#886644"}"##;

/// Штамп с плоской вершиной: под бочкой вся земля на одной высоте.
const PLATEAU: &str = r#"{"heights":[[0,0,0,0],[0,1,1,0],[0,1,1,0],[0,0,0,0]]}"#;

#[test]
fn an_object_without_z_stands_on_the_mountain() {
    let game = Setup::default()
        .with_stamp("pyramid", PLATEAU)
        .with_mountains(&list(&[mountain("pyramid", [10.0, 6.0], [12.0, 8.0], 4.0)]))
        .with_objects(BARREL)
        .game();
    near(game.world.base_z(0), 4.0, "бочка на плато горы");
}

#[test]
fn a_steep_mountain_flank_is_shut_to_walking_and_a_gentle_one_is_not() {
    let steep = Setup::default()
        .with_mountains(&list(&[mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 20.0)]))
        .game();
    let blocked = steep.world.terrain().walk_blocked();
    assert!(
        blocked.iter().any(|polygon| {
            polygon
                .iter()
                .all(|p| (6.0..=14.0).contains(&p[0]) && (2.0..=10.0).contains(&p[1]))
        }),
        "крутой склон закрыт"
    );
    let gentle = Setup::default()
        .with_mountains(&list(&[mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 1.0)]))
        .game();
    assert!(gentle.world.terrain().walk_blocked().is_empty());
}

fn peak_camera() -> Camera3d {
    Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW)
}

fn pixel(camera: &Camera3d, point: [f64; 3]) -> [f64; 2] {
    camera.project(point).expect("точка перед камерой")
}

#[test]
fn terrain_at_lands_on_the_top_of_the_mountain() {
    let game = Setup::default()
        .with_mountains(&list(&[mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 4.0)]))
        .game();
    let camera = peak_camera();
    let window = pixel(&camera, [10.0, 6.0, 4.0]);
    let point = terrain_hit(&game.world, &game.scene, &camera, window).expect("луч вниз");
    near(point[2], 4.0, "щелчок ловит вершину горы");
    near(point[0], 10.0, "x вершины");
    near(point[1], 6.0, "y вершины");
}

// -------------------------------------------------------------------------------------------
// stamp_at
// -------------------------------------------------------------------------------------------

fn two_overlapping() -> Game {
    Setup::default()
        .with_mountains(&list(&[
            mountain("pyramid", [8.0, 6.0], [8.0, 8.0], 4.0),
            mountain("pyramid", [12.0, 6.0], [8.0, 8.0], 6.0),
        ]))
        .game()
}

fn stamp_under(game: &Game, point: [f64; 3]) -> Option<usize> {
    let camera = peak_camera();
    stamp_hit(&game.world, &game.scene, &camera, pixel(&camera, point))
}

#[test]
fn stamp_at_finds_the_mountain_under_the_pointer_and_nothing_beside_them() {
    let game = Setup::default().game();
    let (first, second) = (
        stamp_under(&game, [4.0, 6.0, at(&game, 4.0, 6.0)]),
        stamp_under(&game, [14.0, 6.0, at(&game, 14.0, 6.0)]),
    );
    assert_eq!((first, second), (Some(0), Some(1)));
    assert_eq!(stamp_under(&game, [9.0, 1.0, 0.0]), None, "ровное место");
}

#[test]
fn of_two_overlapping_mountains_stamp_at_gives_the_one_that_is_higher_at_the_point() {
    let game = two_overlapping();
    assert_eq!(
        stamp_under(&game, [8.5, 6.0, at(&game, 8.5, 6.0)]),
        Some(0),
        "ближе к вершине первой"
    );
    assert_eq!(
        stamp_under(&game, [11.5, 6.0, at(&game, 11.5, 6.0)]),
        Some(1),
        "ближе к вершине второй"
    );
}

#[test]
fn of_two_equally_high_mountains_stamp_at_gives_the_lower_number() {
    let game = Setup::default()
        .with_mountains(&list(&[
            mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 4.0),
            mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 4.0),
        ]))
        .game();
    assert_eq!(game.world.terrain().mountain_at(9.0, 6.0), Some(0));
}

// -------------------------------------------------------------------------------------------
// set_terrain и terrain_heights
// -------------------------------------------------------------------------------------------

fn flat_heights() -> Vec<f64> {
    vec![0.0; 25 * 41]
}

fn pyramid_json(height: f64) -> Json {
    json!({"stamp": "pyramid", "position": [10, 6], "size": [8, 8], "height": height})
}

#[test]
fn set_terrain_with_mountains_changes_the_effective_heights_and_not_the_heights() {
    let mut game = Setup {
        stamps: None,
        ..Setup::default()
    }
    .game();
    near(at(&game, 10.0, 6.0), 0.0, "гор нет");
    edit::set_terrain(&mut game, &flat_heights(), None, &[pyramid_json(4.0)])
        .expect("рельеф поставлен");
    near(at(&game, 10.0, 6.0), 4.0, "гора поднимает итог");
    let terrain = game.world.terrain();
    assert!(terrain.base_heights().iter().all(|&h| h == 0.0));
    assert_eq!(terrain.mountains().len(), 1);
    edit::set_terrain(&mut game, &flat_heights(), None, &[]).expect("гор нет");
    near(at(&game, 10.0, 6.0), 0.0, "пустой список — гор нет");
}

#[test]
fn set_terrain_puts_the_same_terrain_a_file_would() {
    let from_file = Setup {
        stamps: Some(list(&[mountain("pyramid", [10.0, 6.0], [8.0, 8.0], 4.0)])),
        ..Setup::default()
    }
    .game();
    let mut game = Setup {
        stamps: None,
        ..Setup::default()
    }
    .game();
    edit::set_terrain(&mut game, &flat_heights(), None, &[pyramid_json(4.0)])
        .expect("рельеф поставлен");
    assert_eq!(game.world.terrain(), from_file.world.terrain());
}

#[test]
fn set_terrain_keeps_the_covers_and_the_effective_heights_follow_a_moved_mountain() {
    let mut game = Setup::default().game();
    let covers = game.world.terrain().covers().to_vec();
    let moved = json!({"stamp": "pyramid", "position": [6, 6], "size": [8, 8], "height": 4, "rotation": 15});
    edit::set_terrain(&mut game, &flat_heights(), None, &[moved]).expect("рельеф поставлен");
    assert_eq!(game.world.terrain().covers(), covers);
    near(at(&game, 6.0, 6.0), 4.0, "гора на новом месте");
    near(at(&game, 14.0, 6.0), 0.0, "на старом месте пусто");
}

#[test]
fn an_object_without_z_stands_on_a_mountain_set_by_set_terrain() {
    let mut game = Setup {
        stamps: None,
        ..Setup::default()
    }
    .with_stamp("pyramid", PLATEAU)
    .with_objects(BARREL)
    .game();
    near(game.world.base_z(0), 0.0, "земля ровная");
    let plateau = json!({"stamp": "pyramid", "position": [10, 6], "size": [12, 8], "height": 4});
    edit::set_terrain(&mut game, &flat_heights(), None, &[plateau]).expect("рельеф поставлен");
    near(game.world.base_z(0), 4.0, "бочка встала на гору");
    edit::set_terrain(&mut game, &flat_heights(), None, &[]).expect("гор нет");
    near(game.world.base_z(0), 0.0, "гору убрали — бочка внизу");
}

fn refused_text(game: &mut Game, items: &[Json]) -> String {
    edit::set_terrain(game, &flat_heights(), None, items).expect_err("рельеф должен не встать")
}

#[test]
fn a_mountain_that_fails_the_check_returns_the_text_and_changes_nothing() {
    let mut game = Setup::default().game();
    let before = game.world.terrain().clone();
    let cases: [(Json, &[&str]); 5] = [
        (
            json!({"stamp": "alp", "position": [10, 6], "size": [8, 8], "height": 4}),
            &["stamps[0] → stamp", "alp", "не объявлен"],
        ),
        (
            json!({"stamp": "pyramid", "position": [10], "size": [8, 8], "height": 4}),
            &["stamps[0] → position"],
        ),
        (
            json!({"stamp": "pyramid", "position": [10, 6], "size": [8, 0], "height": 4}),
            &["stamps[0] → size", "больше нуля"],
        ),
        (
            json!({"stamp": "pyramid", "position": [10, 6], "size": [8, 8]}),
            &["stamps[0]", "height"],
        ),
        (
            json!({"stamp": "pyramid", "position": [10, 6], "size": [8, 8], "height": 4, "tilt": 1}),
            &["stamps[0] → tilt", "неизвестное поле"],
        ),
    ];
    for (item, fragments) in cases {
        let text = refused_text(&mut game, std::slice::from_ref(&item));
        assert!(fragments.iter().all(|f| text.contains(f)), "{item}: {text}");
        assert_eq!(
            game.world.terrain(),
            &before,
            "{item}: рельеф не должен меняться"
        );
    }
    let text = refused_text(&mut game, &[Json::Null]);
    assert!(text.contains("stamps[0]"), "{text}");
}

#[test]
fn terrain_heights_gives_both_the_heights_and_the_effective_heights() {
    let game = Setup {
        base: 1.0,
        ..Setup::default()
    }
    .game();
    let TerrainHeights {
        density,
        columns,
        rows,
        heights,
        effective,
        ..
    } = edit::terrain_heights(&game).expect("сцена трёхмерная");
    assert_eq!((density, columns, rows), (2, 41, 25));
    assert_eq!(heights.len(), effective.len());
    assert!(heights.iter().all(|&h| h == 1.0), "без гор");
    let centre = 12 * columns + 28;
    near(effective[centre], 5.0, "гора в точке (14, 6)");
    assert_eq!(effective[0], 1.0);
}

#[test]
fn terrain_heights_without_a_terrain_file_gives_zeros_for_both() {
    let mut game = Setup::default().game();
    edit::set_terrain(&mut game, &flat_heights(), None, &[]).expect("гор нет");
    let snapshot = edit::terrain_heights(&game).expect("сцена трёхмерная");
    assert!(snapshot.heights.iter().all(|&h| h == 0.0));
    assert!(snapshot.effective.iter().all(|&h| h == 0.0));
}

// -------------------------------------------------------------------------------------------
// slope у слоя покрытий
// -------------------------------------------------------------------------------------------

refused!(
    the_first_layer_takes_no_slope,
    Setup::default().with_covers(r#"[{"material":"grass","slope":30}]"#),
    ["covers[0] → slope", "у нижнего слоя"]
);

refused!(
    the_first_layer_takes_no_mask_either,
    Setup::default().with_covers(&format!(r#"[{{"material":"grass","mask":"{MASK}"}}]"#)),
    ["covers[0] → mask", "у нижнего слоя маски нет"]
);

refused!(
    a_layer_above_the_first_without_a_mask_and_without_slope_is_an_error,
    Setup::default().with_covers(r#"[{"material":"grass"},{"material":"earth"}]"#),
    ["covers[1]", "mask", "slope"]
);

refused!(
    a_slope_that_is_not_a_number_is_an_error,
    Setup::default().with_covers(r#"[{"material":"grass"},{"material":"earth","slope":"steep"}]"#),
    ["covers[1] → slope", "ожидалось число"]
);

refused!(
    a_slope_above_ninety_is_an_error,
    Setup::default().with_covers(r#"[{"material":"grass"},{"material":"earth","slope":91}]"#),
    ["covers[1] → slope", "от 0 до 90"]
);

refused!(
    a_negative_slope_is_an_error,
    Setup::default().with_covers(r#"[{"material":"grass"},{"material":"earth","slope":-1}]"#),
    ["covers[1] → slope", "от 0 до 90"]
);

#[test]
fn slope_of_zero_and_of_ninety_are_fine_and_a_layer_may_have_both_a_mask_and_a_slope() {
    for slope in ["0", "90", "37.5"] {
        let covers = format!(r#"[{{"material":"grass"}},{{"material":"earth","slope":{slope}}}]"#);
        Setup::default()
            .with_covers(&covers)
            .load()
            .unwrap_or_else(|f| panic!("slope {slope}: {:?}", f.errors));
    }
    let both =
        format!(r#"[{{"material":"grass"}},{{"material":"earth","slope":34,"mask":"{MASK}"}}]"#);
    let game = Setup::default().with_covers(&both).game();
    assert_eq!(
        game.world.terrain().covers(),
        [
            Cover {
                material: 0,
                mask: None,
                slope: None
            },
            Cover {
                material: 1,
                mask: Some(0),
                slope: Some(34.0)
            },
        ]
    );
}

#[test]
fn a_layer_with_a_slope_and_no_mask_has_no_mask_and_numbers_the_masks_of_the_others() {
    let covers = format!(
        r#"[{{"material":"grass"}},{{"material":"earth","slope":22}},{{"material":"earth","mask":"{MASK}"}}]"#
    );
    let game = Setup::default().with_covers(&covers).game();
    let layers = game.world.terrain().covers();
    assert_eq!(layers[1].mask, None);
    assert_eq!(layers[1].slope, Some(22.0));
    assert_eq!(layers[2].mask, Some(0), "маска — первая из заданных");
    assert_eq!(layers[2].slope, None);
}

#[test]
fn read_texts_asks_for_the_masks_that_are_set_and_not_for_a_layer_without_one() {
    let covers = format!(
        r#"[{{"material":"grass"}},{{"material":"earth","slope":22}},{{"material":"earth","mask":"{MASK}"}}]"#
    );
    let setup = Setup::default().with_covers(&covers);
    let (config, _) = read_entry(&setup.game_json()).expect("game.json");
    let texts = read_texts(&config, None, None, Some(&setup.terrain()));
    assert_eq!(texts.masks, [MASK.to_string()]);
    let only_slope = Setup::default();
    let texts = read_texts(&config, None, None, Some(&only_slope.terrain()));
    assert_eq!(texts.masks, Vec::<String>::new());
}

#[test]
fn the_table_for_the_shader_carries_the_mask_number_and_the_slope_of_every_layer() {
    let covers = format!(
        r#"[{{"material":"grass"}},{{"material":"earth","slope":22}},{{"material":"earth","slope":30,"mask":"{MASK}"}},{{"material":"grass","mask":"{MASK}"}}]"#
    );
    let setup = Setup::default().with_covers(&covers);
    let game = setup.game();
    let (config, _) = read_entry(&setup.game_json()).expect("game.json");
    let mut maps = maps_of("grass");
    maps.extend(maps_of("earth"));
    let masks: Vec<(String, ImageVerdict)> = vec![(MASK.to_string(), ok_map(4))];
    let relief = Relief::new(
        &config.files.materials,
        &maps,
        game.world.terrain().covers(),
        false,
        &[MASK.to_string(), MASK.to_string()],
        &masks,
        [20, 12],
    )
    .expect("карты и маски собираются");
    assert_eq!(relief.table.len(), COVER_TABLE_LEN);
    let rows: Vec<[f32; 2]> = relief.table[1..5]
        .iter()
        .map(|row| [row[2], row[3]])
        .collect();
    assert_eq!(
        rows,
        [[-1.0, -1.0], [-1.0, 22.0], [0.0, 30.0], [1.0, -1.0]],
        "номер маски и slope, −1 — нет"
    );
}

// -------------------------------------------------------------------------------------------
// Рельеф без гор и без slope остаётся прежним
// -------------------------------------------------------------------------------------------

#[test]
fn the_flat_terrain_type_has_no_mountains_and_no_base_heights() {
    let flat = Terrain::flat();
    assert!(flat.mountains().is_empty());
    assert!(flat.base_heights().is_empty());
    assert!(flat.heights().is_empty());
}

//! Фаза 63 — лазание по лестнице: узлы `rung` файла модели, правило `climb`, ключ модели `climb`, подход, подъём,
//! сход и спуск, кисти и ступни на перекладинах и каждая ошибка проверки. Лестницу и человека со скелетом собирает сам
//! тест (`motion_support` — человек; лестница — коробка с узлами `rung`), игра — как в `phase55_motions.rs`.

use engine::core::building::view_rotation;
use engine::core::climb::{self, Ladder};
use engine::core::game::Game;
use engine::core::input::{KeyAction, KeyEvent, StepInput};
use engine::core::property;
use engine::core::report::RuleFired;
use engine::core::skeleton::{Blend, Playing, PoseScratch, Skeleton, Stance};
use engine::core::value::Rotation;
use engine::data::error::GameError;
use engine::data::load::{ModelFile, load_rest_with_motions, read_entry};
use engine::render::models::model_draws;
use serde_json::json;

#[path = "motion_support/mod.rs"]
mod motion_support;
use motion_support::{ARM_LEFT, ARM_RIGHT, Channel, ClipSpec, HIPS, person, person_clips, rig_glb};

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"hero":"flag","ladder":"flag"}}"#;
const CLIMB_RULE: &str = r#"{"kind":"climb","for":{"has":["hero"]},"ladders":{"has":["ladder"]}}"#;
const MOVE_RULE: &str = r#"{"kind":"move","for":{"has":["position","velocity"]}}"#;
const PERSON: &str = r#"{"path":"models/person.glb","idle":"Idle","walk":"Run","climb":"Climb","feet":["foot_l","foot_r"],"hands":["hand_l","hand_r"]}"#;
const SPEED: f64 = 1.2;
const STEPS_PER_SECOND: f64 = 60.0;
/// Шаг перекладин по высоте и отход лестницы по `y` на шаг.
const RUNG_STEP: f64 = 0.3;
const RUNG_LEAN: f64 = 0.03;
/// Где в сцене стоит середина лестницы; перекладины идут от неё к верху сцены (к меньшему `y`).
const LADDER_AT: [f64; 2] = [10.3, 10.6];
const RUNGS: u32 = 20;
/// Верх лестницы — на шаг выше верхней перекладины, и настил лежит на этой высоте.
const DECK_TOP: f64 = RUNG_STEP * (RUNGS + 1) as f64;

/// Девять перекладин в осях сцены, как их ставит автор: от нижней к верхней, верх отходит к меньшему `y`.
fn rungs() -> Vec<[f32; 3]> {
    (0..RUNGS)
        .map(|k| {
            [
                0.0,
                -0.4 - (RUNG_LEAN * f64::from(k)) as f32,
                (RUNG_STEP * f64::from(k + 1)) as f32,
            ]
        })
        .collect()
}

/// Поручни у верха лестницы: по бокам верха, на 1,5 выше настила — по плечо человеку ростом 2,85.
fn grabs() -> Vec<[f32; 3]> {
    let top_y = -0.4 - (RUNG_LEAN * f64::from(RUNGS)) as f32;
    let z = (DECK_TOP + 1.5) as f32;
    vec![[-0.4, top_y, z], [0.4, top_y, z]]
}

/// `.glb` лестницы: коробка 0,6 × 1,2 × 6,4, пустые узлы `rung_N` в `points` и `grab_N` в `grab_points` (оси сцены: x
/// вправо, y к игроку, z вверх).
fn ladder_glb(points: &[[f32; 3]], grab_points: &[[f32; 3]]) -> Vec<u8> {
    let (low, high) = ([-0.3f32, 0.0, -1.0], [0.3f32, 6.4, 0.2]);
    let corners: Vec<[f32; 3]> = (0..8)
        .map(|bits: usize| {
            [
                if bits & 1 == 0 { low[0] } else { high[0] },
                if bits & 2 == 0 { low[1] } else { high[1] },
                if bits & 4 == 0 { low[2] } else { high[2] },
            ]
        })
        .collect();
    let indices: [u16; 36] = [
        0, 4, 6, 0, 6, 2, 1, 3, 7, 1, 7, 5, 0, 1, 5, 0, 5, 4, 2, 6, 7, 2, 7, 3, 0, 2, 3, 0, 3, 1,
        4, 5, 7, 4, 7, 6,
    ];
    let mut bin: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let positions = bin.len();
    bin.extend(indices.iter().flat_map(|index| index.to_le_bytes()));
    let mut nodes = vec![json!({"name": "frame", "mesh": 0})];
    for (at, point) in points.iter().enumerate() {
        nodes.push(
            json!({"name": format!("rung_{at:02}"), "translation": [point[0], point[2], point[1]]}),
        );
    }
    for (at, point) in grab_points.iter().enumerate() {
        nodes.push(
            json!({"name": format!("grab_{at}"), "translation": [point[0], point[2], point[1]]}),
        );
    }
    let document = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": positions},
            {"buffer": 0, "byteOffset": positions, "byteLength": bin.len() - positions}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3", "min": low, "max": high},
            {"bufferView": 1, "componentType": 5123, "count": 36, "type": "SCALAR"}
        ]
    });
    let mut text = document.to_string().into_bytes();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((text.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(text);
    file.extend((bin.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(bin);
    file
}

/// Движения человека и «Climb»: таз ниже, руки вытянуты вперёд на высоте плеч — так кисти и ступни достают до перекладин.
fn clips() -> Vec<ClipSpec> {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let forward = [-half, 0.0, 0.0, half];
    let mut all = person_clips();
    all.push(ClipSpec {
        name: "Climb",
        channels: vec![
            Channel {
                node: HIPS,
                path: "translation",
                times: vec![0.0, 1.0],
                values: vec![vec![0.0, 0.7, 0.0], vec![0.0, 0.7, 0.0]],
            },
            Channel {
                node: ARM_LEFT,
                path: "rotation",
                times: vec![0.0, 1.0],
                values: vec![forward.to_vec(), forward.to_vec()],
            },
            Channel {
                node: ARM_RIGHT,
                path: "rotation",
                times: vec![0.0, 1.0],
                values: vec![forward.to_vec(), forward.to_vec()],
            },
        ],
    });
    all
}

struct Setup {
    rungs: Vec<[f32; 3]>,
    grabs: Vec<[f32; 3]>,
    person: String,
    hero_extra: String,
    rules: String,
    scene_extra: String,
    flat: bool,
    code: Option<String>,
}

impl Setup {
    fn new() -> Setup {
        Setup {
            rungs: rungs(),
            grabs: grabs(),
            person: PERSON.to_string(),
            hero_extra: format!(r#","climb_speed":{SPEED},"velocity":[0,0]"#),
            rules: format!(r#"{{"rules":[{CLIMB_RULE},{MOVE_RULE}]}}"#),
            scene_extra: String::new(),
            flat: false,
            code: None,
        }
    }

    /// Игра с кодом: `act(obj)` вызывает правило `check` для героя до правила `climb`.
    fn with_code(body: &str) -> Setup {
        let act = r#"{"kind":"check","for":{"has":["hero"]},"do":[["run","act"]]}"#;
        Setup {
            rules: format!(r#"{{"rules":[{act},{CLIMB_RULE},{MOVE_RULE}]}}"#),
            code: Some(format!(
                "function act(obj)
{body}
end
"
            )),
            ..Setup::new()
        }
    }

    fn load(&self) -> Result<(Game, Vec<GameError>), Vec<String>> {
        let camera = if self.flat {
            ""
        } else {
            r#","view_height":16,"camera":{"pitch":55}"#
        };
        let models = if self.flat {
            String::new()
        } else {
            format!(
                r#","models":{{"person":{},"ladder":{{"path":"models/ladder.glb"}}}}"#,
                self.person
            )
        };
        let code = if self.code.is_some() {
            r#","code":"code.lua""#
        } else {
            ""
        };
        let text = format!(
            r##"{{"name":"L","scene":{{"width":40,"height":30,"background":"#000000"{camera}}},
"random_seed":1,"max_objects":64,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{code}{models}}}}}"##
        );
        let scene = format!(
            r##"{{"objects":[
{{"name":"hero","model":"person","hero":true,"position":[10.0,11.3],"size":[0.6,0.6],"height":2.85{}}},
{{"name":"ladder","model":"ladder","ladder":true,"position":[10.0,10.0],"size":[0.6,1.2]}},
{{"name":"deck","deck":true,"shape":"box","color":"#888888","position":[9.0,8.0],"size":[3.0,1.9],"height":{DECK_TOP}}}{}]}}"##,
            self.hero_extra, self.scene_extra
        );
        let (config, entry_warnings) = read_entry(&text).map_err(|f| describe(&f.errors))?;
        let files = vec![
            (
                "person".to_string(),
                ModelFile {
                    bytes: Some(rig_glb(&person(), &clips())),
                    images: Vec::new(),
                },
            ),
            (
                "ladder".to_string(),
                ModelFile {
                    bytes: Some(ladder_glb(&self.rungs, &self.grabs)),
                    images: Vec::new(),
                },
            ),
        ];
        load_rest_with_motions(
            &text,
            config,
            Some(PROPS),
            Some(&scene),
            Some(&self.rules),
            Some(SCREENS),
            &[],
            &[],
            &[],
            &[],
            self.code.as_deref(),
            false,
            &[],
            None,
            &[],
            &[],
            &[],
            &files,
            None,
            &[],
            None,
        )
        .map(|(game, _, warnings, _)| {
            let mut all = entry_warnings;
            all.extend(warnings);
            (game, all)
        })
        .map_err(|failure| describe(&failure.errors))
    }

    fn game(&self) -> Game {
        match self.load() {
            Ok((game, warnings)) => {
                assert!(warnings.is_empty(), "предупреждения: {warnings:#?}");
                game
            }
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

fn expect_error(setup: &Setup, fragments: &[&str]) {
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

fn named(game: &Game, name: &str) -> u32 {
    game.world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some(name))
        .unwrap_or_else(|| panic!("объекта \"{name}\" нет на сцене"))
}

/// Середина основания героя и высота основания.
fn spot(game: &Game) -> [f64; 3] {
    let hero = named(game, "hero");
    let (position, size) = (
        game.world.vec2(hero, property::POSITION).expect("position"),
        game.world.vec2(hero, property::SIZE).expect("size"),
    );
    [
        position[0] + size[0] / 2.0,
        position[1] + size[1] / 2.0,
        game.world.base_z(hero),
    ]
}

fn climbing(game: &Game) -> bool {
    game.world.flag(named(game, "hero"), property::CLIMBING)
}

fn ask(game: &mut Game) {
    let hero = named(game, "hero");
    game.world.set_flag(hero, property::CLIMB, true);
}

/// Шаги, пока герой лезет, но не больше `limit`; после каждого шага — `each`. Число сделанных шагов.
fn climb_on(game: &mut Game, limit: usize, mut each: impl FnMut(&Game)) -> usize {
    for done in 1..=limit {
        game.step(StepInput::empty());
        each(game);
        if !climbing(game) {
            return done;
        }
    }
    limit
}

fn ladder_of(game: &Game) -> Ladder {
    Ladder::of(&game.world, named(game, "ladder")).expect("у лестницы есть перекладины")
}

/// Места у концов лестницы, как их задаёт документ: низ — в 0,4 от нижнего конца к лезущему (к большему `y`), верх — в
/// 0,5 от верхнего конца от лезущего, на настиле.
fn places() -> ([f64; 3], [f64; 3]) {
    let low_end_y = LADDER_AT[1] + RUNG_LEAN;
    let high_end_y = LADDER_AT[1] - RUNG_LEAN * f64::from(RUNGS);
    (
        [LADDER_AT[0], low_end_y + 0.4, 0.0],
        [LADDER_AT[0], high_end_y - 0.5, DECK_TOP],
    )
}

fn close(a: [f64; 3], b: [f64; 3], within: f64) -> bool {
    (0..3).all(|axis| (a[axis] - b[axis]).abs() <= within)
}

fn put_hero(game: &mut Game, middle: [f64; 2], z: f64) {
    let hero = named(game, "hero");
    game.world
        .set_position_exact(hero, [middle[0] - 0.3, middle[1] - 0.3], z);
}

// -------------------------------------------------------------------------------------------
// Узлы rung и ошибки проверки
// -------------------------------------------------------------------------------------------

#[test]
fn the_rung_nodes_of_the_file_become_the_rungs_of_the_model_from_the_bottom_up() {
    let game = Setup::new().game();
    let models = game.world.models();
    let model = models
        .get(models.index_of("ladder").expect("модель объявлена"))
        .expect("модель есть");
    let rungs = &model.building.rungs;
    assert_eq!(rungs.len(), RUNGS as usize);
    assert!(rungs.windows(2).all(|pair| pair[0][2] < pair[1][2]));
    // Середина коробки по `y` лежит в начале осей: узлы сдвинуты вместе с моделью.
    let top = rungs.len() - 1;
    let lean = RUNG_LEAN as f32 * top as f32;
    assert!((rungs[0][1] - 0.0).abs() < 1e-5 && (rungs[top][1] + lean).abs() < 1e-5);
    assert!((rungs[0][2] - 0.3).abs() < 1e-5 && (rungs[top][2] - 6.0).abs() < 1e-5);

    let mut reversed = Setup::new();
    reversed.rungs.reverse();
    let back = reversed.game();
    let models = back.world.models();
    let again = models
        .get(models.index_of("ladder").expect("модель объявлена"))
        .expect("модель есть");
    assert_eq!(
        &again.building.rungs, rungs,
        "порядок узлов в файле не важен"
    );
}

#[test]
fn one_rung_or_a_ladder_that_leans_nowhere_stops_the_start() {
    let mut one = Setup::new();
    one.rungs.truncate(1);
    expect_error(&one, &["ladder", "один узел"]);

    let mut upright = Setup::new();
    for rung in &mut upright.rungs {
        rung[1] = -0.4;
    }
    expect_error(&upright, &["ladder", "нет наклона"]);
}

#[test]
fn a_model_without_rungs_is_no_ladder_and_a_model_of_one_object_is_not_asked_for_them() {
    let mut bare = Setup::new();
    bare.rungs.clear();
    bare.grabs.clear();
    let mut game = bare.game();
    put_hero(&mut game, [10.3, 11.6], 0.0);
    ask(&mut game);
    game.step(StepInput::empty());
    assert!(!climbing(&game), "у модели без перекладин лезть не по чему");
}

#[test]
fn every_mistake_of_the_climb_rule_and_its_properties_stops_the_start() {
    let mut flat = Setup::new();
    flat.flat = true;
    expect_error(&flat, &["rules[0]", "climb есть только в трёхмерной сцене"]);

    let mut no_ladders = Setup::new();
    no_ladders.rules = r#"{"rules":[{"kind":"climb","for":{"has":["hero"]}}]}"#.to_string();
    expect_error(&no_ladders, &["rules[0]", "ladders"]);

    let mut no_speed = Setup::new();
    no_speed.hero_extra = r#","velocity":[0,0]"#.to_string();
    expect_error(&no_speed, &["не хватает свойства \"climb_speed\""]);

    for speed in ["0", "-1"] {
        let mut slow = Setup::new();
        slow.hero_extra = format!(r#","climb_speed":{speed},"velocity":[0,0]"#);
        expect_error(&slow, &["climb_speed", "больше нуля"]);
    }

    let mut no_hands = Setup::new();
    no_hands.person = PERSON.replace(r#","hands":["hand_l","hand_r"]"#, "");
    expect_error(&no_hands, &["под climb", "нет hands"]);

    let mut no_feet = Setup::new();
    no_feet.person = PERSON.replace(r#","feet":["foot_l","foot_r"]"#, "");
    expect_error(&no_feet, &["под climb", "нет feet"]);

    let mut no_motion = Setup::new();
    no_motion.person = PERSON.replace(r#""climb":"Climb""#, r#""climb":"Nowhere""#);
    expect_error(&no_motion, &["climb", "Nowhere"]);
}

#[test]
fn climbing_is_written_by_the_engine_alone() {
    let mut in_scene = Setup::new();
    in_scene.hero_extra = format!(r#","climb_speed":{SPEED},"velocity":[0,0],"climbing":true"#);
    expect_error(&in_scene, &["признак \"climbing\" пишет только движок"]);

    let mut in_keys = Setup::new();
    in_keys.hero_extra = format!(
        r#","climb_speed":{SPEED},"velocity":[0,0],"keys":{{"KeyC":{{"press":[["climbing",true]]}}}}"#
    );
    expect_error(&in_keys, &["признак \"climbing\" пишет только движок"]);

    for action in [
        r#"["set","climbing",true]"#,
        r#"["give","climbing",{"has":["hero"]}]"#,
        r#"["take","climbing"]"#,
    ] {
        let mut in_rules = Setup::new();
        in_rules.rules = format!(
            r#"{{"rules":[{CLIMB_RULE},{{"kind":"check","for":{{"has":["hero"]}},"do":[{action}]}}]}}"#
        );
        expect_error(&in_rules, &["признак \"climbing\" пишет только движок"]);
    }
}

// -------------------------------------------------------------------------------------------
// Подъём и спуск
// -------------------------------------------------------------------------------------------

#[test]
fn a_request_at_the_foot_of_the_ladder_starts_the_climb_and_the_engine_takes_the_request_back() {
    let mut game = Setup::new().game();
    let hero = named(&game, "hero");
    ask(&mut game);
    game.step(StepInput::empty());
    assert!(climbing(&game));
    assert!(!game.world.flag(hero, property::CLIMB), "просьба снята");
    assert!(game.world.climb_state(hero).is_climbing());
}

#[test]
fn of_two_ladders_in_reach_the_climber_takes_the_nearer_one() {
    let mut setup = Setup::new();
    setup.scene_extra =
        r#",{"name":"second","model":"ladder","ladder":true,"position":[10.8,10.0],"size":[0.6,1.2]}"#
            .to_string();
    for (start, expected_x) in [
        ([10.3, 11.6], LADDER_AT[0]),
        ([11.1, 11.6], LADDER_AT[0] + 0.8),
    ] {
        let mut game = setup.game();
        put_hero(&mut game, start, 0.0);
        ask(&mut game);
        climb_on(&mut game, 600, |_| {});
        let (_, high) = places();
        assert!(
            close(spot(&game), [expected_x, high[1], DECK_TOP], 1e-4),
            "от {start:?}: {:?}",
            spot(&game)
        );
    }
}

#[test]
fn a_request_far_from_both_ends_or_a_step_too_high_or_low_does_nothing_but_is_taken_back() {
    for (middle, z) in [([10.3, 14.0], 0.0), ([10.3, 11.6], 1.0), ([10.3, 9.0], 0.0)] {
        let mut game = Setup::new().game();
        put_hero(&mut game, middle, z);
        let before = spot(&game);
        ask(&mut game);
        game.step(StepInput::empty());
        assert!(!climbing(&game), "{middle:?} на {z}");
        assert!(!game.world.flag(named(&game, "hero"), property::CLIMB));
        assert_eq!(spot(&game), before);
    }
}

#[test]
fn the_climber_turns_to_the_ladder_smoothly_the_short_way_round_while_he_walks_to_the_foot() {
    let mut game = Setup::new().game();
    let hero = named(&game, "hero");
    let facing = |game: &Game| {
        game.world
            .rotation(hero, property::FACING)
            .map_or(0.0, Rotation::angle)
    };
    game.world.set_rotation(
        hero,
        property::FACING,
        Rotation::from_degrees(350.0).expect("угол"),
    );
    ask(&mut game);
    let mut turned = Vec::new();
    for _ in 0..20 {
        game.step(StepInput::empty());
        turned.push(facing(&game));
    }
    assert!(
        turned
            .iter()
            .all(|&angle| (180.0 - 1e-6..=350.0).contains(&angle)),
        "с 350 к 180 поворот идёт через 270, а не через 0: {turned:?}"
    );
    let midway = turned[8];
    assert!(
        midway > 190.0 && midway < 340.0,
        "посреди подхода лицо между началом и концом поворота: {midway}"
    );
    assert!(
        (turned[19] - 180.0).abs() < 1e-6,
        "к концу подхода лицом к лестнице: {}",
        turned[19]
    );
}

#[test]
fn the_climber_turns_to_the_ladder_walks_to_the_foot_and_rises_at_the_speed_of_climb_speed() {
    let mut game = Setup::new().game();
    ask(&mut game);
    let (_, high) = places();
    let mut heights = Vec::new();
    let steps = climb_on(&mut game, 600, |game| heights.push(spot(game)[2]));
    let seconds = steps as f64 / STEPS_PER_SECOND;
    let expected = 0.3 + DECK_TOP / SPEED;
    assert!(
        (seconds - expected).abs() < 0.05,
        "лез {seconds} с, ждали {expected}"
    );
    assert!(!climbing(&game));
    assert!(
        close(spot(&game), high, 1e-4),
        "на месте наверху: {:?}, ждали {high:?}",
        spot(&game)
    );
    let hero = named(&game, "hero");
    let facing = view_rotation(&game.world, hero).angle().rem_euclid(360.0);
    assert!((facing - 180.0).abs() < 1e-6, "лицом к лестнице: {facing}");
    let rising: Vec<f64> = heights.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let steady = rising
        .iter()
        .filter(|&&rise| (rise - SPEED / 60.0).abs() < 1e-9)
        .count();
    assert!(
        steady as f64 / rising.len() as f64 > 0.7,
        "по лестнице тело идёт со скоростью climb_speed: {steady} из {}",
        rising.len()
    );
    assert!(
        rising
            .iter()
            .all(|&rise| (0.0..=SPEED / 60.0 + 1e-9).contains(&rise))
    );
}

#[test]
fn from_the_place_on_the_deck_a_request_leads_back_down_to_the_place_at_the_foot() {
    let mut game = Setup::new().game();
    ask(&mut game);
    climb_on(&mut game, 600, |_| {});
    ask(&mut game);
    let mut heights = Vec::new();
    let steps = climb_on(&mut game, 600, |game| heights.push(spot(game)[2]));
    assert!(!climbing(&game));
    let (low, _) = places();
    assert!(close(spot(&game), low, 1e-4), "{:?} ≠ {low:?}", spot(&game));
    let seconds = steps as f64 / STEPS_PER_SECOND;
    assert!(
        (seconds - (0.3 + DECK_TOP / SPEED)).abs() < 0.05,
        "спуск шёл {seconds} с"
    );
    assert!(
        heights.windows(2).all(|pair| pair[1] <= pair[0] + 1e-12),
        "спуск не поднимается"
    );
}

#[test]
fn a_request_written_while_climbing_is_taken_back_and_the_climb_goes_on() {
    let mut game = Setup::new().game();
    ask(&mut game);
    game.step(StepInput::empty());
    for _ in 0..30 {
        ask(&mut game);
        game.step(StepInput::empty());
        assert!(climbing(&game));
        assert!(!game.world.flag(named(&game, "hero"), property::CLIMB));
    }
    assert!(spot(&game)[2] >= 0.0);
    let steps = climb_on(&mut game, 600, |_| {});
    assert!(steps < 600 && !climbing(&game));
    assert!(close(spot(&game), places().1, 1e-4));
}

#[test]
fn move_and_walk_do_not_move_the_climber_and_velocity_and_walk_to_stay() {
    let mut setup = Setup::new();
    setup.hero_extra =
        format!(r#","climb_speed":{SPEED},"velocity":[3,0],"walk_to":[20,11],"walk_speed":4"#);
    setup.rules = format!(
        r#"{{"rules":[{CLIMB_RULE},{MOVE_RULE},{{"kind":"walk","for":{{"has":["hero"]}}}}]}}"#
    );
    let mut game = setup.game();
    put_hero(&mut game, [10.3, 11.6], 0.0);
    ask(&mut game);
    climb_on(&mut game, 600, |game| {
        let hero = named(game, "hero");
        assert_eq!(game.world.vec2(hero, property::VELOCITY), Some([3.0, 0.0]));
        assert_eq!(game.world.vec2(hero, property::WALK_TO), Some([20.0, 11.0]));
        assert!(
            (spot(game)[0] - LADDER_AT[0]).abs() < 0.35,
            "сошёл вбок: {:?}",
            spot(game)
        );
    });
    let x = spot(&game)[0];
    for _ in 0..30 {
        game.step(StepInput::empty());
    }
    assert!(spot(&game)[0] > x + 0.5, "сошёл с лестницы — снова бежит");
}

#[test]
fn a_ladder_removed_under_the_climber_leaves_him_where_he_was_and_ends_the_climb() {
    let mut game = Setup::new().game();
    ask(&mut game);
    for _ in 0..60 {
        game.step(StepInput::empty());
    }
    assert!(climbing(&game));
    let before = spot(&game);
    let ladder = named(&game, "ladder");
    game.world.delete(ladder);
    game.step(StepInput::empty());
    assert!(!climbing(&game));
    assert!(!game.world.climb_state(named(&game, "hero")).is_climbing());
    assert_eq!(spot(&game), before);
}

#[test]
fn code_asks_for_the_climb_with_the_flag_and_cannot_write_climbing_or_a_speed_of_zero() {
    let mut game = Setup::with_code("obj.climb = true").game();
    game.step(StepInput::empty());
    assert!(climbing(&game), "просьба кода начала лазание в том же шаге");

    for (body, message) in [
        (
            "obj.climbing = true",
            "признак \"climbing\" пишет только движок",
        ),
        (
            "obj.climbing = nil",
            "признак \"climbing\" пишет только движок",
        ),
        (
            "obj.climb_speed = 0",
            "climb_speed\" должно быть больше нуля",
        ),
    ] {
        let mut game = Setup::with_code(body).game();
        game.step(StepInput::empty());
        let error = game
            .code_error()
            .map(|e| e.message.clone())
            .unwrap_or_default();
        assert!(error.contains(message), "{body}: {error:?}");
    }
}

#[test]
fn a_session_reports_the_climb_rule_for_the_steps_that_moved_the_climber() {
    let mut game = Setup::new().game();
    game.begin_session();
    let hero = named(&game, "hero");
    ask(&mut game);
    game.step(StepInput::empty());
    let reported = |game: &Game| {
        game.last_report()
            .expect("сессия собирает отчёт")
            .fired
            .iter()
            .any(|fired| matches!(fired, RuleFired::Climb { rule, objects } if rule == "rules[0]" && objects == &[hero]))
    };
    assert!(!reported(&game), "шаг начала лазания никого не сдвинул");
    game.step(StepInput::empty());
    assert!(reported(&game));
}

#[test]
fn the_key_of_the_hero_asks_for_the_climb_like_any_flag() {
    let mut setup = Setup::new();
    setup.hero_extra = format!(
        r#","climb_speed":{SPEED},"velocity":[0,0],"keys":{{"KeyE":{{"press":[["climb",true]],"release":[["climb",false]]}}}}"#
    );
    let mut game = setup.game();
    let key = |action| StepInput {
        events: vec![KeyEvent {
            code: "KeyE".to_string(),
            action,
            target: None,
        }],
        cursor: None,
    };
    game.step(key(KeyAction::Press));
    assert!(climbing(&game));
    game.step(key(KeyAction::Release));
    assert!(climbing(&game));
}

// -------------------------------------------------------------------------------------------
// Движение тела и позы кистей и ступней
// -------------------------------------------------------------------------------------------

fn skeleton_of(game: &Game) -> Skeleton {
    let models = game.world.models();
    models
        .get(models.index_of("person").expect("модель объявлена"))
        .and_then(|model| model.skeleton.clone())
        .expect("у модели есть скелет")
}

#[test]
fn on_the_ladder_the_body_plays_the_climb_motion_and_without_one_the_idle_and_animation_wins() {
    let mut game = Setup::new().game();
    let hero = named(&game, "hero");
    let skeleton = skeleton_of(&game);
    let climb = skeleton.clip_named("Climb").expect("движение есть");
    ask(&mut game);
    for _ in 0..20 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world
            .motion(hero)
            .blend(&skeleton)
            .current()
            .playing
            .clip,
        Some(climb)
    );
    let run = skeleton.clip_named("Run").expect("движение есть");
    game.world
        .set_text(hero, property::ANIMATION, "Run".to_string());
    game.step(StepInput::empty());
    assert_eq!(
        game.world
            .motion(hero)
            .blend(&skeleton)
            .current()
            .playing
            .clip,
        Some(run),
        "названное в animation главнее"
    );

    let mut plain = Setup::new();
    plain.person = PERSON.replace(r#","climb":"Climb""#, "");
    let mut game = plain.game();
    let hero = named(&game, "hero");
    let skeleton = skeleton_of(&game);
    ask(&mut game);
    for _ in 0..20 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world
            .motion(hero)
            .blend(&skeleton)
            .current()
            .playing
            .clip,
        skeleton.idle,
        "без ключа climb тело стоит в позе idle"
    );
}

/// Точка кости в осях сцены в позе `scratch` героя `stance`.
fn world_point(
    skeleton: &Skeleton,
    scratch: &PoseScratch,
    stance: &Stance,
    bone: &str,
) -> [f64; 3] {
    let bone = skeleton.bone_named(bone).expect("кость есть");
    let matrix = scratch.globals()[bone];
    let (x, y) = (
        f64::from(matrix[3][0]) * stance.scale,
        f64::from(matrix[3][1]) * stance.scale,
    );
    [
        stance.place[0] + stance.cos * x - stance.sin * y,
        stance.place[1] + stance.sin * x + stance.cos * y,
        stance.base + f64::from(matrix[3][2]) * stance.scale,
    ]
}

/// Поза героя `game` с кистями и ступнями на перекладинах; `None`, пока хвата нет.
fn gripped_pose(game: &Game) -> Option<(PoseScratch, Stance, f64)> {
    let hero = named(game, "hero");
    let grip = climb::grip(&game.world, hero)?;
    let skeleton = skeleton_of(game);
    let mut scratch = PoseScratch::default();
    skeleton.pose(
        &Blend::single(Playing {
            clip: skeleton.clip_named("Climb"),
            time: 0.0,
            looped: true,
        }),
        &mut scratch,
    );
    let (sin, cos) = view_rotation(&game.world, hero).sin_cos();
    let middle = spot(game);
    let stance = Stance {
        place: [middle[0], middle[1]],
        cos,
        sin,
        scale: 1.5,
        base: middle[2],
    };
    skeleton.grip_ladder(&stance, &grip, &mut scratch);
    Some((scratch, stance, grip.weight))
}

/// На каждом шаге подъёма и спуска с полным хватом три из четырёх кистей и ступней держатся: на перекладине или на верху
/// лестницы — со сдвигом вбок, ступня — подошвой сверху и подушечкой на середине, кисть у верха — на поручне, на его
/// высоте не выше узла `grab` и не ниже настила.
#[test]
fn on_every_moment_of_the_climb_three_of_four_hands_and_feet_hold_on_and_the_fourth_is_between() {
    for down in [false, true] {
        let mut game = Setup::new().game();
        ask(&mut game);
        climb_on(&mut game, 600, |_| {});
        if down {
            ask(&mut game);
        } else {
            let (low, _) = places();
            put_hero(&mut game, [low[0], low[1]], 0.0);
            ask(&mut game);
        }
        let skeleton = skeleton_of(&game);
        let ladder = ladder_of(&game);
        let mut bars: Vec<[f64; 3]> = ladder.rungs().to_vec();
        let (last, before) = (bars[bars.len() - 1], bars[bars.len() - 2]);
        bars.push(std::array::from_fn(|axis| 2.0 * last[axis] - before[axis]));
        let back = ladder.back();
        let grabs = ladder.grabs().to_vec();
        assert_eq!(grabs.len(), 2, "у лестницы два поручня");
        let mut steps = 0;
        climb_on(&mut game, 600, |game| {
            let Some((scratch, stance, weight)) = gripped_pose(game) else {
                return;
            };
            if weight < 1.0 {
                return;
            }
            let limbs = [
                ("hand_r", 0.225),
                ("foot_l", -0.15),
                ("hand_l", -0.225),
                ("foot_r", 0.15),
            ];
            let mut held = 0;
            let mut where_they_are = Vec::new();
            for (bone, across) in limbs {
                let point = world_point(&skeleton, &scratch, &stance, bone);
                let foot = bone.starts_with("foot");
                let on_bar = bars.iter().any(|bar| {
                    let set = if foot { 0.06 } else { 0.0 };
                    let lift = point[2] - bar[2];
                    let lifted = if foot {
                        (0.015..=0.25).contains(&lift)
                    } else {
                        lift.abs() <= 0.02
                    };
                    (point[1] - (bar[1] + back[1] * set)).abs() <= 0.02
                        && lifted
                        && (point[0] - (bar[0] - across + back[0] * set)).abs() <= 0.02
                });
                let on_grab = grabs.iter().any(|grab| {
                    (point[0] - grab[0]).abs() <= 0.02
                        && (point[1] - grab[1]).abs() <= 0.02
                        && (DECK_TOP - 0.02..=grab[2] + 0.02).contains(&point[2])
                });
                where_they_are.push(format!("{bone}: {point:.3?}"));
                held += usize::from(on_bar || on_grab);
            }
            assert!(
                held >= 3,
                "держатся {held} из четырёх, тело на {:.3?}: {where_they_are:#?}, перекладины {bars:.3?}, поручни {grabs:.3?}",
                spot(game)
            );
            steps += 1;
        });
        assert!(steps > 100, "проверено {steps} шагов, спуск: {down}");
    }
}

#[test]
fn the_renderer_poses_the_climber_with_the_hands_and_feet_on_the_rungs_and_the_idler_without() {
    let mut game = Setup::new().game();
    let hero = named(&game, "hero");
    let at = game
        .world
        .models()
        .index_of("person")
        .expect("модель объявлена");
    let idle = model_draws(&game).bones[at].clone();
    ask(&mut game);
    for _ in 0..90 {
        game.step(StepInput::empty());
    }
    let (scratch, _, weight) = gripped_pose(&game).expect("лезет и держится");
    assert_eq!(weight, 1.0);
    let skeleton = skeleton_of(&game);
    let mut expected = Vec::new();
    skeleton.skin_matrices(&scratch, &mut expected);
    let expected: Vec<f32> = expected.iter().flatten().flatten().copied().collect();
    let drawn = model_draws(&game).bones[at].clone();
    assert_eq!(drawn.len(), expected.len());
    let worst = drawn
        .iter()
        .zip(&expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(
        worst < 1e-3,
        "кадр расходится с позой на перекладинах на {worst}"
    );
    assert!(
        idle.iter().zip(&drawn).any(|(a, b)| (a - b).abs() > 0.05),
        "поза лезущего не похожа на стойку"
    );
    assert!(game.world.flag(hero, property::CLIMBING));
}

/// Пока объект лезет, кадр — его движение с кистями и ступнями на перекладинах и без поиска пола под ступнями, при любом
/// хвате: на краю настила пол под ступнёй то наверху, то на земле, и таз прыгал бы.
#[test]
fn while_climbing_the_frame_never_seeks_the_floor_under_the_feet() {
    let mut game = Setup::new().game();
    let at = game
        .world
        .models()
        .index_of("person")
        .expect("модель объявлена");
    let skeleton = skeleton_of(&game);
    let mut compared = 0;
    let mut partial = 0;
    let mut released = 0;
    for _ in 0..2 {
        ask(&mut game);
        climb_on(&mut game, 600, |game| {
            let hero = named(game, "hero");
            let Some(grip) = climb::grip(&game.world, hero) else {
                return;
            };
            let mut scratch = PoseScratch::default();
            skeleton.pose(&game.world.motion(hero).blend(&skeleton), &mut scratch);
            let (sin, cos) = view_rotation(&game.world, hero).sin_cos();
            let middle = spot(game);
            let stance = Stance {
                place: [middle[0], middle[1]],
                cos,
                sin,
                scale: 1.5,
                base: middle[2],
            };
            skeleton.grip_ladder(&stance, &grip, &mut scratch);
            let mut expected = Vec::new();
            skeleton.skin_matrices(&scratch, &mut expected);
            let drawn = model_draws(game).bones[at].clone();
            let worst = drawn
                .iter()
                .zip(expected.iter().flatten().flatten())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(
                worst < 1e-3,
                "хват {}: кадр расходится на {worst}",
                grip.weight
            );
            compared += 1;
            partial += usize::from(grip.weight < 1.0);
            if grip.weight == 0.0 && grip.hands_weight == 0.0 {
                let mut free = PoseScratch::default();
                skeleton.pose(&game.world.motion(hero).blend(&skeleton), &mut free);
                let mut plain = Vec::new();
                skeleton.skin_matrices(&free, &mut plain);
                let seam = drawn
                    .iter()
                    .zip(plain.iter().flatten().flatten())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f32, f32::max);
                assert!(
                    seam < 1e-3,
                    "без хвата поза расходится с движением на {seam}"
                );
                released += 1;
            }
        });
    }
    assert!(
        compared > 200 && partial > 20 && released >= 4,
        "сравнено {compared}, с неполным хватом {partial}, без хвата {released}"
    );
}

/// Наверху кисти отпускают лестницу за первую половину схода, а при посадке сверху берутся за неё за последнюю: пока
/// ступни ещё держатся, кисти уже свободны.
#[test]
fn at_the_top_the_hands_let_go_before_the_feet_and_take_hold_after_them() {
    let mut game = Setup::new().game();
    for _ in 0..2 {
        ask(&mut game);
        let mut hands_free_feet_held = 0;
        climb_on(&mut game, 600, |game| {
            let Some(grip) = climb::grip(&game.world, named(game, "hero")) else {
                return;
            };
            assert!(
                grip.hands_weight <= grip.weight + 1e-12,
                "кисти держат крепче ступней: {} > {}",
                grip.hands_weight,
                grip.weight
            );
            let at_top = spot(game)[2] > DECK_TOP - 2.0 * RUNG_STEP;
            if at_top && grip.hands_weight == 0.0 && grip.weight > 0.1 {
                hands_free_feet_held += 1;
            }
        });
        assert!(
            hands_free_feet_held > 3,
            "кисти свободны при хвате ступней на {hands_free_feet_held} шагах"
        );
    }
}

#[test]
fn the_same_body_height_up_and_down_gives_the_same_grip() {
    let mut game = Setup::new().game();
    ask(&mut game);
    let mut up = Vec::new();
    climb_on(&mut game, 600, |game| {
        if let Some(grip) = climb::grip(&game.world, named(game, "hero")) {
            up.push((
                grip.height,
                grip.weight,
                grip.hands([0.5, 0.5], [[0.0; 2]; 2]),
                grip.feet(),
            ));
        }
    });
    ask(&mut game);
    let mut down = Vec::new();
    climb_on(&mut game, 600, |game| {
        if let Some(grip) = climb::grip(&game.world, named(game, "hero")) {
            down.push((
                grip.height,
                grip.weight,
                grip.hands([0.5, 0.5], [[0.0; 2]; 2]),
                grip.feet(),
            ));
        }
    });
    let mut compared = 0;
    for (height, weight, hands, feet) in &up {
        let Some(other) = down
            .iter()
            .find(|other| (other.0 - height).abs() < 0.002 && other.1 == 1.0 && *weight == 1.0)
        else {
            continue;
        };
        let near =
            |a: &[[f64; 3]], b: &[[f64; 3]]| a.iter().zip(b).all(|(a, b)| close(*a, *b, 0.03));
        assert!(
            near(hands, &other.2) && near(feet, &other.3),
            "высота {height}"
        );
        compared += 1;
    }
    assert!(compared > 20, "сравнено {compared}");
}

#[test]
fn the_grip_lets_go_at_the_ends_of_the_ladder_and_the_feet_find_the_floor_again() {
    let mut game = Setup::new().game();
    for (way, floor) in [("подъём", DECK_TOP), ("спуск", 0.0)] {
        ask(&mut game);
        let mut weights = Vec::new();
        climb_on(&mut game, 600, |game| {
            weights.push(climb::grip(&game.world, named(game, "hero")).map(|grip| grip.weight));
        });
        let held: Vec<f64> = weights.iter().flatten().copied().collect();
        assert!(held.contains(&1.0), "{way}");
        let first = held.first().copied().expect("хват был");
        let last = held.last().copied().expect("хват был");
        assert!(
            first < 0.2 && last == 0.0,
            "{way}: хват берётся и отпускается плавно: {first} … {last}"
        );
        assert!(climb::grip(&game.world, named(&game, "hero")).is_none());
        assert!(
            (spot(&game)[2] - floor).abs() < 1e-9,
            "{way}: после лестницы основание на {}, ждали {floor}",
            spot(&game)[2]
        );
        assert!(
            weights.iter().take(10).all(Option::is_none),
            "{way}: на подходе хвата нет"
        );
    }
}

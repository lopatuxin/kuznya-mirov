//! Фаза 59 — предмет в руках: `hold`, `hand`, `support`, вторая кисть на цевье и предмет в кадре.

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::model::Model;
use engine::core::property;
use engine::core::skeleton::{PoseScratch, Skeleton, Stance};
use engine::core::surface;
use engine::data::error::GameError;
use engine::data::load::{ModelFile, load_rest_with_motions, read_entry};
use engine::render::models::model_draws;

#[path = "motion_support/mod.rs"]
mod motion_support;
use motion_support::{
    ARM_RIGHT, Channel, ClipSpec, person, person_clips, point, pose_at, prop_glb, rig_glb,
};

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag"}}"#;
const MODEL: &str = r#"{"path":"models/rig.glb","idle":"Idle","walk":"Run","feet":["foot_l","foot_r"],"spine":"spine","hands":["hand_l","hand_r"]}"#;
const ITEM: &str = r#"{"path":"models/rifle.glb","hand":"right","support":"support"}"#;
const OTHER: &str = r#"{"path":"models/other.glb","hand":"right","support":"support"}"#;

struct Setup {
    rig: String,
    rifle: String,
    scene: String,
    rules: String,
    code: Option<String>,
    clips: Vec<ClipSpec>,
}

impl Setup {
    fn new() -> Setup {
        Setup {
            rig: MODEL.to_string(),
            rifle: ITEM.to_string(),
            scene: actor(r#","hold":"rifle""#),
            rules: r#"{"rules":[]}"#.to_string(),
            code: None,
            clips: person_clips(),
        }
    }

    fn game_text(&self) -> String {
        let code = self
            .code
            .as_ref()
            .map(|_| r#","code":"code.lua""#)
            .unwrap_or_default();
        format!(
            r##"{{"name":"M","scene":{{"width":40,"height":30,"background":"#000000","view_height":16,"camera":{{"pitch":55}}}},
"random_seed":1,"max_objects":64,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"models":{{"rig":{},"rifle":{},"other":{OTHER}}}{code}}}}}"##,
            self.rig, self.rifle
        )
    }

    fn load(&self) -> Result<(Game, Vec<GameError>), Vec<String>> {
        let text = self.game_text();
        let (config, entry_warnings) = read_entry(&text).map_err(|f| describe(&f.errors))?;
        let scene = format!(r#"{{"objects":[{}]}}"#, self.scene);
        let files = vec![
            ("rig".to_string(), rig_glb(&person(), &self.clips)),
            ("rifle".to_string(), prop_glb(true)),
            ("other".to_string(), prop_glb(true)),
        ]
        .into_iter()
        .map(|(name, bytes)| {
            (
                name,
                ModelFile {
                    bytes: Some(bytes),
                    images: Vec::new(),
                },
            )
        })
        .collect::<Vec<_>>();
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
            Ok((game, mut warnings)) => {
                warnings.retain(|warning| !warning.path.ends_with("→ other"));
                assert_eq!(warnings, Vec::new(), "{warnings:?}");
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

fn actor(extra: &str) -> String {
    format!(
        r#"{{"name":"actor","model":"rig","actor":true,"position":[5,5],"size":[0.6,0.6]{extra}}}"#
    )
}

fn model<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель загружена")
}

fn skeleton(game: &Game) -> Skeleton {
    model(game, "rig")
        .skeleton
        .clone()
        .expect("у человека есть скелет")
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|axis| close(a[axis], b[axis]))
}

fn expect_error(setup: &Setup, fragments: &[&str]) {
    let errors = setup
        .load()
        .err()
        .unwrap_or_else(|| panic!("игра должна была не загрузиться: {fragments:?}"));
    assert!(
        errors
            .iter()
            .any(|e| fragments.iter().all(|f| e.contains(f))),
        "нет ошибки со словами {fragments:?}: {errors:#?}"
    );
}

fn support_target(held: &[[f32; 4]; 4], item: &Model) -> [f32; 3] {
    let support = item.support.expect("точка цевья");
    let point: [f32; 3] = std::array::from_fn(|axis| support[axis] + item.building.shift[axis]);
    std::array::from_fn(|row| (0..3).map(|k| held[k][row] * point[k]).sum::<f32>() + held[3][row])
}

fn check_grip(skeleton: &Skeleton, item: &Model, scratch: &mut PoseScratch) {
    let held = skeleton
        .hold_item(item, true, scratch)
        .expect("кисть с предметом");
    let target = support_target(&held, item);
    let support_hand = match item.held_hand {
        engine::core::model::HeldHand::Left => "hand_r",
        engine::core::model::HeldHand::Right => "hand_l",
    };
    assert!(
        near(point(skeleton, scratch, support_hand), target),
        "{support_hand}: {:?} вместо {target:?}",
        point(skeleton, scratch, support_hand)
    );
}

#[test]
fn the_item_origin_sits_on_the_right_hand_and_the_left_hand_holds_support_through_the_run() {
    let game = Setup::new().game();
    let draws = model_draws(&game);
    let rifle = game.world.models().index_of("rifle").expect("автомат");
    let instance = draws.instances[rifle][0];
    assert_eq!(instance.attachment, [0.0, 0.0, 0.0, 1.0]);
    let skeleton = skeleton(&game);
    let id = game.world.ids().next().expect("герой");
    let mut rest = PoseScratch::default();
    skeleton.pose(&game.world.motion(id).blend(&skeleton), &mut rest);
    let stance = Stance {
        place: [5.3, 5.3],
        cos: 1.0,
        sin: 0.0,
        scale: 1.0,
        base: game.world.base_z(id),
    };
    skeleton.plant_feet(
        &stance,
        |point| Some(surface::ground_at(&game.world, id, point, stance.base)),
        &mut rest,
    );
    let item = model(&game, "rifle");
    let hand_matrix = skeleton.hold_item(item, true, &mut rest).expect("кисть");
    let hand = hand_matrix[3];
    let shift = item.building.shift;
    assert!(
        close(instance.placement[0] - shift[0], 5.3 + hand[0]),
        "{instance:?}"
    );
    assert!(
        close(instance.placement[1] - shift[1], 5.3 + hand[1]),
        "{instance:?}; кисть {hand:?}"
    );
    assert!(
        close(instance.base[0] - shift[2], stance.base as f32 + hand[2]),
        "{instance:?}; кисть {hand_matrix:?}; база {}",
        stance.base
    );

    let run = skeleton.clip_named("Run").expect("бег");
    for phase in [0.0, 0.25, 0.5, 0.75] {
        let mut scratch = pose_at(&skeleton, run, phase);
        check_grip(&skeleton, item, &mut scratch);
    }
}

#[test]
fn hand_left_attaches_the_item_to_the_other_hand() {
    let mut setup = Setup::new();
    setup.rifle = ITEM.replace(r#""hand":"right""#, r#""hand":"left""#);
    let game = setup.game();
    let rifle = game.world.models().index_of("rifle").expect("автомат");
    let instance = model_draws(&game).instances[rifle][0];
    assert!(close(instance.placement[0], 5.15));
    let skeleton = skeleton(&game);
    let idle = skeleton.idle.expect("стойка");
    let mut scratch = pose_at(&skeleton, idle, 0.0);
    check_grip(&skeleton, model(&game, "rifle"), &mut scratch);
}

#[test]
fn the_item_turns_with_the_holding_hand() {
    let mut setup = Setup::new();
    let half = std::f32::consts::FRAC_1_SQRT_2;
    setup.clips.push(ClipSpec {
        name: "Turn",
        channels: vec![Channel {
            node: ARM_RIGHT,
            path: "rotation",
            times: vec![0.0, 1.0],
            values: vec![vec![half, 0.0, 0.0, half], vec![half, 0.0, 0.0, half]],
        }],
    });
    setup.scene = actor(r#","hold":"rifle","animation":"Turn""#);
    let mut game = setup.game();
    for _ in 0..12 {
        game.step(StepInput::empty());
    }
    let rifle = game.world.models().index_of("rifle").expect("автомат");
    let attachment = model_draws(&game).instances[rifle][0].attachment;
    let skeleton = skeleton(&game);
    let turn = skeleton.clip_named("Turn").expect("поворот");
    let mut scratch = pose_at(&skeleton, turn, 0.0);
    let hand = skeleton
        .hold_item(model(&game, "rifle"), true, &mut scratch)
        .expect("кисть");
    assert!(
        (0..4).all(|axis| close(attachment[axis], [-half, 0.0, 0.0, half][axis])),
        "{attachment:?}; кисть {hand:?}"
    );
}

#[test]
fn lua_can_replace_the_held_model_without_changing_the_motion() {
    let mut setup = Setup::new();
    setup.code = Some("function equip(actor) actor.hold = \"other\" end".to_string());
    setup.rules = r#"{"rules":[{"kind":"check","for":{"has":["actor"]},"do":[["run","equip"]]}]}"#
        .to_string();
    let mut game = setup.game();
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let id = game.world.ids().next().expect("герой");
    assert_eq!(game.world.text(id, property::HOLD), Some("other"));
    let models = game.world.models();
    let draws = model_draws(&game);
    assert_eq!(
        draws.instances[models.index_of("other").expect("other")].len(),
        1
    );
    assert!(draws.instances[models.index_of("rifle").expect("rifle")].is_empty());
}

#[test]
fn an_unknown_item_written_by_lua_is_a_code_error() {
    let mut setup = Setup::new();
    setup.code = Some("function equip(actor) actor.hold = \"ghost\" end".to_string());
    setup.rules = r#"{"rules":[{"kind":"check","for":{"has":["actor"]},"do":[["run","equip"]]}]}"#
        .to_string();
    let mut game = setup.game();
    game.step(StepInput::empty());
    let error = game.code_error().expect("неизвестный предмет — ошибка");
    assert!(error.message.contains("модели \"ghost\" нет"), "{error:?}");
}

#[test]
fn broken_item_and_holder_declarations_stop_the_start() {
    let mut wrong_hand = Setup::new();
    wrong_hand.rifle = ITEM.replace(r#""hand":"right""#, r#""hand":"middle""#);
    expect_error(&wrong_hand, &["hand", "middle"]);

    let mut wrong_support = Setup::new();
    wrong_support.rifle = ITEM.replace(r#""support":"support""#, r#""support":"missing""#);
    expect_error(&wrong_support, &["support", "missing"]);

    let mut unknown = Setup::new();
    unknown.scene = actor(r#","hold":"ghost""#);
    expect_error(&unknown, &["модели \"ghost\" нет"]);

    let mut handless = Setup::new();
    handless.rig = MODEL.replace(r#","hands":["hand_l","hand_r"]"#, "");
    expect_error(&handless, &["hold", "hands"]);
}

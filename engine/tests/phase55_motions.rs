//! Фаза 55 — движения с поправками (`motions.json`) и стенд анимации. Модель со скелетом собирает сам тест
//! (`motion_support`): человек с тазом, ногами, спиной, грудью и руками, движения стойки, бега, взмаха, скольжения
//! правой ступни и ухода спины. Проверяются разбор и ошибки `motions.json`, оба предупреждения, каждая поправка в позе,
//! имя движения в `walk` игры и стенд: кости, длина, скольжение ступней, расхождение концов, кость под точкой и кольца.

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::skeleton::{Blend, Playing, PoseScratch, Skeleton};
use engine::data::error::GameError;
use engine::data::load::{ModelFile, load_rest_with_motions, read_entry};
use engine::data::stand::{Stand, StandView};

#[path = "motion_support/mod.rs"]
mod motion_support;
use motion_support::{person, person_clips, point, pose_at, rig_glb};

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag"}}"#;
const WALK_RULE: &str = r#"{"rules":[{"kind":"walk","for":{"has":["actor"]}}]}"#;
const MODEL: &str = r#"{"path":"models/rig.glb","idle":"Idle","walk":"Run","feet":["foot_l","foot_r"],"spine":"spine","hands":["hand_l","hand_r"]}"#;
const WINDOW: [f64; 2] = [1280.0, 720.0];

struct Setup {
    model: String,
    motions_path: Option<String>,
    motions: Option<String>,
    scene: String,
    rules: &'static str,
}

impl Setup {
    fn new(motions: &str) -> Setup {
        Setup {
            model: MODEL.to_string(),
            motions_path: Some("motions.json".to_string()),
            motions: Some(motions.to_string()),
            scene: actor(""),
            rules: r#"{"rules":[]}"#,
        }
    }

    fn load(&self) -> Result<(Game, Vec<GameError>), Vec<String>> {
        let motions = self
            .motions_path
            .as_ref()
            .map(|path| format!(r#","motions":"{path}""#))
            .unwrap_or_default();
        let text = format!(
            r##"{{"name":"M","scene":{{"width":40,"height":30,"background":"#000000","view_height":16,"camera":{{"pitch":55}}}},
"random_seed":1,"max_objects":64,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"models":{{"rig":{}}}{motions}}}}}"##,
            self.model
        );
        let scene = format!(r#"{{"objects":[{}]}}"#, self.scene);
        let (config, entry_warnings) = read_entry(&text).map_err(|f| describe(&f.errors))?;
        let files = vec![(
            "rig".to_string(),
            ModelFile {
                bytes: Some(rig_glb(&person(), &person_clips())),
                images: Vec::new(),
            },
        )];
        load_rest_with_motions(
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
            None,
            false,
            &[],
            None,
            &[],
            &[],
            &[],
            &files,
            None,
            &[],
            self.motions.as_deref(),
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
            Ok((game, _)) => game,
            Err(errors) => panic!("игра не загрузилась: {errors:#?}"),
        }
    }

    fn warnings(&self) -> Vec<String> {
        match self.load() {
            Ok((_, warnings)) => warnings
                .iter()
                .map(|w| format!("{} → {}: {}", w.file, w.path, w.message))
                .collect(),
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
        r#"{{"name":"actor","model":"rig","actor":true,"position":[5,5],"size":[0.6,0.6],"walk_speed":4{extra}}}"#
    )
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

fn skeleton_of(game: &Game) -> Skeleton {
    let models = game.world.models();
    models
        .get(models.index_of("rig").expect("модель объявлена"))
        .and_then(|model| model.skeleton.clone())
        .expect("у модели есть скелет")
}

fn clip(skeleton: &Skeleton, name: &str) -> usize {
    skeleton
        .clip_named(name)
        .unwrap_or_else(|| panic!("нет движения {name}"))
}

fn close(a: f32, b: f32, within: f32) -> bool {
    (a - b).abs() <= within
}

/// Игра с одним движением `motions.json`.
fn with_motion(body: &str) -> (Game, Skeleton) {
    let game = Setup::new(&format!(r#"{{"M":{body}}}"#)).game();
    let skeleton = skeleton_of(&game);
    (game, skeleton)
}

// -------------------------------------------------------------------------------------------
// Разбор и ошибки
// -------------------------------------------------------------------------------------------

#[test]
fn a_motion_of_motions_json_lies_on_the_skeleton_beside_the_clips_of_the_file() {
    let (_, skeleton) = with_motion(r#"{"model":"rig","clip":"Run","speed":2}"#);
    let motion = clip(&skeleton, "M");
    assert!(motion >= 5, "после пяти движений файла");
    assert!(
        close(skeleton.clips[motion].length, 0.5, 1e-6),
        "speed 2 вдвое короче: {}",
        skeleton.clips[motion].length
    );
    assert_eq!(skeleton.clips[clip(&skeleton, "Run")].length, 1.0);
}

#[test]
fn every_mistake_of_motions_json_stops_the_start_with_its_own_message() {
    let cases: [(&str, &[&str]); 20] = [
        (r#"{"M":{"clip":"Run"}}"#, &["motions.json", "model"]),
        (r#"{"M":{"model":"rig"}}"#, &["motions.json", "clip"]),
        (r#"{"M":{"model":"rig","clip":"Run","pace":1}}"#, &["pace"]),
        (
            r#"{"M":{"model":"ghost","clip":"Run"}}"#,
            &["ghost", "не объявлена"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Fly"}}"#,
            &["M → clip", "Fly", "\"Idle\", \"Run\""],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","upper":"Fly"}}"#,
            &["M → upper", "Fly"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","speed":0}}"#,
            &["speed", "больше нуля"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","speed":"fast"}}"#,
            &["speed", "число"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","loop":"yes"}}"#,
            &["loop", "true или false"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","bounce":"high"}}"#,
            &["bounce", "число"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","lean":[1]}}"#,
            &["lean", "число"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","arms_out":true}}"#,
            &["arms_out", "число"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","bones":{"spine":[1,2]}}}"#,
            &["bones → spine", "тройка"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","bones":{"spine":[{"at":1.5,"turn":[0,0,0]}]}}}"#,
            &["at", "от 0 до 1"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","bones":{"spine":[{"at":0.2,"turn":[0,0,0]},{"at":0.2,"turn":[1,1,1]}]}}}"#,
            &["две отметки в одной доле"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","bones":{"spine":[1,2,3,{"at":0,"turn":[0,0,0]}]}}}"#,
            &["вперемешку"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","bones":{"tail":[0,0,10]}}}"#,
            &["tail", "нет кости"],
        ),
        (
            r#"{"M":{"model":"rig","clip":"Run","upper":"Wave"},"W":{"model":"rig","clip":"Wave","upper":"Wave"},"N":{"model":"rig","clip":"Run","upper":"W"}}"#,
            &["само берёт верх"],
        ),
        (r#"{"Run":{"model":"rig","clip":"Wave"}}"#, &["уже занято"]),
        (r#"[1]"#, &["motions.json", "объект"]),
    ];
    for (motions, fragments) in cases {
        expect_error(&Setup::new(motions), fragments);
    }
}

#[test]
fn a_motion_needs_the_keys_of_the_model_it_asks_for() {
    let bare = r#"{"path":"models/rig.glb","idle":"Idle","feet":["foot_l","foot_r"]}"#;
    for (motion, fragments) in [
        (
            r#"{"model":"rig","clip":"Run","upper":"Wave"}"#,
            vec!["upper", "spine"],
        ),
        (
            r#"{"model":"rig","clip":"Run","lean":5}"#,
            vec!["lean", "spine"],
        ),
        (
            r#"{"model":"rig","clip":"Run","arms_out":5}"#,
            vec!["arms_out", "hands"],
        ),
    ] {
        let mut setup = Setup::new(&format!(r#"{{"M":{motion}}}"#));
        setup.model = bare.to_string();
        expect_error(&setup, &fragments);
    }
    let mut feetless = Setup::new(r#"{"M":{"model":"rig","clip":"Run","bounce":0}}"#);
    feetless.model = r#"{"path":"models/rig.glb"}"#.to_string();
    expect_error(&feetless, &["bounce", "feet"]);
}

#[test]
fn the_keys_spine_and_hands_of_the_model_are_checked_against_the_skeleton() {
    for (model, fragments) in [
        (
            r#"{"path":"models/rig.glb","spine":"neck"}"#,
            vec!["spine", "neck"],
        ),
        (
            r#"{"path":"models/rig.glb","spine":3}"#,
            vec!["spine", "строка"],
        ),
        (
            r#"{"path":"models/rig.glb","hands":["hand_l"]}"#,
            vec!["hands", "двух костей", "кисти"],
        ),
        (
            r#"{"path":"models/rig.glb","hands":["hand_l","hand_l"]}"#,
            vec!["hands", "одна и та же"],
        ),
        (
            r#"{"path":"models/rig.glb","hands":["hips","hand_r"]}"#,
            vec!["hands", "предплечье и плечо"],
        ),
    ] {
        let mut setup = Setup::new("{}");
        setup.model = model.to_string();
        expect_error(&setup, &fragments);
    }
}

#[test]
fn files_motions_must_be_a_json_file_that_is_there() {
    let mut wrong = Setup::new("{}");
    wrong.motions_path = Some("motions.txt".to_string());
    expect_error(&wrong, &["files → motions", ".json"]);
    let mut missing = Setup::new("{}");
    missing.motions = None;
    expect_error(&missing, &["motions.json", "файл не найден"]);
    let mut none = Setup::new("{}");
    none.motions_path = None;
    none.motions = None;
    assert!(none.load().is_ok(), "без files.motions движений просто нет");
}

#[test]
fn a_motion_of_motions_json_is_named_where_a_clip_of_the_file_is_named() {
    let mut setup = Setup::new(r#"{"Flat":{"model":"rig","clip":"Run","bounce":0}}"#);
    setup.model = MODEL.replace(r#""walk":"Run""#, r#""walk":"Flat""#);
    setup.scene = actor(r#","animation":"Flat""#);
    let game = setup.game();
    let skeleton = skeleton_of(&game);
    assert_eq!(skeleton.walk, skeleton.clip_named("Flat"));
    let id = game.world.ids().next().expect("объект");
    assert_eq!(game.world.text(id, property::ANIMATION), Some("Flat"));
    let mut unknown = Setup::new(r#"{"Flat":{"model":"rig","clip":"Run"}}"#);
    unknown.scene = actor(r#","animation":"Sharp""#);
    expect_error(&unknown, &["Sharp"]);
}

#[test]
fn an_unnamed_motion_and_a_repeating_one_that_does_not_close_are_only_warnings() {
    let unnamed = Setup::new(r#"{"Spare":{"model":"rig","clip":"Run"}}"#);
    let warnings = unnamed.warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Spare") && w.contains("нигде не названо")),
        "{warnings:?}"
    );
    let mut named = Setup::new(r#"{"Spare":{"model":"rig","clip":"Run"}}"#);
    named.scene = actor(r#","animation":"Spare""#);
    assert!(named.warnings().iter().all(|w| !w.contains("Spare")));
    let mut drift = Setup::new(r#"{"Drifting":{"model":"rig","clip":"Drift","loop":true}}"#);
    drift.scene = actor(r#","animation":"Drifting""#);
    let warnings = drift.warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Drifting") && w.contains("30°") && w.contains("spine")),
        "{warnings:?}"
    );
    let mut once = Setup::new(r#"{"Drifting":{"model":"rig","clip":"Drift","loop":false}}"#);
    once.scene = actor(r#","animation":"Drifting""#);
    assert!(
        once.warnings().iter().all(|w| !w.contains("расходится")),
        "движение один раз не замыкается"
    );
}

// -------------------------------------------------------------------------------------------
// Поправки в позе
// -------------------------------------------------------------------------------------------

#[test]
fn speed_and_loop_decide_how_long_the_motion_is_and_what_it_does_after_its_end() {
    let (_, skeleton) = with_motion(r#"{"model":"rig","clip":"Drift","speed":0.5,"loop":true}"#);
    let motion = clip(&skeleton, "M");
    assert!(close(skeleton.clips[motion].length, 2.0, 1e-6));
    let chest = |skeleton: &Skeleton, motion: usize, time: f32, looped: bool| {
        let mut scratch = PoseScratch::default();
        skeleton.pose(
            &Blend::single(Playing {
                clip: Some(motion),
                time,
                looped,
            }),
            &mut scratch,
        );
        point(skeleton, &scratch, "chest")
    };
    assert!(
        close(
            chest(&skeleton, motion, 2.5, false)[1],
            chest(&skeleton, motion, 0.5, false)[1],
            1e-4
        ),
        "loop: true повторяется, даже если зовут один раз"
    );
    assert!(
        chest(&skeleton, motion, 1.0, false)[1] > chest(&skeleton, motion, 0.0, false)[1],
        "спина уходит вперёд"
    );
    let (_, once) = with_motion(r#"{"model":"rig","clip":"Drift","loop":false}"#);
    let motion = clip(&once, "M");
    assert_eq!(
        chest(&once, motion, 1.0, true),
        chest(&once, motion, 7.0, true),
        "loop: false сильнее того, как движение позвали: замирает на последней позе"
    );
}

#[test]
fn upper_takes_the_body_above_the_spine_from_the_other_motion_and_the_legs_stay() {
    let (_, skeleton) = with_motion(r#"{"model":"rig","clip":"Run","upper":"Wave"}"#);
    let plain = pose_at(&skeleton, clip(&skeleton, "Run"), 0.0);
    let waved = pose_at(&skeleton, clip(&skeleton, "M"), 0.0);
    let hips = point(&skeleton, &waved, "hips");
    assert!(
        close(hips[2], 1.0, 1e-4),
        "таз бега, а не взмаха (1,3): {hips:?}"
    );
    assert_eq!(
        point(&skeleton, &waved, "foot_l"),
        point(&skeleton, &plain, "foot_l")
    );
    let hand = point(&skeleton, &waved, "hand_l");
    assert!(
        close(hand[0], 0.45, 1e-3),
        "рука взмаха вынесена вбок, а не висит: {hand:?}"
    );
    assert!(point(&skeleton, &plain, "hand_l")[0] < -0.1);
}

#[test]
fn bounce_scales_the_swing_of_the_pelvis_around_its_mean_height() {
    let height = |body: &str, phase: f32| {
        let (_, skeleton) = with_motion(body);
        let pose = pose_at(&skeleton, clip(&skeleton, "M"), phase);
        point(&skeleton, &pose, "hips")[2]
    };
    let swing = |bounce: &str| {
        let body = format!(r#"{{"model":"rig","clip":"Run","bounce":{bounce}}}"#);
        height(&body, 0.5) - height(&body, 0.0)
    };
    assert!(close(swing("1"), 0.1, 1e-4), "как в файле");
    assert!(close(swing("0"), 0.0, 1e-4), "без подскока");
    assert!(close(swing("0.5"), 0.05, 1e-4), "вполовину");
    assert!(close(swing("2"), 0.2, 1e-4), "вдвое сильнее");
    let level = height(r#"{"model":"rig","clip":"Run","bounce":0}"#, 0.0);
    let mean = (0..64)
        .map(|k| height(r#"{"model":"rig","clip":"Run"}"#, k as f32 / 64.0))
        .sum::<f32>()
        / 64.0;
    assert!(
        close(level, mean, 1e-3),
        "высота — среднее по кругу: {level} и {mean}"
    );
}

#[test]
fn lean_tips_the_trunk_forward_and_arms_out_spreads_the_arms_apart() {
    let (_, skeleton) = with_motion(r#"{"model":"rig","clip":"Idle","lean":20,"arms_out":30}"#);
    let plain = pose_at(&skeleton, clip(&skeleton, "Idle"), 0.0);
    let posed = pose_at(&skeleton, clip(&skeleton, "M"), 0.0);
    let (before, after) = (
        point(&skeleton, &plain, "chest"),
        point(&skeleton, &posed, "chest"),
    );
    let spine = point(&skeleton, &plain, "spine");
    assert!(
        after[1] - before[1] > 0.3 * (20.0f32).to_radians().sin() * 0.9,
        "грудь ушла вперёд (+y модели): {before:?} → {after:?}"
    );
    assert!(close(after[0], before[0], 1e-4), "наклон не уводит вбок");
    assert!(after[2] < before[2], "и опустилась");
    let reach = |p: [f32; 3]| ((p[1] - spine[1]).powi(2) + (p[2] - spine[2]).powi(2)).sqrt();
    assert!(
        close(reach(after), reach(before), 1e-3),
        "поворот не растягивает"
    );
    let (left, right) = (
        point(&skeleton, &posed, "hand_l"),
        point(&skeleton, &posed, "hand_r"),
    );
    let (left_before, right_before) = (
        point(&skeleton, &plain, "hand_l"),
        point(&skeleton, &plain, "hand_r"),
    );
    assert!(
        left[0] < left_before[0] - 0.05 && right[0] > right_before[0] + 0.05,
        "левая рука ушла влево, правая вправо: {left:?} {right:?}"
    );
    let lean_only = with_motion(r#"{"model":"rig","clip":"Idle","lean":-20}"#).1;
    let back = pose_at(&lean_only, clip(&lean_only, "M"), 0.0);
    assert!(
        point(&lean_only, &back, "chest")[1] < before[1],
        "отрицательный lean — назад"
    );
}

#[test]
fn a_turn_of_a_bone_swings_it_around_its_own_axis_in_the_direction_a_ring_names() {
    let (_, skeleton) = with_motion(r#"{"model":"rig","clip":"Idle","bones":{"arm_l":[0,0,30]}}"#);
    let plain = pose_at(&skeleton, clip(&skeleton, "Idle"), 0.0);
    let turned = pose_at(&skeleton, clip(&skeleton, "M"), 0.0);
    let arm = skeleton.bone_named("arm_l").expect("кость");
    let matrix = plain.globals()[arm];
    let unit = |column: usize| {
        let v = [matrix[column][0], matrix[column][1], matrix[column][2]];
        let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|c| c / length)
    };
    let (first, second) = (unit(0), unit(1));
    let angle_of = |pose: &engine::core::skeleton::PoseScratch| {
        let (start, end) = (
            point(&skeleton, pose, "arm_l"),
            point(&skeleton, pose, "forearm_l"),
        );
        let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        dot(d, second).atan2(dot(d, first)).to_degrees()
    };
    let swing = angle_of(&turned) - angle_of(&plain);
    assert!(
        close(swing.rem_euclid(360.0), 30.0, 0.05),
        "поворот на +30° вокруг z кости идёт от оси x к оси y кости: {swing}"
    );
}

#[test]
fn a_bone_with_marks_follows_them_smoothly_and_a_repeating_motion_returns_to_the_first() {
    let marks =
        r#"[{"at":0.25,"turn":[0,0,40]},{"at":0.5,"turn":[0,0,0]},{"at":0.75,"turn":[0,0,-40]}]"#;
    let body = |looped: bool| {
        format!(r#"{{"model":"rig","clip":"Idle","loop":{looped},"bones":{{"arm_l":{marks}}}}}"#)
    };
    let swing = |looped: bool, phase: f32| {
        let (_, skeleton) = with_motion(&body(looped));
        let plain = pose_at(&skeleton, clip(&skeleton, "Idle"), 0.0);
        let posed = pose_at(&skeleton, clip(&skeleton, "M"), phase);
        let (a, b) = (
            point(&skeleton, &plain, "hand_l"),
            point(&skeleton, &posed, "hand_l"),
        );
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
    };
    assert!(
        swing(true, 0.5) < 1e-4,
        "на отметке «ноль» кость стоит как в файле"
    );
    assert!(
        swing(true, 0.25) > 0.2 && swing(true, 0.75) > 0.2,
        "на отметках повёрнута"
    );
    assert!(
        close(swing(true, 0.25), swing(true, 0.75), 1e-3),
        "зеркальные отметки — равные уходы"
    );
    assert!(
        swing(true, 0.375) > 0.05 && swing(true, 0.375) < swing(true, 0.25),
        "между отметками плавно"
    );
    assert!(
        close(swing(true, 0.0), swing(true, 1.0), 1e-4),
        "повторяющееся движение: конец сходится с началом"
    );
    assert!(
        swing(true, 0.1) > 0.05 && close(swing(true, 0.1), swing(true, 0.9), 1e-3),
        "и через конец круга кость идёт плавно от последней отметки к первой"
    );
    assert!(
        close(swing(false, 0.0), swing(false, 0.1), 1e-4),
        "один раз — до первой отметки держится она"
    );
    assert!(
        close(swing(false, 0.9), swing(false, 1.0), 1e-4),
        "и после последней держится последняя"
    );
}

#[test]
fn a_game_that_names_a_motion_in_walk_plays_the_corrected_motion() {
    let mut setup = Setup::new(r#"{"Flat":{"model":"rig","clip":"Run","bounce":0}}"#);
    setup.model = MODEL.replace(r#""walk":"Run""#, r#""walk":"Flat""#);
    setup.scene = actor(r#","walk_to":[5.3,25]"#);
    setup.rules = WALK_RULE;
    let mut game = setup.game();
    let skeleton = skeleton_of(&game);
    let id = game.world.ids().next().expect("объект");
    for _ in 0..30 {
        game.step(StepInput::empty());
    }
    let blend = game.world.motion(id).blend(&skeleton);
    assert_eq!(blend.current().playing.clip, skeleton.clip_named("Flat"));
    assert_eq!(blend.layers().len(), 1, "переход давно кончился");
    let heights: Vec<f32> = (0..8)
        .map(|k| {
            let pose = pose_at(&skeleton, clip(&skeleton, "Flat"), k as f32 / 8.0);
            point(&skeleton, &pose, "hips")[2]
        })
        .collect();
    assert!(
        heights.windows(2).all(|pair| close(pair[0], pair[1], 1e-4)),
        "таз идущего не подскакивает: {heights:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Стенд
// -------------------------------------------------------------------------------------------

fn stand_for(motions: &str, name: &str) -> (Game, Result<Stand, Vec<String>>) {
    stand_seen(motions, name, StandView::Side)
}

fn stand_seen(motions: &str, name: &str, view: StandView) -> (Game, Result<Stand, Vec<String>>) {
    let game = Setup::new(motions).game();
    let stand = Stand::build(&game, motions, name, view, WINDOW);
    (game, stand)
}

#[test]
fn the_stand_gives_the_bones_the_length_and_the_frames_of_the_motion() {
    let motions = r#"{"Slow":{"model":"rig","clip":"Run","speed":0.5}}"#;
    let (_, stand) = stand_for(motions, "Slow");
    let stand = stand.expect("стенд собрался");
    assert_eq!(stand.bones.len(), 17, "все узлы файла");
    assert_eq!(stand.bones[0].name, "armature");
    assert_eq!(stand.bones[0].parent, None);
    assert_eq!(stand.bones[2].parent, Some(1));
    assert!(
        close(stand.length, 2.0, 1e-6),
        "вдвое медленнее — вдвое дольше"
    );
    assert_eq!(stand.frames, 2, "три ключа — два кадра");
}

#[test]
fn the_stand_refuses_what_the_game_would_refuse_and_says_why_in_words() {
    let motions = r#"{"Bad":{"model":"rig","clip":"Fly"}}"#;
    let game = Setup::new(r#"{"Ok":{"model":"rig","clip":"Run"}}"#).game();
    let errors = Stand::build(&game, motions, "Bad", StandView::Side, WINDOW)
        .err()
        .expect("такого движения в файле нет");
    assert!(errors.iter().any(|e| e.contains("Fly")), "{errors:?}");
    let errors = Stand::build(&game, "{", "Bad", StandView::Side, WINDOW)
        .err()
        .expect("не JSON");
    assert!(!errors.is_empty());
    let errors = Stand::build(&game, motions, "Gone", StandView::Side, WINDOW)
        .err()
        .expect("нет движения");
    assert!(errors.iter().any(|e| e.contains("Gone")), "{errors:?}");
}

fn slips_of_clip(clip: &str) -> Vec<(usize, f32, f32)> {
    let motions = format!(r#"{{"S":{{"model":"rig","clip":"{clip}"}}}}"#);
    let (_, stand) = stand_for(&motions, "S");
    let stand = stand.expect("стенд собрался");
    stand
        .analysis
        .slips
        .iter()
        .map(|slip| (slip.foot, slip.from, slip.to))
        .collect()
}

#[test]
fn on_a_standing_body_any_movement_of_a_foot_on_the_ground_is_a_slip() {
    let slips = slips_of_clip("Skate");
    assert!(!slips.is_empty());
    assert!(
        slips.iter().all(|slip| slip.0 == 1),
        "скользит только вторая ступня: {slips:?}"
    );
    assert!(
        close(slips[0].1, 0.0, 1e-6) && close(slips.last().map_or(0.0, |s| s.2), 0.5, 0.03),
        "половина круга, пока ступня едет туда и обратно: {slips:?}"
    );
    assert!(
        slips_of_clip("Idle").is_empty(),
        "стоящая ступня не скользит"
    );
}

#[test]
fn a_run_in_place_whose_stance_feet_go_back_at_one_speed_has_no_slips() {
    assert_eq!(
        slips_of_clip("Treadmill"),
        Vec::new(),
        "обе ступни уходят назад с одной скоростью — это скорость земли"
    );
}

#[test]
fn a_foot_that_speeds_up_in_the_middle_of_the_stance_slips_there() {
    let slips = slips_of_clip("Stumble");
    assert!(!slips.is_empty());
    assert!(
        slips.iter().all(|slip| slip.0 == 0),
        "скользит только левая ступня: {slips:?}"
    );
    assert!(
        close(slips[0].1, 0.25, 0.03) && close(slips.last().map_or(0.0, |s| s.2), 0.5, 0.03),
        "вторая половина стойки левой ступни: {slips:?}"
    );
}

#[test]
fn the_end_of_a_repeating_motion_that_does_not_meet_its_start_is_reported_with_the_bone() {
    let (_, stand) = stand_for(r#"{"D":{"model":"rig","clip":"Drift"}}"#, "D");
    let stand = stand.expect("стенд");
    let (degrees, bone) = stand.analysis.end_gap.expect("30° расхождения");
    assert!(close(degrees, 30.0, 0.5), "{degrees}");
    assert_eq!(stand.bones[bone].name, "spine");
    let (_, stand) = stand_for(r#"{"D":{"model":"rig","clip":"Drift","loop":false}}"#, "D");
    assert_eq!(
        stand.expect("стенд").analysis.end_gap,
        None,
        "один раз — конец с началом не сходятся и не должны"
    );
    let (_, stand) = stand_for(r#"{"R":{"model":"rig","clip":"Run"}}"#, "R");
    assert_eq!(stand.expect("стенд").analysis.end_gap, None);
}

#[test]
fn the_joints_on_the_canvas_follow_the_pose_and_the_bone_under_a_point_is_the_nearest_one() {
    let motions = r#"{"W":{"model":"rig","clip":"Idle","bones":{"arm_l":[{"at":0,"turn":[0,0,0]},{"at":1,"turn":[0,0,90]}]}}}"#;
    let (_, stand) = stand_seen(motions, "W", StandView::Front);
    let mut stand = stand.expect("стенд");
    let camera = stand.camera(WINDOW);
    stand.pose(0.0);
    let start = stand.joints(&camera);
    stand.pose(0.5);
    let end = stand.joints(&camera);
    let hand = 12;
    let (a, b) = (start[hand].expect("видна"), end[hand].expect("видна"));
    assert!(
        (a[0] - b[0]).abs() + (a[1] - b[1]).abs() > 20.0,
        "рука на холсте переехала: {a:?} → {b:?}"
    );
    assert_eq!(start[1], end[1], "таз стоит на месте");
    stand.pose(0.0);
    let joints = stand.joints(&camera);
    let spine = joints[8].expect("видна");
    assert_eq!(
        stand.bone_at(&camera, [spine[0] + 1.0, spine[1] - 1.0]),
        Some(8)
    );
    assert_eq!(stand.bone_at(&camera, [5.0, 5.0]), None, "в углу кости нет");
}

#[test]
fn a_motion_without_loop_is_posed_on_the_stand_as_a_repeating_one_around_its_last_mark() {
    let marks = r#"[{"at":0.25,"turn":[0,0,40]},{"at":0.5,"turn":[0,0,-40]}]"#;
    let joints = |loop_key: &str| {
        let motions = format!(
            r#"{{"W":{{"model":"rig","clip":"Idle"{loop_key},"bones":{{"arm_l":{marks}}}}}}}"#
        );
        let (_, stand) = stand_seen(&motions, "W", StandView::Front);
        let mut stand = stand.expect("стенд");
        let camera = stand.camera(WINDOW);
        stand.pose(0.8);
        stand.joints(&camera)[12].expect("видна")
    };
    assert_eq!(joints(""), joints(r#","loop":true"#));
    assert_ne!(joints(""), joints(r#","loop":false"#));
}

#[test]
fn the_drawn_model_follows_the_same_loop_as_the_joints_for_a_motion_without_loop() {
    let marks = r#"[{"at":0.25,"turn":[0,0,40]},{"at":0.5,"turn":[0,0,-40]}]"#;
    let bones_of = |loop_key: &str, phase: f32| {
        let motions = format!(
            r#"{{"W":{{"model":"rig","clip":"Idle"{loop_key},"bones":{{"arm_l":{marks}}}}}}}"#
        );
        let (_, stand) = stand_seen(&motions, "W", StandView::Front);
        let mut stand = stand.expect("стенд");
        let model = stand.model();
        stand.pose(phase);
        frame_of(&stand).model_bones[model].clone()
    };
    let without = bones_of("", 0.8);
    assert_eq!(
        without,
        bones_of(r#","loop":true"#, 0.8),
        "кости на экране те же, что у loop: true"
    );
    assert_ne!(without, bones_of(r#","loop":false"#, 0.8));
    assert_eq!(
        bones_of("", 1.0),
        bones_of("", 0.0),
        "у повторяющегося движения конец круга — его начало, как и у суставов"
    );
}

#[test]
fn a_ring_dragged_along_its_circle_reports_the_angle_and_the_turn_after_it_swings_the_bone_that_way()
 {
    let motions = r#"{"P":{"model":"rig","clip":"Idle"}}"#;
    let (_, stand) = stand_seen(motions, "P", StandView::Front);
    let mut stand = stand.expect("стенд");
    stand.pose(0.0);
    let camera = stand.camera(WINDOW);
    let arm = 10;
    let rings = stand.rings(&camera, arm).expect("кольца");
    assert!(rings.center.is_some());
    for axis in 0..3 {
        let ring = &rings.rings[axis];
        assert_eq!(ring.len(), 48);
        let on = ring[3].expect("точка кольца видна");
        assert_eq!(stand.ring_at(&camera, arm, on), Some(axis));
    }
    let ring = &rings.rings[2];
    let (from, to) = (ring[0].expect("видна"), ring[4].expect("видна"));
    stand.grab_ring(&camera, arm, 2, from);
    let swung = stand.drag_ring(&camera, to).expect("хват есть");
    assert!(
        close(swung as f32, 30.0, 0.1),
        "четыре шага по 7,5°: {swung}"
    );
    let back = stand.drag_ring(&camera, from).expect("хват есть");
    assert!(close(back as f32, 0.0, 0.1), "и обратно: {back}");
    stand.release_ring();
    assert_eq!(stand.drag_ring(&camera, to), None, "без хвата угла нет");
}

fn frame_of(stand: &Stand) -> engine::render::scene3d::Frame3d {
    use engine::render::relief::FlatShapes;
    use engine::render::scene3d::compose_frame3d;
    use engine::render::wind::Motion;
    compose_frame3d(
        stand.game(),
        &stand.camera(WINDOW),
        &Motion::default(),
        &[],
        &[],
        &mut FlatShapes::default(),
    )
}

#[test]
fn the_world_of_the_stand_composes_into_a_frame_with_one_model_in_the_pose_of_the_phase() {
    let (_, stand) = stand_for(r#"{"R":{"model":"rig","clip":"Run"}}"#, "R");
    let mut stand = stand.expect("стенд");
    let model = stand.game().world.models().index_of("rig").expect("модель");
    let frame_at = |stand: &mut Stand, phase: f32| {
        stand.pose(phase);
        frame_of(stand)
    };
    let start = frame_at(&mut stand, 0.0);
    assert_eq!(start.models[model].len(), 1, "одна модель в мире просмотра");
    assert_eq!(
        start.model_bones[model].len(),
        16 * 15,
        "матрицы пятнадцати суставов"
    );
    assert!(
        start.shapes.is_empty(),
        "фигур в мире просмотра нет, только пол"
    );
    let peak = frame_at(&mut stand, 0.5);
    assert_ne!(
        start.model_bones[model], peak.model_bones[model],
        "доля двигает позу в кадре"
    );
}

#[test]
fn the_stand_puts_the_model_on_a_light_grey_floor_with_a_fine_grid_seen_from_every_side() {
    for view in [StandView::Side, StandView::Front, StandView::Game] {
        let (_, stand) = stand_seen(r#"{"R":{"model":"rig","clip":"Run"}}"#, "R", view);
        let stand = stand.expect("стенд");
        let frame = frame_of(&stand);
        let scene = stand.game().scene;
        let middle = [f64::from(scene.width) / 2.0, f64::from(scene.height) / 2.0];
        let floor = &frame.ground[0];
        assert_eq!(floor.size, [6.0, 6.0]);
        assert_eq!(
            [floor.position[0], floor.position[1]],
            [middle[0] as f32 - 3.0, middle[1] as f32 - 3.0],
            "модель стоит посередине пола"
        );
        assert!(
            floor.color[0] == floor.color[1] && floor.color[1] == floor.color[2],
            "пол нейтральный: {:?}",
            floor.color
        );
        assert!(floor.color[0] > 0.6 && floor.color[0] < 0.9, "и светлый");
        let lines = &frame.ground[1..];
        assert_eq!(lines.len(), 26, "по тринадцать линий через полметра");
        assert!(
            lines.iter().all(|line| line.color[0] < floor.color[0]
                && (line.size[0].min(line.size[1]) - 0.03).abs() < 1e-6
                && (line.size[0].max(line.size[1]) - 6.0).abs() < 1e-6),
            "линии темнее пола и тонкие"
        );
        let mut across: Vec<f32> = lines
            .iter()
            .filter(|line| line.size[0] < line.size[1])
            .map(|line| line.position[0] + line.size[0] / 2.0)
            .collect();
        across.sort_by(f32::total_cmp);
        assert!(
            across
                .windows(2)
                .all(|pair| close(pair[1] - pair[0], 0.5, 1e-4)),
            "клетка полметра: {across:?}"
        );
        assert!(
            !frame.surface.is_empty(),
            "пол лежит на земле треугольниками"
        );
        let camera = stand.camera(WINDOW);
        for corner in [
            [-3.0, -3.0],
            [3.0, -3.0],
            [3.0, 3.0],
            [-3.0, 3.0],
            [0.0, 0.0],
        ] {
            let seen = camera.project([middle[0] + corner[0], middle[1] + corner[1], 0.0]);
            assert!(seen.is_some(), "{view:?}: пол впереди камеры");
        }
        let feet = camera
            .project([middle[0], middle[1], 0.0])
            .expect("ступни видны");
        assert!(
            feet[1] > 0.0 && feet[1] < WINDOW[1],
            "{view:?}: ступни в кадре"
        );
    }
}

#[test]
fn every_view_frames_the_whole_model_so_that_it_fills_about_four_fifths_of_the_window() {
    for view in [StandView::Side, StandView::Front, StandView::Game] {
        let (_, stand) = stand_seen(r#"{"R":{"model":"rig","clip":"Run"}}"#, "R", view);
        let stand = stand.expect("стенд");
        let camera = stand.camera(WINDOW);
        let scene = stand.game().scene;
        let (x, y) = (f64::from(scene.width) / 2.0, f64::from(scene.height) / 2.0);
        let feet = camera.project([x, y, 0.0]).expect("ступни видны");
        let head = camera.project([x, y, 1.8]).expect("макушка видна");
        let share = (feet[1] - head[1]).abs() / WINDOW[1];
        assert!(
            (0.72..=0.85).contains(&share),
            "{view:?}: модель занимает {share:.2} высоты окна"
        );
        assert!(
            head[1] > 0.0 && feet[1] < WINDOW[1],
            "{view:?}: от ступней до макушки всё в кадре: {head:?} {feet:?}"
        );
    }
    let (_, stand) = stand_seen(
        r#"{"R":{"model":"rig","clip":"Run"}}"#,
        "R",
        StandView::Side,
    );
    let side = stand.expect("стенд").camera(WINDOW);
    assert!(
        side.pitch < 15.0,
        "сбоку смотрит почти ровно: {}",
        side.pitch
    );
}

#[test]
fn the_game_view_is_fitted_with_the_same_pitch_it_is_drawn_with_when_the_game_pitch_is_out_of_range()
 {
    use engine::core::scene::{CameraConfig, Projection};
    let motions = r#"{"R":{"model":"rig","clip":"Run"}}"#;
    let mut game = Setup::new(motions).game();
    game.scene.camera = Some(CameraConfig {
        pitch: 2.0,
        projection: Projection::Perspective,
    });
    let stand = Stand::build(&game, motions, "R", StandView::Game, WINDOW).expect("стенд");
    let camera = stand.camera(WINDOW);
    assert_eq!(
        camera.pitch, 5.0,
        "рисуется с наклоном в пределах камеры редактора"
    );
    let scene = stand.game().scene;
    let (x, y) = (f64::from(scene.width) / 2.0, f64::from(scene.height) / 2.0);
    let feet = camera.project([x, y, 0.0]).expect("видна");
    let head = camera.project([x, y, 1.8]).expect("видна");
    let share = (feet[1] - head[1]).abs() / WINDOW[1];
    assert!(
        (0.72..=0.85).contains(&share),
        "модель занимает {share:.2} окна"
    );
}

#[test]
fn the_game_view_of_the_stand_honours_an_orthographic_game_camera_and_the_side_views_do_not() {
    use engine::core::scene::{CameraConfig, Projection};
    let motions = r#"{"R":{"model":"rig","clip":"Run"}}"#;
    let mut game = Setup::new(motions).game();
    game.scene.camera = Some(CameraConfig {
        pitch: 50.0,
        projection: Projection::Orthographic,
    });
    let seen = |view| {
        Stand::build(&game, motions, "R", view, WINDOW)
            .map(|mut stand| {
                stand.set_view(view, WINDOW);
                stand
            })
            .expect("стенд")
    };
    let stand = seen(StandView::Game);
    let camera = stand.camera(WINDOW);
    assert_eq!(camera.projection, Projection::Orthographic);
    assert_eq!(camera.pitch, 50.0);
    let scene = stand.game().scene;
    let (x, y) = (f64::from(scene.width) / 2.0, f64::from(scene.height) / 2.0);
    let feet = camera.project([x, y, 0.0]).expect("видна");
    let head = camera.project([x, y, 1.8]).expect("видна");
    let share = (feet[1] - head[1]).abs() / WINDOW[1];
    assert!(
        (0.72..=0.85).contains(&share),
        "модель занимает {share:.2} окна"
    );
    let mut rebuilt = seen(StandView::Side);
    assert_eq!(rebuilt.camera(WINDOW).projection, Projection::Perspective);
    rebuilt.keep_camera(&stand);
    assert_eq!(
        rebuilt.camera(WINDOW),
        camera,
        "вид держится после пересборки"
    );
    assert_eq!(
        seen(StandView::Front).camera(WINDOW).projection,
        Projection::Perspective
    );
}

#[test]
fn the_rings_of_a_bone_are_about_a_seventh_of_the_height_of_the_model() {
    let (_, stand) = stand_seen(
        r#"{"P":{"model":"rig","clip":"Idle"}}"#,
        "P",
        StandView::Front,
    );
    let mut stand = stand.expect("стенд");
    stand.pose(0.0);
    let camera = stand.camera(WINDOW);
    let scene = stand.game().scene;
    let (x, y) = (f64::from(scene.width) / 2.0, f64::from(scene.height) / 2.0);
    let feet = camera.project([x, y, 0.0]).expect("видна");
    let head = camera.project([x, y, 1.8]).expect("видна");
    let height = (feet[1] - head[1]).abs();
    let rings = stand.rings(&camera, 8).expect("кольца");
    let centre = rings.center.expect("середина видна");
    let widest = rings
        .rings
        .iter()
        .flatten()
        .flatten()
        .map(|point| (point[0] - centre[0]).hypot(point[1] - centre[1]))
        .fold(0.0, f64::max);
    let share = widest / height;
    assert!(
        (0.12..=0.16).contains(&share),
        "кольцо — {share:.3} роста модели"
    );
}

#[test]
fn the_view_follows_the_wheel_and_the_dragging_and_keeps_the_pitch_in_its_limits() {
    let (_, stand) = stand_for(r#"{"R":{"model":"rig","clip":"Run"}}"#, "R");
    let mut stand = stand.expect("стенд");
    let near = stand.camera(WINDOW);
    stand.zoom(-300.0);
    let closer = stand.camera(WINDOW);
    assert!(
        closer.view_height < near.view_height,
        "колесо к себе приближает"
    );
    stand.turn(100.0, 0.0);
    assert!(stand.camera(WINDOW).yaw != closer.yaw);
    stand.turn(0.0, 10_000.0);
    assert!(stand.camera(WINDOW).pitch <= 90.0);
    stand.set_view(StandView::Front, WINDOW);
    assert!(close(stand.camera(WINDOW).yaw as f32, 0.0, 1e-6));
    stand.set_view(StandView::Side, WINDOW);
    assert!(close(stand.camera(WINDOW).yaw as f32, 90.0, 1e-6));
    stand.set_view(StandView::Game, WINDOW);
    assert!(
        close(stand.camera(WINDOW).pitch as f32, 55.0, 1e-6),
        "наклон камеры игры"
    );
}

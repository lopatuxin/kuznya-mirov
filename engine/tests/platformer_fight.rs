//! Фаза 06 платформера — проба боя на настоящей игре `games/platformer`: Илья рубит топором, сам
//! парирует, уворачивается и пропускает удары разбойника; каждый третий удар разбойника особый, его
//! защита не останавливает, а прыжок спасает.

use std::fs;

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::property;
use engine::core::screens::{self, ButtonCommand, ScreenState, ScreensConfig};
use engine::data::load::{load_rest_with_stamps, read_entry};
use engine::data::session::PlaySession;

#[path = "platformer_support/mod.rs"]
mod support;
use support::{platformer_dir, read};

const VIEWPORT: [f32; 2] = [800.0, 600.0];
const HERO_START: &str = r#""name": "Герой", "position": [3, 7.1]"#;

/// Партия первого уровня, где Илья поставлен в точку `hero_at` (левый верхний угол его прямоугольника).
struct Fight {
    game: Game,
    config: ScreensConfig,
    state: ScreenState,
    session: PlaySession,
}

impl Fight {
    fn start(hero_at: [f64; 2]) -> Self {
        Fight::start_as(hero_at, "ilya")
    }

    /// То же за героя `hero_id` из таблицы героев.
    fn start_as(hero_at: [f64; 2], hero_id: &str) -> Self {
        let game_json = read("game.json");
        let (config, _) = read_entry(&game_json).expect("game.json разобран");
        let images = support::image_verdicts(&config.files.images);
        let tables = support::table_texts(&config.files.tables);
        let fonts: Vec<(String, Option<Vec<u8>>)> = config
            .files
            .fonts
            .iter()
            .map(|(name, path)| (name.clone(), fs::read(platformer_dir().join(path)).ok()))
            .collect();
        let scene = read("scene.json");
        assert!(scene.contains(HERO_START), "Илья не на старте в scene.json");
        let scene = scene.replace(
            HERO_START,
            &format!(
                r#""name": "Герой", "position": [{}, {}]"#,
                hero_at[0], hero_at[1]
            ),
        );
        let scene = scene.replace(r#""hero_id": "ilya""#, &format!(r#""hero_id": "{hero_id}""#));
        let (mut game, config, _, _) = load_rest_with_stamps(
            &game_json,
            config,
            Some(&read("properties.json")),
            Some(&scene),
            Some(&read("rules.json")),
            Some(&read("screens.json")),
            &fonts,
            &[],
            &[],
            &images,
            Some(&read("code.lua")),
            false,
            &tables,
            None,
            &[],
            &[],
            &[],
        )
        .unwrap_or_else(|e| panic!("уровень не загрузился: {e:?}"));
        let mut state = ScreenState::new(config.start_screen);
        let session = PlaySession::begin_live(&mut game, &config, &mut state);
        let level = config
            .screens
            .iter()
            .position(|screen| screen.name == "game")
            .expect("экран game");
        screens::apply_command(
            ButtonCommand::NewGame(level, None),
            &mut game,
            &config,
            &mut state,
        );
        Fight {
            game,
            config,
            state,
            session,
        }
    }

    /// Один шаг мира; `down` и `up` — клавиши, нажатые и отпущенные перед ним.
    fn step(&mut self, down: &[&str], up: &[&str]) {
        let mut queue = UiQueue::new();
        for code in down {
            queue.push_key_down(code);
        }
        for code in up {
            queue.push_key_up(code);
        }
        let mut mouse = MouseState::default();
        self.session.step_once(
            &mut queue,
            &mut mouse,
            &mut self.game,
            &self.config,
            &mut self.state,
            VIEWPORT,
            &[],
        );
        if let Some(error) = self.game.code_error() {
            panic!("ошибка кода игры: {}", error.message);
        }
    }

    fn prop(&self, name: &str) -> property::PropertyId {
        self.game
            .properties
            .resolve(name)
            .unwrap_or_else(|| panic!("нет свойства {name}"))
    }

    fn hero(&self) -> u32 {
        let hero = self.prop("hero");
        self.game
            .world
            .ids()
            .find(|&id| self.game.world.has(id, hero))
            .expect("нет Ильи")
    }

    fn robber(&self, no: f64) -> u32 {
        let robber = self.prop("robber");
        let robber_no = self.prop("robber_no");
        self.game
            .world
            .ids()
            .find(|&id| {
                self.game.world.has(id, robber)
                    && self.game.world.number_like(id, robber_no) == Some(no)
            })
            .expect("нет разбойника")
    }

    fn text(&self, id: u32, name: &str) -> String {
        self.game
            .world
            .text(id, self.prop(name))
            .unwrap_or_default()
            .to_string()
    }

    fn number(&self, id: u32, name: &str) -> f64 {
        self.game
            .world
            .number_like(id, self.prop(name))
            .unwrap_or(0.0)
    }

    fn flag(&self, id: u32, name: &str) -> bool {
        self.game.world.flag(id, self.prop(name))
    }

    fn center_x(&self, id: u32) -> f64 {
        let p = self.game.world.vec2(id, property::POSITION).unwrap();
        let s = self.game.world.vec2(id, property::SIZE).unwrap();
        p[0] + s[0] / 2.0
    }
}

/// Илья у первого разбойника на стартовой поляне: середина разбойника — 15, Илья в 1,6 клетки левее.
const BY_THE_FIRST_ROBBER: [f64; 2] = [13.0, 7.1];

/// Удар разбойника, дошедший до Ильи: был ли он особым и чем Илья ответил.
#[derive(Debug, Clone, PartialEq)]
struct Blow {
    special: bool,
    outcome: String,
}

/// Шагает `steps` шагов, нажимая клавиши по `keys(fight, шаг)`, и собирает удары первого разбойника,
/// дошедшие до Ильи, и особые удары, которые прошли мимо.
fn watch_blows(
    fight: &mut Fight,
    steps: usize,
    mut keys: impl FnMut(&Fight, usize) -> (Vec<&'static str>, Vec<&'static str>),
) -> (Vec<Blow>, usize) {
    let robber = fight.robber(1.0);
    let hero = fight.hero();
    let mut blows = Vec::new();
    let mut missed_specials = 0;
    let mut was_done = false;
    let mut was_mood = String::new();
    for tick in 0..steps {
        let (down, up) = keys(fight, tick);
        fight.step(&down, &up);
        let done = fight.flag(robber, "blow_done");
        let mood = fight.text(robber, "mood");
        if done && !was_done {
            blows.push(Blow {
                special: fight.flag(robber, "special"),
                outcome: fight.text(hero, "act"),
            });
        }
        if was_mood == "strike" && mood == "rest" && !done && fight.flag(robber, "special") {
            missed_specials += 1;
        }
        was_done = done;
        was_mood = mood;
    }
    (blows, missed_specials)
}

#[test]
fn ilya_standing_by_a_robber_parries_dodges_and_misses_and_every_third_blow_is_special() {
    let mut fight = Fight::start(BY_THE_FIRST_ROBBER);
    let (blows, _) = watch_blows(&mut fight, 30 * 60, |_, _| (vec![], vec![]));
    assert!(blows.len() >= 12, "ударов за 30 секунд: {}", blows.len());
    for (i, blow) in blows.iter().enumerate() {
        assert_eq!(blow.special, (i + 1) % 3 == 0, "удар {}: {blows:?}", i + 1);
        if blow.special {
            assert_eq!(
                blow.outcome, "hurt",
                "особый удар защита не держит: {blows:?}"
            );
        } else {
            assert!(
                ["parry", "dodge", "hurt"].contains(&blow.outcome.as_str()),
                "{blows:?}"
            );
        }
    }
    let ordinary: Vec<&str> = blows
        .iter()
        .filter(|b| !b.special)
        .map(|b| b.outcome.as_str())
        .collect();
    for outcome in ["parry", "dodge", "hurt"] {
        assert!(
            ordinary.contains(&outcome),
            "нет исхода {outcome}: {ordinary:?}"
        );
    }
}

#[test]
fn a_jump_in_the_middle_of_each_special_windup_clears_every_sweep() {
    let mut fight = Fight::start(BY_THE_FIRST_ROBBER);
    let robber = fight.robber(1.0);
    let mut jumped_at = None;
    let (blows, missed) = watch_blows(&mut fight, 30 * 60, |fight, tick| {
        let windup = fight.text(robber, "mood") == "windup" && fight.flag(robber, "special");
        if windup && fight.number(robber, "act_time") >= 0.4 && jumped_at.is_none() {
            jumped_at = Some(tick);
            return (vec!["Space"], vec![]);
        }
        if jumped_at.is_some_and(|at| tick == at + 20) {
            jumped_at = None;
            return (vec![], vec!["Space"]);
        }
        (vec![], vec![])
    });
    assert!(
        missed >= 3,
        "особых ударов мимо: {missed}, удары: {blows:?}"
    );
    assert!(
        blows.iter().all(|b| !b.special),
        "особый удар попал: {blows:?}"
    );
}

/// Шагает, пока у разбойника не настанет `mood`, не дольше 20 секунд.
fn until_mood(fight: &mut Fight, robber: u32, mood: &str, special: Option<bool>) {
    for _ in 0..20 * 60 {
        if fight.text(robber, "mood") == mood
            && special.is_none_or(|s| fight.flag(robber, "special") == s)
        {
            return;
        }
        fight.step(&[], &[]);
    }
    panic!("разбойник так и не дошёл до {mood}");
}

/// Что стало с разбойником за 0,7 секунды после нажатия удара.
struct AfterChop {
    moods: Vec<String>,
    flashed: bool,
    /// Самый дальний отлёт от Ильи, в клетках.
    pushed: f64,
}

fn after_chop(fight: &mut Fight, robber: u32) -> AfterChop {
    let start = fight.center_x(robber);
    let away = (start - fight.center_x(fight.hero())).signum();
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut after = AfterChop {
        moods: Vec::new(),
        flashed: false,
        pushed: 0.0,
    };
    for _ in 0..42 {
        fight.step(&[], &[]);
        after.moods.push(fight.text(robber, "mood"));
        after.flashed |= fight.number(robber, "flash_time") > 0.0;
        after.pushed = after.pushed.max((fight.center_x(robber) - start) * away);
    }
    after
}

#[test]
fn a_chop_staggers_a_resting_robber_but_only_flashes_one_winding_up() {
    let mut fight = Fight::start(BY_THE_FIRST_ROBBER);
    let robber = fight.robber(1.0);
    until_mood(&mut fight, robber, "windup", Some(false));
    let after = after_chop(&mut fight, robber);
    assert!(after.flashed, "по разбойнику в замахе нет вспышки");
    assert!(
        !after.moods.iter().any(|m| m == "stagger"),
        "замах сорвался: {:?}",
        after.moods
    );
    assert!(
        after.moods.iter().any(|m| m == "strike"),
        "удар не пошёл: {:?}",
        after.moods
    );

    until_mood(&mut fight, robber, "rest", None);
    let after = after_chop(&mut fight, robber);
    assert!(after.flashed, "по разбойнику в передышке нет вспышки");
    assert!(
        after.moods.iter().any(|m| m == "stagger"),
        "не отшатнулся: {:?}",
        after.moods
    );
    assert!(after.pushed > 0.25, "отлетел на {} клетки", after.pushed);
}

#[test]
fn every_hero_staggers_a_resting_robber_with_his_own_weapon() {
    for hero_id in ["ilya", "alyosha", "vasilisa"] {
        let mut fight = Fight::start_as(BY_THE_FIRST_ROBBER, hero_id);
        let robber = fight.robber(1.0);
        until_mood(&mut fight, robber, "rest", None);
        let after = after_chop(&mut fight, robber);
        assert!(
            after.moods.iter().any(|m| m == "stagger"),
            "{hero_id}: не отшатнулся: {:?}",
            after.moods
        );
    }
}

#[test]
fn a_defence_cuts_a_chop_and_the_chop_follows_unstopped() {
    let mut fight = Fight::start(BY_THE_FIRST_ROBBER);
    let robber = fight.robber(1.0);
    let hero = fight.hero();
    until_mood(&mut fight, robber, "windup", Some(false));
    while fight.number(robber, "act_time") < 0.45 - 0.1 {
        fight.step(&[], &[]);
    }
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut acts: Vec<(String, bool)> = Vec::new();
    for _ in 0..90 {
        fight.step(&[], &[]);
        let now = (fight.text(hero, "act"), fight.flag(hero, "unstoppable"));
        if acts.last() != Some(&now) {
            acts.push(now);
        }
    }
    assert!(acts.len() >= 4, "{acts:?}");
    let names: Vec<&str> = acts.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(names.first(), Some(&"chop"), "{acts:?}");
    assert!(
        ["parry", "dodge", "hurt"].contains(&names[1]),
        "защита не оборвала замах: {acts:?}"
    );
    assert_eq!(acts[2], ("chop".to_string(), true), "{acts:?}");
    assert_eq!(
        names[3], "",
        "удар после защиты не дошёл до конца: {acts:?}"
    );
}

#[test]
fn a_robber_does_not_leave_his_place_chasing_ilya() {
    // Илья на уступе за краем места первого разбойника: место кончается на 21, Илья — на 24.
    let mut fight = Fight::start([24.0, 5.9]);
    let robber = fight.robber(1.0);
    let mut furthest: f64 = 0.0;
    for _ in 0..20 * 60 {
        fight.step(&[], &[]);
        furthest = furthest.max(fight.center_x(robber));
    }
    assert!(furthest <= 21.1, "разбойник ушёл до {furthest}");
    assert!(furthest >= 20.9, "разбойник не подошёл к краю: {furthest}");
}

#[test]
fn the_rear_robber_of_a_pair_waits_behind_the_front_one() {
    // Илья слева от пары на последнем уступе: передний разбойник — 124, задний — 130.
    let mut fight = Fight::start([119.6, 6.3]);
    let front = fight.robber(3.0);
    let rear = fight.robber(4.0);
    let mut closest = f64::MAX;
    for _ in 0..25 * 60 {
        fight.step(&[], &[]);
        closest = closest.min((fight.center_x(rear) - fight.center_x(front)).abs());
    }
    assert!(closest >= 0.9, "задний подошёл к переднему на {closest}");
    assert!(closest <= 1.5, "задний так и не подошёл: {closest}");
}

/// Положения и кадры Ильи и разбойника.
fn pose_of(fight: &Fight, hero: u32, robber: u32) -> ([f64; 2], f64, [f64; 2], f64) {
    (
        fight.game.world.vec2(hero, property::POSITION).unwrap(),
        fight.number(hero, "hero_frame"),
        fight.game.world.vec2(robber, property::POSITION).unwrap(),
        fight.number(robber, "robber_frame"),
    )
}

/// С шага попадания: три шага оба стоят и кадры не меняются, а не позже чем через два шага после
/// остановки разбойник уже отлетает.
fn frozen_for_three_steps(fight: &mut Fight, hero: u32, robber: u32) {
    let at_hit = pose_of(fight, hero, robber);
    for after in 1..=3 {
        fight.step(&[], &[]);
        assert_eq!(
            pose_of(fight, hero, robber),
            at_hit,
            "шаг {after} после попадания"
        );
    }
    assert!(!fight.flag(hero, "held") && !fight.flag(robber, "held"));
    fight.step(&[], &[]);
    fight.step(&[], &[]);
    assert_ne!(
        pose_of(fight, hero, robber).2,
        at_hit.2,
        "разбойник не отлетел за два шага после остановки"
    );
}

#[test]
fn both_fighters_stand_still_for_three_steps_after_a_chop_lands() {
    let mut fight = Fight::start(BY_THE_FIRST_ROBBER);
    let robber = fight.robber(1.0);
    let hero = fight.hero();
    until_mood(&mut fight, robber, "rest", None);
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut steps = 0;
    while fight.number(robber, "flash_time") <= 0.0 {
        fight.step(&[], &[]);
        steps += 1;
        assert!(steps < 60, "удар Ильи так и не попал");
    }
    frozen_for_three_steps(&mut fight, hero, robber);
}

#[test]
fn both_fighters_stand_still_for_three_steps_after_a_parry() {
    let mut fight = Fight::start(BY_THE_FIRST_ROBBER);
    let robber = fight.robber(1.0);
    let hero = fight.hero();
    let mut steps = 0;
    while !(fight.text(hero, "act") == "parry" && fight.flag(robber, "held")) {
        fight.step(&[], &[]);
        steps += 1;
        assert!(steps < 30 * 60, "за 30 секунд ни одного парирования");
    }
    frozen_for_three_steps(&mut fight, hero, robber);
}

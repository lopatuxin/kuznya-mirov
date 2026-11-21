//! Фазы 06, 11 и 13 платформера — бой на настоящей игре `games/platformer`: герой рубит оружием из таблицы,
//! сам парирует, уворачивается и пропускает удары разбойника; каждый третий удар разбойника особый, его
//! защита не останавливает, а прыжок спасает. Разбойник теряет здоровье по урону героя и падает, герой
//! теряет здоровье, погибает и встаёт в начале уровня; Василису первые удары принимают чары, а бьёт она сгустком,
//! после которого раненый издали разбойник делает рывок. Фаза 66
//! Кузни: над разбойником шкала убывает долей и оставляет след.

use std::fs;

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::property;
use engine::core::scene::LayerView;
use engine::core::screens::{self, ButtonCommand, ScreenState, ScreensConfig};
use engine::core::trail::{TrailOwner, Trails};
use engine::core::world_elements::{BarDraw, add_trails, compute_world_draws};
use engine::data::load::{load_rest_with_stamps, read_entry};
use engine::data::session::PlaySession;

#[path = "platformer_support/mod.rs"]
mod support;
use support::{platformer_dir, read};

const VIEWPORT: [f32; 2] = [800.0, 600.0];
const HERO_START: &str = r#""name": "Герой", "position": [3, 7.1]"#;
/// Здоровье героя, которого бой не убьёт: тест смотрит на ход боя, а не на гибель.
const HARDY: f64 = 1.0e6;

/// Партия первого уровня, где Илья поставлен в точку `hero_at` (левый верхний угол его прямоугольника).
struct Fight {
    game: Game,
    config: ScreensConfig,
    state: ScreenState,
    session: PlaySession,
    trails: Trails,
    image_names: Vec<String>,
}

impl Fight {
    fn start(hero_at: [f64; 2]) -> Self {
        Fight::start_as(hero_at, "ilya")
    }

    /// То же за героя `hero_id` из таблицы героев.
    fn start_as(hero_at: [f64; 2], hero_id: &str) -> Self {
        Fight::start_with(hero_at, hero_id, |_, text| text)
    }

    /// То же, а `edit` правит текст каждой таблицы данных по её имени.
    fn start_with(hero_at: [f64; 2], hero_id: &str, edit: impl Fn(&str, String) -> String) -> Self {
        let game_json = read("game.json");
        let (config, _) = read_entry(&game_json).expect("game.json разобран");
        let images = support::image_verdicts(&config.files.images);
        let image_names = config
            .files
            .images
            .iter()
            .map(|decl| decl.name.clone())
            .collect();
        let tables: Vec<(String, Option<String>)> = support::table_texts(&config.files.tables)
            .into_iter()
            .map(|(name, text)| {
                let text = text.map(|text| edit(&name, text));
                (name, text)
            })
            .collect();
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
        let scene = scene.replace(
            r#""hero_id": "ilya""#,
            &format!(r#""hero_id": "{hero_id}""#),
        );
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
        let mut fight = Fight {
            game,
            config,
            state,
            session,
            trails: Trails::default(),
            image_names,
        };
        fight.restart();
        fight
    }

    /// Партия заново по команде экрана, как кнопка «Начать заново».
    fn restart(&mut self) {
        let level = self
            .config
            .screens
            .iter()
            .position(|screen| screen.name == "game")
            .expect("экран game");
        screens::apply_command(
            ButtonCommand::NewGame(level, None),
            &mut self.game,
            &self.config,
            &mut self.state,
        );
    }

    /// Партия с первым шагом: числа героя и разбойников уже взяты из таблиц.
    fn ready(hero_at: [f64; 2], hero_id: &str) -> Self {
        let mut fight = Fight::start_as(hero_at, hero_id);
        fight.step(&[], &[]);
        fight
    }

    /// Партия, где у героя не выпадают ни защиты, ни удар в слабое место: исход боя не зависит от случая.
    fn plain(hero_at: [f64; 2], hero_id: &str) -> Self {
        let mut fight = Fight::ready(hero_at, hero_id);
        let hero = fight.hero();
        for chance in ["parry_chance", "dodge_chance", "crit_chance"] {
            fight.set(hero, chance, 0.0);
        }
        fight
    }

    /// Бой героя не убивает.
    fn make_hardy(&mut self) {
        let hero = self.hero();
        self.set(hero, "health", HARDY);
        self.set(hero, "max_health", HARDY);
    }

    fn set(&mut self, id: u32, name: &str, value: f64) {
        let prop = self.prop(name);
        self.game.world.set_number(id, prop, value);
    }

    /// Полоски над объектами в этот миг, как их считает страница; память следов помнит прошлые вызовы.
    fn bars(&mut self) -> Vec<BarDraw> {
        let (mut bars, _, _) = compute_world_draws(
            &self.game.world,
            &self.game.scene,
            &self.game.properties,
            &self.config.world_elements,
            &LayerView::default(),
        );
        self.trails.follow_world(self.game.world_epoch());
        let mut pass = self.trails.pass(TrailOwner::World, self.game.step_count());
        add_trails(
            &mut bars,
            &self.config.world_elements,
            &self.game.world,
            &mut pass,
        );
        bars
    }

    /// Полоска здоровья над объектом `id`, если она нарисована.
    fn bar_over(&mut self, id: u32) -> Option<BarDraw> {
        self.bars().into_iter().find(|bar| bar.object == id)
    }

    /// Заставляет шкалу над разбойником `id` рисоваться, как у того, кто в бою.
    fn show_bar(&mut self, id: u32) {
        let show_bar = self.prop("show_bar");
        self.game.world.set_flag(id, show_bar, true);
    }

    /// Вспышки боя вида `kind`: объекты с признаком `fx`, названные так же, с картинкой того же имени.
    fn flashes(&self, kind: &str) -> Vec<u32> {
        let fx = self.prop("fx");
        self.game
            .world
            .ids()
            .filter(|&id| {
                self.game.world.has(id, fx)
                    && self.game.world.text(id, property::NAME) == Some(kind)
                    && self
                        .game
                        .world
                        .image(id, property::IMAGE)
                        .is_some_and(|image| self.image_names[image] == kind)
            })
            .collect()
    }

    /// Один шаг мира; `down` и `up` — клавиши, нажатые и отпущенные перед ним.
    fn step(&mut self, down: &[&str], up: &[&str]) {
        self.step_quiet(down, up);
        if let Some(error) = self.game.code_error() {
            panic!("ошибка кода игры: {}", error.message);
        }
    }

    /// Шаг без проверки ошибки кода: тест сам смотрит на `game.code_error()`.
    fn step_quiet(&mut self, down: &[&str], up: &[&str]) {
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

    /// Отсвет разбойника номер `no`.
    fn glow_of(&self, no: f64) -> u32 {
        let glow = self.prop("robber_glow");
        let robber_no = self.prop("robber_no");
        self.game
            .world
            .ids()
            .find(|&id| {
                self.game.world.has(id, glow)
                    && self.game.world.number_like(id, robber_no) == Some(no)
            })
            .expect("нет отсвета")
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
    mut keys: impl FnMut(&mut Fight, usize) -> (Vec<&'static str>, Vec<&'static str>),
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
    let mut fight = Fight::ready(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
    let hero = fight.hero();
    // Илья стоит и не уходит от разбойника: отлёт от пропущенных ударов он возвращает на место.
    let (blows, _) = watch_blows(&mut fight, 200 * 60, |fight, _| {
        if fight.center_x(hero) < 13.0 {
            fight
                .game
                .world
                .set_vec2(hero, property::POSITION, BY_THE_FIRST_ROBBER);
        }
        (vec![], vec![])
    });
    assert!(blows.len() >= 90, "ударов за 200 секунд: {}", blows.len());
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
    let mut fight = Fight::ready(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
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
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
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
    assert_eq!(fight.number(robber, "health"), 60.0);

    until_mood(&mut fight, robber, "rest", None);
    let after = after_chop(&mut fight, robber);
    assert!(after.flashed, "по разбойнику в передышке нет вспышки");
    assert!(
        after.moods.iter().any(|m| m == "stagger"),
        "не отшатнулся: {:?}",
        after.moods
    );
    assert!(after.pushed > 0.25, "отлетел на {} клетки", after.pushed);
    assert_eq!(
        fight.number(robber, "health"),
        46.0,
        "топор Ильи — 14 урона"
    );
}

#[test]
fn the_health_bar_appears_in_combat_and_hides_when_the_robber_patrols() {
    let start = [3.0, 7.1];
    let mut fight = Fight::start(start);
    let hero = fight.hero();
    let robber = fight.robber(1.0);

    fight.step(&[], &[]);
    assert!(!fight.flag(robber, "show_bar"));

    fight
        .game
        .world
        .set_vec2(hero, property::POSITION, BY_THE_FIRST_ROBBER);
    fight.step(&[], &[]);
    assert!(fight.flag(robber, "show_bar"));
    assert!(
        fight.bar_over(robber).is_some(),
        "шкала над разбойником в бою нарисована"
    );

    fight.game.world.set_vec2(hero, property::POSITION, start);
    until_mood(&mut fight, robber, "patrol", None);
    assert!(!fight.flag(robber, "show_bar"));
    assert!(fight.bar_over(robber).is_none(), "в обходе шкалы нет");
}

/// Сколько попаданий с отдачей нужно герою, чтобы убить первого разбойника: каждое — в передышке.
/// `from_behind` — разбойник перед каждым ударом смотрит от героя.
fn chops_to_kill(fight: &mut Fight, from_behind: bool) -> usize {
    let robber = fight.robber(1.0);
    for chops in 1..=20 {
        until_mood(fight, robber, "rest", None);
        if from_behind {
            fight.game.world.set_flag(robber, property::FLIP_X, false);
        }
        after_chop(fight, robber);
        if fight.number(robber, "health") == 0.0 {
            return chops;
        }
    }
    panic!("разбойник не упал за 20 попаданий");
}

#[test]
fn a_robber_falls_from_the_hits_his_hero_needs_and_then_fades_away_with_his_glow() {
    for (hero_id, hits) in [("ilya", 5), ("alyosha", 9), ("vasilisa", 6)] {
        let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, hero_id);
        fight.make_hardy();
        assert_eq!(chops_to_kill(&mut fight, false), hits, "{hero_id}");
    }

    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
    let robber = fight.robber(1.0);
    for _ in 0..4 {
        until_mood(&mut fight, robber, "rest", None);
        after_chop(&mut fight, robber);
    }
    assert_eq!(fight.number(robber, "health"), 4.0);
    until_mood(&mut fight, robber, "rest", None);
    fight.step(&["KeyJ"], &["KeyJ"]);
    while fight.number(robber, "health") > 0.0 {
        fight.step(&[], &[]);
    }
    assert_eq!(fight.text(robber, "mood"), "dying");
    assert!(!fight.flag(robber, "dead"), "гаснет, а не пропал сразу");
    for _ in 0..11 {
        fight.step(&[], &[]);
    }
    let fading = fight.number(robber, "opacity");
    assert!(fading > 0.0 && fading < 1.0, "гаснет: {fading}");
    for _ in 0..24 {
        fight.step(&[], &[]);
    }
    assert!(fight.flag(robber, "dead"));
    assert_eq!(fight.number(robber, "opacity"), 0.0);
    assert!(
        fight.bar_over(robber).is_none(),
        "шкала пропала вместе с ним"
    );
    let glow = fight.glow_of(1.0);
    assert!(fight.flag(glow, "dead"), "отсвет убитого вышел из правил");
    assert_eq!(fight.number(glow, "opacity"), 0.0);
}

#[test]
fn alyosha_strikes_double_from_behind_and_at_a_recoiling_robber() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "alyosha");
    fight.make_hardy();
    assert_eq!(chops_to_kill(&mut fight, true), 5, "пять двойных в спину");

    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "alyosha");
    fight.make_hardy();
    let robber = fight.robber(1.0);
    let mood = fight.prop("mood");
    fight
        .game
        .world
        .set_text(robber, mood, "recoil".to_string());
    fight.step(&["KeyJ"], &["KeyJ"]);
    for _ in 0..12 {
        fight.step(&[], &[]);
    }
    assert_eq!(
        fight.number(robber, "health"),
        46.0,
        "7 × 2 по отшатнувшемуся"
    );

    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
    let robber = fight.robber(1.0);
    until_mood(&mut fight, robber, "rest", None);
    fight.game.world.set_flag(robber, property::FLIP_X, false);
    after_chop(&mut fight, robber);
    assert_eq!(
        fight.number(robber, "health"),
        46.0,
        "у Ильи спина не удваивает"
    );
}

#[test]
fn a_hit_that_always_finds_the_weak_spot_doubles_the_damage_without_stacking_on_alyosha() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
    let hero = fight.hero();
    fight.set(hero, "crit_chance", 1.0);
    let robber = fight.robber(1.0);
    until_mood(&mut fight, robber, "rest", None);
    after_chop(&mut fight, robber);
    assert_eq!(fight.number(robber, "health"), 32.0, "14 × 2");

    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "alyosha");
    fight.make_hardy();
    let hero = fight.hero();
    fight.set(hero, "crit_chance", 1.0);
    let robber = fight.robber(1.0);
    until_mood(&mut fight, robber, "rest", None);
    fight.game.world.set_flag(robber, property::FLIP_X, false);
    after_chop(&mut fight, robber);
    assert_eq!(fight.number(robber, "health"), 46.0, "не больше двойного");
}

/// Что случилось с героем за один удар разбойника, дошедший до него.
struct BlowTaken {
    special: bool,
    health_lost: f64,
    spirit_lost: f64,
}

fn next_blow(fight: &mut Fight, robber: u32) -> BlowTaken {
    let hero = fight.hero();
    let mut was_done = fight.flag(robber, "blow_done");
    for _ in 0..30 * 60 {
        let (health, spirit) = (fight.number(hero, "health"), fight.number(hero, "spirit"));
        fight.step(&[], &[]);
        let done = fight.flag(robber, "blow_done");
        if done && !was_done {
            return BlowTaken {
                special: fight.flag(robber, "special"),
                health_lost: health - fight.number(hero, "health"),
                spirit_lost: spirit - fight.number(hero, "spirit"),
            };
        }
        was_done = done;
    }
    panic!("удар разбойника не дошёл до героя");
}

#[test]
fn a_missed_ordinary_blow_takes_twelve_and_a_special_one_twenty_five() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    let robber = fight.robber(1.0);
    for (special, damage) in [(false, 12.0), (false, 12.0), (true, 25.0)] {
        let blow = next_blow(&mut fight, robber);
        assert_eq!(blow.special, special);
        assert_eq!(blow.health_lost, damage);
        assert_eq!(blow.spirit_lost, 0.0, "у Ильи чар нет");
    }
}

#[test]
fn vasilisas_ward_takes_ordinary_blows_and_not_the_special_one_and_recharges() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "vasilisa");
    let robber = fight.robber(1.0);
    let hero = fight.hero();
    assert_eq!(fight.number(hero, "spirit"), 4.0);
    for _ in 0..2 {
        let blow = next_blow(&mut fight, robber);
        assert!(!blow.special);
        assert_eq!(blow.health_lost, 0.0, "чары приняли удар");
        assert_eq!(blow.spirit_lost, 1.0);
        assert_ne!(fight.text(hero, "act"), "hurt", "не отшатывается");
        assert_eq!(fight.flashes("ward").len(), 1, "вокруг неё вспыхнули чары");
    }
    let blow = next_blow(&mut fight, robber);
    assert!(blow.special);
    assert_eq!(blow.health_lost, 25.0, "особый удар чары не держат");
    assert_eq!(blow.spirit_lost, 0.0, "и заряд не тратит");

    let mut fight = Fight::plain([3.0, 7.1], "vasilisa");
    let hero = fight.hero();
    fight.set(hero, "spirit", 3.0);
    for _ in 0..168 {
        fight.step(&[], &[]);
    }
    assert_eq!(
        fight.number(hero, "spirit"),
        3.0,
        "через 2,8 с заряда ещё нет"
    );
    for _ in 0..6 {
        fight.step(&[], &[]);
    }
    assert_eq!(
        fight.number(hero, "spirit"),
        4.0,
        "через 2,9 с заряд вернулся"
    );
    for _ in 0..600 {
        fight.step(&[], &[]);
    }
    assert_eq!(fight.number(hero, "spirit"), 4.0, "запас не выше полного");
}

#[test]
fn a_ward_blow_does_not_cut_the_heros_own_chop() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "vasilisa");
    let robber = fight.robber(1.0);
    let hero = fight.hero();
    until_mood(&mut fight, robber, "windup", Some(false));
    while fight.number(robber, "act_time") < 0.45 - 0.2 {
        fight.step(&[], &[]);
    }
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut acts: Vec<String> = Vec::new();
    for _ in 0..60 {
        fight.step(&[], &[]);
        let act = fight.text(hero, "act");
        if acts.last() != Some(&act) {
            acts.push(act);
        }
    }
    assert!(
        acts.iter().all(|act| act == "chop" || act.is_empty()),
        "чары не прерывают удар: {acts:?}"
    );
    assert_eq!(fight.number(hero, "spirit"), 3.0);
}

#[test]
fn a_hero_with_no_health_stands_at_the_start_and_every_robber_stands_at_his_place_again() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    let hero = fight.hero();
    let first = fight.robber(1.0);
    let second = fight.robber(2.0);
    chops_to_kill(&mut fight, false);
    for _ in 0..40 {
        fight.step(&[], &[]);
    }
    assert!(fight.flag(first, "dead"));
    fight.set(second, "health", 20.0);
    fight
        .game
        .world
        .set_vec2(hero, property::POSITION, [71.6, 6.1]);
    fight.set(hero, "health", 5.0);
    for _ in 0..20 * 60 {
        fight.step(&[], &[]);
        if fight.number(hero, "health") > 100.0 {
            break;
        }
    }
    assert_eq!(fight.number(hero, "health"), 116.0, "герой так и не погиб");
    assert_eq!(
        fight.game.world.vec2(hero, property::POSITION),
        Some([3.0, 7.1]),
        "герой не в начале уровня"
    );
    for (robber, x, y) in [(first, 14.6, 7.1), (second, 73.6, 6.1)] {
        assert!(!fight.flag(robber, "dead"));
        assert_eq!(fight.number(robber, "health"), 60.0);
        assert_eq!(fight.number(robber, "opacity"), 1.0);
        assert_eq!(fight.text(robber, "mood"), "patrol");
        let at = fight.game.world.vec2(robber, property::POSITION).unwrap();
        assert!(
            (at[0] - x).abs() < 1e-6 && (at[1] - y).abs() < 0.05,
            "место {at:?}"
        );
    }
    let glow = fight.glow_of(1.0);
    assert!(!fight.flag(glow, "dead"), "отсвет вернулся в правила");
}

#[test]
fn vasilisa_stands_at_the_start_with_a_full_ward_after_dying() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "vasilisa");
    let hero = fight.hero();
    fight.set(hero, "spirit", 0.0);
    fight.set(hero, "health", 5.0);
    for _ in 0..20 * 60 {
        fight.step(&[], &[]);
        if fight.number(hero, "health") > 100.0 {
            break;
        }
    }
    assert_eq!(
        fight.number(hero, "health"),
        106.0,
        "Василиса так и не погибла"
    );
    assert_eq!(fight.number(hero, "spirit"), 4.0, "запас чар полный");
}

#[test]
fn falling_into_a_gorge_keeps_the_health_and_the_robbers_as_they_are() {
    let mut fight = Fight::plain([3.0, 7.1], "ilya");
    let hero = fight.hero();
    let robber = fight.robber(1.0);
    fight.set(hero, "health", 50.0);
    fight.set(robber, "health", 30.0);
    let pit = fight.prop("pit");
    let gorge = fight
        .game
        .world
        .ids()
        .find(|&id| fight.game.world.has(id, pit))
        .expect("дно ущелья");
    let at = fight.game.world.vec2(gorge, property::POSITION).unwrap();
    fight
        .game
        .world
        .set_vec2(hero, property::POSITION, [at[0] + 1.0, at[1] - 0.5]);
    fight.step(&[], &[]);
    assert_eq!(
        fight.game.world.vec2(hero, property::POSITION),
        Some([3.0, 7.1])
    );
    assert_eq!(fight.number(hero, "health"), 50.0);
    assert_eq!(fight.number(robber, "health"), 30.0);
}

#[test]
fn a_dead_robber_neither_strikes_nor_takes_a_hit() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    let hero = fight.hero();
    let robber = fight.robber(1.0);
    chops_to_kill(&mut fight, false);
    for _ in 0..40 {
        fight.step(&[], &[]);
    }
    assert!(fight.flag(robber, "dead"));
    let health = fight.number(hero, "health");
    let place = fight.game.world.vec2(robber, property::POSITION);
    let swings = fight.number(robber, "last_swing");
    fight.step(&["KeyJ"], &["KeyJ"]);
    for _ in 0..20 * 60 {
        fight.step(&[], &[]);
    }
    assert_eq!(
        fight.number(hero, "health"),
        health,
        "убитый разбойник не бьёт"
    );
    assert_eq!(fight.game.world.vec2(robber, property::POSITION), place);
    assert_eq!(
        fight.number(robber, "last_swing"),
        swings,
        "под удар не попал"
    );
    assert_eq!(fight.number(robber, "flash_time"), 0.0);
}

#[test]
fn every_hero_takes_the_numbers_of_his_stats_and_weapon_in_the_first_step() {
    // Здоровье, урон, время удара, парирование, уклонение и дух — числа требований фазы 11; урон сгустка
    // Василисы — от Мудрости, фаза 13 платформера.
    let expected = [
        ("ilya", 116.0, 14.0, 0.43, 0.16, 0.06, 0.0),
        ("alyosha", 108.0, 7.0, 0.27, 0.24, 0.14, 0.0),
        ("vasilisa", 106.0, 10.0, 0.39, 0.16, 0.08, 4.0),
    ];
    for (hero_id, health, damage, swing, parry, dodge, spirit) in expected {
        let fight = Fight::ready([3.0, 7.1], hero_id);
        let hero = fight.hero();
        let close = |name: &str, want: f64, tolerance: f64| {
            let got = fight.number(hero, name);
            assert!(
                (got - want).abs() <= tolerance,
                "{hero_id} {name}: {got}, а нужно {want}"
            );
        };
        close("max_health", health, 0.0);
        close("health", health, 0.0);
        close("damage", damage, 0.0);
        close("swing_time", swing, 0.005);
        close("parry_chance", parry, 1e-9);
        close("dodge_chance", dodge, 1e-9);
        close("crit_chance", dodge, 1e-9);
        close("max_spirit", spirit, 0.0);
        close("spirit", spirit, 0.0);
    }
    let fight = Fight::ready([3.0, 7.1], "vasilisa");
    let hero = fight.hero();
    assert!((fight.number(hero, "ward_time") - 4.0 / 1.4).abs() < 1e-9);
    let robber = fight.robber(1.0);
    assert_eq!(fight.number(robber, "max_health"), 60.0);
    assert_eq!(fight.number(robber, "health"), 60.0);
}

#[test]
fn a_missing_weapon_or_enemy_row_is_a_code_error_naming_it() {
    let mut fight = Fight::start_with([3.0, 7.1], "ilya", |name, text| match name {
        "heroes" => text.replace(r#""weapon": "axe""#, r#""weapon": "sword""#),
        _ => text,
    });
    fight.step_quiet(&[], &[]);
    let error = fight.game.code_error().expect("ошибка кода");
    assert!(error.message.contains("sword"), "{}", error.message);

    let mut fight = Fight::start_with([3.0, 7.1], "ilya", |name, text| match name {
        "enemies" => text.replace(r#""robber": {"#, r#""bandit": {"#),
        _ => text,
    });
    fight.step_quiet(&[], &[]);
    let error = fight.game.code_error().expect("ошибка кода");
    assert!(error.message.contains("robber"), "{}", error.message);
}

#[test]
fn a_chop_shrinks_the_bar_over_the_robber_by_a_share_and_leaves_a_trail() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
    let robber = fight.robber(1.0);
    until_mood(&mut fight, robber, "rest", None);
    let full = fight.bar_over(robber).expect("в бою шкала нарисована");
    assert_eq!(full.share, 1.0);
    assert_eq!(full.trail, None);

    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut hit = None;
    for _ in 0..30 {
        fight.step(&[], &[]);
        if fight.number(robber, "health") < 60.0 {
            hit = fight.bar_over(robber);
            break;
        }
    }
    let hit = hit.expect("удар не попал");
    assert!((hit.share - 46.0 / 60.0).abs() < 1e-6, "доля {}", hit.share);
    let trail = hit.trail.expect("на месте потерянного куска след");
    assert!(
        (trail.size[0] - hit.width).abs() < 1e-4,
        "след тянется до прежней доли"
    );
    for _ in 0..40 {
        fight.step(&[], &[]);
    }
    let later = fight.bar_over(robber).expect("шкала всё ещё в бою");
    assert!((later.share - 46.0 / 60.0).abs() < 1e-6);
    assert!(later.trail.is_none(), "через 0,6 с следа нет");
}

#[test]
fn a_bar_shown_in_the_step_of_the_hit_shows_the_trail_of_that_hit() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    fight.make_hardy();
    let robber = fight.robber(1.0);
    let show_bar = fight.prop("show_bar");
    until_mood(&mut fight, robber, "rest", None);

    let mut hit = None;
    for i in 0..30 {
        fight.game.world.set_flag(robber, show_bar, false);
        assert!(fight.bar_over(robber).is_none(), "до удара шкала спрятана");
        let keys: &[&str] = if i == 0 { &["KeyJ"] } else { &[] };
        fight.step(keys, keys);
        if fight.number(robber, "health") < 60.0 {
            hit = Some(
                fight
                    .bar_over(robber)
                    .expect("код показал шкалу в шаг удара"),
            );
            break;
        }
    }
    let hit = hit.expect("удар не попал");
    assert!((hit.share - 46.0 / 60.0).abs() < 1e-6, "доля {}", hit.share);
    let trail = hit.trail.expect("след удара виден сразу");
    assert!(
        (trail.size[0] - hit.width).abs() < 1e-4,
        "след до полной доли"
    );
}

#[test]
fn a_new_game_from_a_screen_starts_without_trails_however_short_the_old_one_was() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "ilya");
    let robber = fight.robber(1.0);
    fight.show_bar(robber);
    let full = fight.bar_over(robber).expect("шкала нарисована");
    assert_eq!(full.share, 1.0);
    let steps = fight.game.step_count();

    fight.restart();
    while fight.game.step_count() < steps {
        fight.step(&[], &[]);
    }
    fight.set(robber, "health", 30.0);
    fight.show_bar(robber);
    let fresh = fight.bar_over(robber).expect("шкала нарисована");
    assert_eq!(fresh.share, 0.5);
    assert_eq!(fresh.trail, None, "доля прошлой партии следа не даёт");
}

#[test]
fn every_hero_with_a_blade_staggers_a_resting_robber_with_his_own_weapon() {
    for hero_id in ["ilya", "alyosha"] {
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

/// Сгустки в мире: объекты с признаком `bolt`.
fn bolts(fight: &Fight) -> Vec<u32> {
    let bolt = fight.prop("bolt");
    fight
        .game
        .world
        .ids()
        .filter(|&id| fight.game.world.has(id, bolt))
        .collect()
}

#[test]
fn vasilisas_bolt_flies_three_cells_from_the_staff_and_goes_out() {
    // На старте разбойник её не видит: сгусток летит, ни во что не попадая.
    let mut fight = Fight::plain([3.0, 7.1], "vasilisa");
    let hero = fight.hero();
    let from = fight.center_x(hero);
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut first = None;
    let mut furthest: f64 = 0.0;
    let mut steps = 0;
    while steps < 120 {
        fight.step(&[], &[]);
        steps += 1;
        let Some(&bolt) = bolts(&fight).first() else {
            if first.is_some() {
                break;
            }
            continue;
        };
        let middle = fight.center_x(bolt);
        first.get_or_insert(middle);
        furthest = furthest.max(middle);
    }
    let first = first.expect("сгусток не вылетел") - from;
    assert!(
        (first - 1.45).abs() < 1e-9,
        "сгусток вылетел в {first} клетки от середины"
    );
    let flew = furthest - from - 1.45;
    assert!((flew - 3.0).abs() < 0.05, "сгусток пролетел {flew} клетки");
    assert!(bolts(&fight).is_empty(), "сгусток не погас");
    assert!(steps < 40, "сгусток летел {steps} шагов");
}

#[test]
fn a_bolt_wounds_a_resting_robber_by_ten_without_pushing_him() {
    let mut fight = Fight::plain(BY_THE_FIRST_ROBBER, "vasilisa");
    fight.make_hardy();
    let robber = fight.robber(1.0);
    until_mood(&mut fight, robber, "rest", None);
    let after = after_chop(&mut fight, robber);
    assert!(after.flashed, "по разбойнику нет вспышки");
    assert!(
        !after.moods.iter().any(|m| m == "stagger" || m == "dash"),
        "сгусток вблизи заставил отшатнуться или рвануться: {:?}",
        after.moods
    );
    assert!(
        after.pushed < 0.05,
        "сгусток отбросил на {} клетки",
        after.pushed
    );
    assert_eq!(
        fight.number(robber, "health"),
        50.0,
        "сгусток Василисы — 10 урона"
    );
}

#[test]
fn a_robber_wounded_from_afar_dashes_to_vasilisa_and_can_strike_at_once() {
    // Середина Василисы в 3,5 клетки левее середины первого разбойника: он её видит и идёт к ней.
    let mut fight = Fight::plain([11.1, 7.1], "vasilisa");
    fight.make_hardy();
    let hero = fight.hero();
    let robber = fight.robber(1.0);
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut gap_at_hit = None;
    let mut moods = Vec::new();
    for _ in 0..60 {
        fight.step(&[], &[]);
        let mood = fight.text(robber, "mood");
        if gap_at_hit.is_none() && fight.number(robber, "health") < 60.0 {
            gap_at_hit = Some(fight.center_x(robber) - fight.center_x(hero));
        }
        if moods.last() != Some(&mood) {
            moods.push(mood);
        }
        if moods.last().is_some_and(|m| m == "windup") {
            break;
        }
    }
    let gap = gap_at_hit.expect("сгусток не ранил разбойника");
    assert!(gap > 1.2, "ранен вблизи, в {gap} клетки");
    let dash = moods
        .iter()
        .position(|m| m == "dash")
        .expect("рывка не было");
    assert_eq!(
        moods[dash + 1..]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["chase", "windup"],
        "после рывка — погоня и сразу замах: {moods:?}"
    );
    let near = fight.center_x(robber) - fight.center_x(hero);
    assert!((0.9..=1.2).contains(&near), "замахнулся с {near} клетки");
}

#[test]
fn a_robber_who_cannot_reach_vasilisa_deflects_her_bolt() {
    // Место первого разбойника кончается на 15, Василиса за краем, в 2,7 клетки от него, лицом к нему.
    let mut fight = Fight::plain([17.3, 7.1], "vasilisa");
    fight.make_hardy();
    let hero = fight.hero();
    let robber = fight.robber(1.0);
    fight.set(robber, "home_right", 15.0);
    until_mood(&mut fight, robber, "chase", None);
    for _ in 0..90 {
        fight.step(&[], &[]);
    }
    fight.game.world.set_flag(hero, property::FLIP_X, true);
    fight.step(&["KeyJ"], &["KeyJ"]);
    let mut clashed = false;
    for _ in 0..40 {
        fight.step(&[], &[]);
        clashed |= !fight.flashes("clash").is_empty();
        assert!(
            fight.text(robber, "mood") != "dash",
            "рванулся, хотя не дойдёт"
        );
    }
    assert!(clashed, "сгусток не отбит");
    assert_eq!(fight.number(robber, "health"), 60.0);
}

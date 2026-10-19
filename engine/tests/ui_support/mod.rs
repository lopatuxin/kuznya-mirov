//! Общее для проверок фаз 40 и 41: маленькая игра, собранная из текстов, и «страница» — очередь мыши, экран,
//! накопитель времени, — которой тесты водят игру теми же вызовами, что `wasm::Engine`.

#![allow(dead_code)]

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::runner::Runner;
use engine::core::screens::{self, ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{load_rest_with_tables, read_entry};

pub const VIEWPORT: [f32; 2] = [800.0, 600.0];

const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];

pub const NO_RULES: &str = r#"{"rules":[]}"#;

/// Тексты игры: `game.json` собирается по ним же (есть ли код, сколько объектов можно).
pub struct Files<'a> {
    pub properties: &'a str,
    pub scene: &'a str,
    pub rules: &'a str,
    pub screens: &'a str,
    pub code: Option<&'a str>,
    pub tables: &'a [(&'a str, &'a str)],
    pub max_objects: usize,
}

impl<'a> Files<'a> {
    pub fn new(properties: &'a str, scene: &'a str, screens: &'a str) -> Self {
        Files {
            properties,
            scene,
            rules: NO_RULES,
            screens,
            code: None,
            tables: &[],
            max_objects: 100,
        }
    }

    pub fn with_code(mut self, code: &'a str) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_rules(mut self, rules: &'a str) -> Self {
        self.rules = rules;
        self
    }

    pub fn with_tables(mut self, tables: &'a [(&'a str, &'a str)]) -> Self {
        self.tables = tables;
        self
    }

    pub fn with_max_objects(mut self, max_objects: usize) -> Self {
        self.max_objects = max_objects;
        self
    }

    fn game_json(&self) -> String {
        let code = if self.code.is_some() {
            r#","code":"code.lua""#
        } else {
            ""
        };
        let tables = self
            .tables
            .iter()
            .map(|(name, _)| format!(r#""{name}":"{name}.json""#))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":{},
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"ui.ttf"}}{code},"tables":{{{tables}}}}}}}"##,
            self.max_objects
        )
    }

    /// Загрузка с ошибками и предупреждениями, как их видит страница.
    pub fn try_load(&self) -> Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure> {
        let game_json = self.game_json();
        let (config, _) = read_entry(&game_json)?;
        let fonts = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
        let tables: Vec<(String, Option<String>)> = self
            .tables
            .iter()
            .map(|(name, text)| ((*name).to_string(), Some((*text).to_string())))
            .collect();
        load_rest_with_tables(
            &game_json,
            config,
            Some(self.properties),
            Some(self.scene),
            Some(self.rules),
            Some(self.screens),
            &fonts,
            &[],
            &[],
            &[],
            self.code,
            false,
            &tables,
        )
        .map(|(game, screens, warnings, _images)| (game, screens, warnings))
    }

    pub fn load(&self) -> Ui {
        match self.try_load() {
            Ok((game, config, _warnings)) => Ui::new(game, config),
            Err(failure) => panic!("игра не загрузилась: {:#?}", failure.errors),
        }
    }
}

/// «Страница»: игра с экранами, мышью и временем.
pub struct Ui {
    pub game: Game,
    pub config: ScreensConfig,
    pub state: ScreenState,
    pub runner: Runner,
    pub mouse: MouseState,
    pub queue: UiQueue,
}

impl Ui {
    pub fn new(game: Game, config: ScreensConfig) -> Self {
        let state = ScreenState::new(config.start_screen);
        Ui {
            game,
            config,
            state,
            runner: Runner::new(),
            mouse: MouseState::default(),
            queue: UiQueue::new(),
        }
    }

    pub fn screen_id(&self, name: &str) -> usize {
        self.config
            .screens
            .iter()
            .position(|screen| screen.name == name)
            .unwrap_or_else(|| panic!("нет экрана {name}"))
    }

    pub fn show(&mut self, name: &str) {
        let target = self.screen_id(name);
        screens::apply_command(
            screens::ButtonCommand::ShowScreen(target),
            &mut self.game,
            &self.config,
            &mut self.state,
        );
    }

    /// Мышь пришла в точку, нажата и отпущена.
    pub fn click(&mut self, x: f32, y: f32) {
        self.press(x, y);
        self.release();
    }

    pub fn press(&mut self, x: f32, y: f32) {
        self.queue.push_mouse_move(x, y);
        self.queue.push_mouse_down();
        self.drain();
    }

    pub fn release(&mut self) {
        self.queue.push_mouse_up();
        self.drain();
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.queue.push_mouse_move(x, y);
        self.drain();
    }

    pub fn key(&mut self, code: &str) {
        self.queue.push_key_down(code);
        self.queue.push_key_up(code);
        self.drain();
    }

    pub fn key_down(&mut self, code: &str) {
        self.queue.push_key_down(code);
        self.drain();
    }

    fn drain(&mut self) {
        screens::process_ui_queue(
            &mut self.queue,
            &mut self.mouse,
            &mut self.game,
            &self.config,
            &mut self.state,
            VIEWPORT,
        );
    }

    /// Колесо в точке: прокрутка списка, если там список.
    pub fn wheel(&mut self, x: f32, y: f32, delta_y: f64) -> bool {
        screens::scroll_wheel(
            &self.config,
            &mut self.state,
            &self.game.world,
            VIEWPORT,
            [x, y],
            delta_y,
        )
    }

    /// Один вызов движка за `dt` секунд, как `requestAnimationFrame`.
    pub fn tick(&mut self, dt: f64) {
        screens::engine_call(
            &mut self.queue,
            &mut self.mouse,
            &mut self.runner,
            &mut self.game,
            &self.config,
            &mut self.state,
            VIEWPORT,
            dt,
        );
    }

    pub fn ticks(&mut self, calls: usize, dt: f64) {
        for _ in 0..calls {
            self.tick(dt);
        }
    }

    pub fn id_of(&self, name: &str) -> u32 {
        self.game
            .world
            .ids()
            .find(|&id| self.game.world.text(id, engine::core::property::NAME) == Some(name))
            .unwrap_or_else(|| panic!("нет объекта {name}"))
    }

    pub fn number(&self, id: u32, property: &str) -> Option<f64> {
        let prop = self.game.properties.resolve(property)?;
        self.game.world.number_like(id, prop)
    }

    pub fn flag(&self, id: u32, property: &str) -> bool {
        let prop = self.game.properties.resolve(property).expect("свойство");
        self.game.world.flag(id, prop)
    }

    pub fn text(&self, id: u32, property: &str) -> Option<String> {
        let prop = self.game.properties.resolve(property)?;
        self.game.world.text(id, prop).map(str::to_string)
    }

    /// Сколько живых объектов с признаком.
    pub fn count(&self, property: &str) -> usize {
        let prop = self.game.properties.resolve(property).expect("свойство");
        self.game
            .world
            .ids()
            .filter(|&id| self.game.world.has(id, prop))
            .count()
    }
}

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::core::game::Game;
use crate::core::property;
use crate::core::rules::Outcome;
use crate::core::runner::Runner;
use crate::data::error::GameError;
use crate::data::load::{self, GameConfig};
use crate::render::{DrawRect, Renderer};

fn set(obj: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(obj, &JsValue::from_str(key), value);
}

fn js_optional_usize(value: Option<usize>) -> JsValue {
    match value {
        Some(n) => JsValue::from_f64(n as f64),
        None => JsValue::NULL,
    }
}

fn js_error_array(items: &[GameError]) -> Array {
    let arr = Array::new();
    for e in items {
        let item = Object::new();
        set(&item, "file", &JsValue::from_str(&e.file));
        set(&item, "path", &JsValue::from_str(&e.path));
        set(&item, "message", &JsValue::from_str(&e.message));
        set(&item, "line", &js_optional_usize(e.line));
        set(&item, "column", &js_optional_usize(e.column));
        arr.push(&item);
    }
    arr
}

/// `load()`'s success shape: warnings are collected the same way errors are, but never stop the
/// game from starting — see «Формат игры» → «Проверка данных перед запуском».
fn js_load_ok(warnings: &[GameError]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::TRUE);
    set(&obj, "warnings", &js_error_array(warnings));
    obj.into()
}

/// `load()`'s failure shape: the errors that stopped the game, next to whatever warnings were
/// collected before the check gave up.
fn js_load_err(errors: &[GameError], warnings: &[GameError]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::FALSE);
    set(&obj, "errors", &js_error_array(errors));
    set(&obj, "warnings", &js_error_array(warnings));
    obj.into()
}

/// `read_entry()`'s success shape: `files` are the paths to fetch next, `warnings` collected the
/// same way `load()`'s are — see `js_load_ok`.
fn js_entry_ok(config: &GameConfig, warnings: &[GameError]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::TRUE);
    let files = Object::new();
    set(
        &files,
        "properties",
        &JsValue::from_str(&config.files.properties),
    );
    set(&files, "scene", &JsValue::from_str(&config.files.scene));
    set(&files, "rules", &JsValue::from_str(&config.files.rules));
    set(&obj, "files", &files);
    set(&obj, "warnings", &js_error_array(warnings));
    obj.into()
}

fn js_running() -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::TRUE);
    obj.into()
}

fn js_ended(outcome: Outcome, step: u64) -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::FALSE);
    let outcome_str = match outcome {
        Outcome::Win => "win",
        Outcome::Loss => "loss",
    };
    set(&obj, "outcome", &JsValue::from_str(outcome_str));
    set(&obj, "step", &JsValue::from_f64(step as f64));
    obj.into()
}

fn compose_instances(game: &Game) -> Vec<DrawRect> {
    let mut ordered: Vec<(i32, u32)> = game
        .world
        .ids()
        .filter(|&id| {
            game.world.vec2(id, property::POSITION).is_some()
                && game.world.vec2(id, property::SIZE).is_some()
                && game.world.color(id, property::COLOR).is_some()
        })
        .map(|id| (game.world.layer(id, property::LAYER).unwrap_or(0), id))
        .collect();
    ordered.sort_unstable();

    ordered
        .into_iter()
        .map(|(_, id)| {
            let p = game
                .world
                .vec2(id, property::POSITION)
                .expect("filtered above");
            let s = game.world.vec2(id, property::SIZE).expect("filtered above");
            let color = game
                .world
                .color(id, property::COLOR)
                .expect("filtered above");
            DrawRect {
                position: [p[0] as f32, p[1] as f32],
                size: [s[0] as f32, s[1] as f32],
                color,
            }
        })
        .collect()
}

/// The engine, one per canvas. See the crate's README / contract notes for the exact JS-side
/// call sequence: `create` once, then `read_entry` followed by `load`, then `key_down`/`key_up`
/// as events happen and `tick` once per `requestAnimationFrame`.
#[wasm_bindgen]
pub struct Engine {
    renderer: Renderer,
    game: Option<Game>,
    runner: Runner,
    pending_config: load::PendingConfig,
    last_tick_ms: Option<f64>,
}

#[wasm_bindgen]
impl Engine {
    /// Sets up the GPU on `canvas` (WebGPU, falling back to WebGL2). Rejects with a
    /// Russian-language `Error` if neither is available.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<Engine, JsValue> {
        console_error_panic_hook::set_once();
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let renderer = Renderer::new(canvas, width, height, 1, 1, [0.0, 0.0, 0.0, 1.0])
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Engine {
            renderer,
            game: None,
            runner: Runner::new(),
            pending_config: load::PendingConfig::default(),
            last_tick_ms: None,
        })
    }

    /// Which backend actually got used: `"webgpu"` or `"webgl2"`.
    pub fn backend(&self) -> String {
        self.renderer.backend().label().to_string()
    }

    /// Step one of loading a game: parses `game.json` only and reports which three files (paths
    /// relative to the game folder) to fetch next. Returns `{ok:true, files:{...}, warnings:[...]}`
    /// on success or `{ok:false, errors:[...], warnings:[...]}` on any prestart-check failure —
    /// same shape `load()` uses, warnings travel alongside either outcome.
    pub fn read_entry(&mut self, game_json: &str) -> JsValue {
        let result = load::read_entry(game_json);
        self.pending_config.set(&result);
        match result {
            Ok((config, warnings)) => js_entry_ok(&config, &warnings),
            Err(failure) => js_load_err(&failure.errors, &failure.warnings),
        }
    }

    /// Step two: hand over the three files `read_entry` named (`None`/`null` for one the page
    /// could not fetch). Runs the full prestart check and, on success, the game is ready to run
    /// starting from the next `tick`. Returns `{ok:true, warnings:[...]}` on success or
    /// `{ok:false, errors:[...], warnings:[...]}` on failure — warnings never stop the game from
    /// starting, but the page still needs to see them either way.
    pub fn load(
        &mut self,
        properties_json: Option<String>,
        scene_json: Option<String>,
        rules_json: Option<String>,
    ) -> JsValue {
        let Some(config) = self.pending_config.take() else {
            return js_load_err(
                &[GameError::new(
                    "engine",
                    "",
                    "load() вызван раньше успешного read_entry()",
                )],
                &[],
            );
        };
        match load::load_rest(
            config,
            properties_json.as_deref(),
            scene_json.as_deref(),
            rules_json.as_deref(),
        ) {
            Ok((game, warnings)) => {
                self.renderer
                    .set_scene(game.scene.width, game.scene.height, game.scene.background);
                self.game = Some(game);
                js_load_ok(&warnings)
            }
            Err(failure) => js_load_err(&failure.errors, &failure.warnings),
        }
    }

    pub fn key_down(&mut self, code: &str) {
        if let Some(game) = &mut self.game {
            game.key_down(code);
        }
    }

    pub fn key_up(&mut self, code: &str) {
        if let Some(game) = &mut self.game {
            game.key_up(code);
        }
    }

    /// Called once per `requestAnimationFrame` with `performance.now()`. Advances the fixed-step
    /// simulation (catch-up capped at five steps) and draws one frame.
    pub fn tick(&mut self, now_ms: f64) -> JsValue {
        let dt_seconds = match self.last_tick_ms {
            Some(prev) => ((now_ms - prev) / 1000.0).max(0.0),
            None => 0.0,
        };
        self.last_tick_ms = Some(now_ms);

        let Some(game) = &mut self.game else {
            let _ = self.renderer.render(&[]);
            return js_running();
        };

        self.runner.advance(game, dt_seconds);
        let instances = compose_instances(game);
        if let Err(e) = self.renderer.render(&instances) {
            web_sys::console::error_1(&JsValue::from_str(&format!("отрисовка не удалась: {e}")));
        }

        match game.outcome() {
            Some((outcome, step)) => js_ended(outcome, step),
            None => js_running(),
        }
    }

    /// Reconfigures the GPU surface. Call on canvas/container resize; does not touch the game.
    pub fn resize(&mut self, width_px: u32, height_px: u32) {
        self.renderer.resize(width_px, height_px);
    }

    /// Call on `visibilitychange` going to hidden: drops accumulated real time so a multi-minute
    /// gap is not caught up in a burst.
    pub fn tab_hidden(&mut self) {
        self.runner.reset();
    }

    /// Once-only runtime warnings collected so far (max_objects reached, random_cell exhausted).
    pub fn messages(&self) -> Array {
        let arr = Array::new();
        if let Some(game) = &self.game {
            for message in game.messages() {
                arr.push(&JsValue::from_str(message));
            }
        }
        arr
    }
}

//! «Редактор» → «Анимация», «Вызовы движка»: `show_motion`, `motion_pose` и вызовы вокруг них. Стенд живёт сам, его
//! мир просмотра не имеет отношения к загруженной игре; вызовы только переводят ответы `data::stand` в JS.

use js_sys::{Array, Object};
use wasm_bindgen::prelude::*;

use crate::core::camera::Camera3d;
use crate::core::model::ModelTable;
use crate::core::skeleton::turn_after;
use crate::data::stand::{Rings, Stand, StandView};
use crate::render::UiLayers;

use super::{Engine, View, js_point, render_world, set};

fn js_points(points: &[Option<[f64; 2]>]) -> Array {
    let list = Array::new();
    for point in points {
        list.push(&point.map_or(JsValue::NULL, js_point));
    }
    list
}

fn js_rings(rings: &Rings) -> JsValue {
    let obj = Object::new();
    set(
        &obj,
        "center",
        &rings.center.map_or(JsValue::NULL, js_point),
    );
    let list = Array::new();
    for ring in &rings.rings {
        list.push(&js_points(ring));
    }
    set(&obj, "rings", &list);
    obj.into()
}

fn js_stand_error(errors: &[String]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::FALSE);
    set(&obj, "error", &JsValue::from_str(&errors.join("\n")));
    obj.into()
}

/// «Редактор», «Вызовы движка», `motion_turn_after`: поправка кости после ещё одного поворота — `[x, y, z]` в
/// градусах после поворота на `degrees` вокруг её оси `axis` (0 — x, 1 — y, 2 — z); тем же порядком осей, что
/// и поправки `motions.json`. `y` приводится к пределам ±90°.
#[wasm_bindgen]
pub fn motion_turn_after(x: f64, y: f64, z: f64, axis: u32, degrees: f64) -> Array {
    let turn = turn_after(
        [x as f32, y as f32, z as f32],
        axis as usize,
        degrees as f32,
    );
    let list = Array::new();
    for angle in turn {
        list.push(&JsValue::from_f64(f64::from(angle)));
    }
    list
}

#[wasm_bindgen]
impl Engine {
    /// «Редактор», «Анимация»: собирает мир просмотра — одна модель с движением `name` из текста `motions_json` на
    /// пустой ровной земле (светло-серый пол шесть на шесть клеток с сеткой в полклетки) при свете игры — и делает его
    /// тем, что рисует `draw`. Камера ставит модель посередине окна на четыре пятых его высоты. `view`: `"side"`, `"front"` или
    /// `"game"` — с какой стороны смотрит камера, пока стенда нет; сменившийся стенд камеру не сбрасывает.
    /// Тот же разбор и те же проверки, что при загрузке игры, но только для этого `motions.json` и этой модели.
    /// Возвращает `{ok: true, bones: [{name, parent}], length, frames}`: все кости скелета в порядке файла
    /// (`parent` — номер кости или `-1`), длина движения в секундах с учётом `speed` и число кадров в нём. Не вышло —
    /// `{ok: false, error}`, `error` — текст, по строке на каждую ошибку; прежний стенд остаётся, и `draw`
    /// рисует его последнюю верную позу.
    pub fn show_motion(&mut self, motions_json: &str, name: &str, view: &str) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return js_stand_error(&["игра не загружена".to_string()]);
        };
        let Some(view) = StandView::parse(view) else {
            return js_stand_error(&[format!(
                "вид \"{view}\": ожидался \"side\", \"front\" или \"game\""
            )]);
        };
        let viewport = self.stand_viewport();
        match Stand::build(game, motions_json, name, view, viewport) {
            Ok(mut stand) => {
                if let Some(old) = self.stand.as_ref() {
                    stand.keep_camera(old);
                    self.stand_models_on_gpu &= old.model() == stand.model();
                }
                let obj = Object::new();
                set(&obj, "ok", &JsValue::TRUE);
                let bones = Array::new();
                for bone in &stand.bones {
                    let item = Object::new();
                    set(&item, "name", &JsValue::from_str(&bone.name));
                    let parent = bone.parent.map_or(-1.0, |parent| parent as f64);
                    set(&item, "parent", &JsValue::from_f64(parent));
                    bones.push(&item);
                }
                set(&obj, "bones", &bones);
                set(&obj, "length", &JsValue::from_f64(f64::from(stand.length)));
                set(&obj, "frames", &JsValue::from_f64(stand.frames as f64));
                self.stand = Some(stand);
                obj.into()
            }
            Err(errors) => js_stand_error(&errors),
        }
    }

    /// «Редактор», «Анимация»: ставит модель в долю движения `phase` (от 0 до 1) и отдаёт всё, что редактор рисует
    /// поверх: `{phase, points, slips, end_gap}`. `points` — сустав каждой кости на холсте в точках окна,
    /// `[x, y]` или `null` за камерой, в порядке `bones` из `show_motion`. `slips` — доли, где ступня стоит на земле и
    /// скользит: `[{foot, from, to}]`, `foot` — 0 или 1 по порядку `feet`. `end_gap` — `{degrees, bone}` у
    /// повторяющегося движения, чей конец расходится с началом больше чем на 5° (`bone` — имя кости), иначе `null`;
    /// `slips` и `end_gap` от доли не зависят. Без стенда — `null`.
    pub fn motion_pose(&mut self, phase: f64) -> JsValue {
        let viewport = self.stand_viewport();
        let Some(stand) = self.stand.as_mut() else {
            return JsValue::NULL;
        };
        stand.pose(phase as f32);
        let camera = stand.camera(viewport);
        let obj = Object::new();
        set(&obj, "phase", &JsValue::from_f64(phase.clamp(0.0, 1.0)));
        set(&obj, "points", &js_points(&stand.joints(&camera)));
        let slips = Array::new();
        for slip in &stand.analysis.slips {
            let item = Object::new();
            set(&item, "foot", &JsValue::from_f64(slip.foot as f64));
            set(&item, "from", &JsValue::from_f64(f64::from(slip.from)));
            set(&item, "to", &JsValue::from_f64(f64::from(slip.to)));
            slips.push(&item);
        }
        set(&obj, "slips", &slips);
        let end_gap = match stand.analysis.end_gap {
            Some((degrees, bone)) => {
                let item = Object::new();
                set(&item, "degrees", &JsValue::from_f64(f64::from(degrees)));
                let name = stand.bones.get(bone).map_or("", |bone| bone.name.as_str());
                set(&item, "bone", &JsValue::from_str(name));
                item.into()
            }
            None => JsValue::NULL,
        };
        set(&obj, "end_gap", &end_gap);
        obj.into()
    }

    /// «Редактор», «Анимация»: кость под точкой холста `(x, y)` (CSS-пикселях) — её номер из `bones` или
    /// `undefined`, если рядом с точкой нет кости.
    pub fn motion_bone_at(&self, x: f64, y: f64) -> JsValue {
        let Some((stand, camera)) = self.stand_camera() else {
            return JsValue::UNDEFINED;
        };
        stand
            .bone_at(&camera, [x, y])
            .map_or(JsValue::UNDEFINED, |bone| JsValue::from_f64(bone as f64))
    }

    /// «Редактор», «Анимация»: кольца поворота кости `bone` — `{center, rings}`: середина кости на холсте и три кольца
    /// (вокруг осей `x`, `y`, `z` кости) ломаными по 48 точек `[x, y]` (`null` за камерой). `undefined` без стенда
    /// и без такой кости.
    pub fn motion_rings(&self, bone: u32) -> JsValue {
        let Some((stand, camera)) = self.stand_camera() else {
            return JsValue::UNDEFINED;
        };
        stand
            .rings(&camera, bone as usize)
            .map_or(JsValue::UNDEFINED, |rings| js_rings(&rings))
    }

    /// «Редактор», «Анимация»: кольцо кости `bone` под точкой `(x, y)` — 0, 1 или 2 (ось `x`, `y` или `z`) — или
    /// `undefined`.
    pub fn motion_ring_at(&self, bone: u32, x: f64, y: f64) -> JsValue {
        let Some((stand, camera)) = self.stand_camera() else {
            return JsValue::UNDEFINED;
        };
        stand
            .ring_at(&camera, bone as usize, [x, y])
            .map_or(JsValue::UNDEFINED, |axis| JsValue::from_f64(axis as f64))
    }

    /// «Редактор», «Анимация»: хват кольца `axis` кости `bone` в точке `(x, y)`; `motion_ring_drag` считает угол от
    /// этого хвата, пока не вызван `motion_ring_release`.
    pub fn motion_ring_grab(&mut self, bone: u32, axis: u32, x: f64, y: f64) {
        let viewport = self.stand_viewport();
        if let Some(stand) = self.stand.as_mut() {
            let camera = stand.camera(viewport);
            stand.grab_ring(&camera, bone as usize, axis as usize, [x, y]);
        }
    }

    /// «Редактор», «Анимация»: на сколько градусов точка `(x, y)` ушла по схваченному кольцу от хвата; плюс — в сторону,
    /// куда растёт поворот кости вокруг этой оси, и прибавлять его надо к поправке, какой она была при хвате
    /// (`motion_turn_after`). `undefined` без хвата.
    pub fn motion_ring_drag(&mut self, x: f64, y: f64) -> JsValue {
        let viewport = self.stand_viewport();
        let Some(stand) = self.stand.as_mut() else {
            return JsValue::UNDEFINED;
        };
        let camera = stand.camera(viewport);
        stand
            .drag_ring(&camera, [x, y])
            .map_or(JsValue::UNDEFINED, JsValue::from_f64)
    }

    pub fn motion_ring_release(&mut self) {
        if let Some(stand) = self.stand.as_mut() {
            stand.release_ring();
        }
    }

    /// «Редактор», «Анимация»: ставит камеру на вид `"side"`, `"front"` или `"game"`.
    pub fn motion_view(&mut self, view: &str) {
        let viewport = self.stand_viewport();
        if let (Some(view), Some(stand)) = (StandView::parse(view), self.stand.as_mut()) {
            stand.set_view(view, viewport);
        }
    }

    /// «Редактор», «Анимация»: колесо мыши приближает и отдаляет модель.
    pub fn motion_zoom(&mut self, delta_y: f64) {
        if let (Some(stand), true) = (self.stand.as_mut(), delta_y.is_finite()) {
            stand.zoom(delta_y);
        }
    }

    /// «Редактор», «Анимация»: перетаскивание правой кнопкой вращает вид вокруг модели; `dx` и `dy` — сдвиг мыши
    /// в точках окна.
    pub fn motion_turn(&mut self, dx: f64, dy: f64) {
        if let (Some(stand), true) = (self.stand.as_mut(), dx.is_finite() && dy.is_finite()) {
            stand.turn(dx, dy);
        }
    }

    /// «Редактор», «Анимация»: закрывает стенд — `draw` снова рисует загруженную игру.
    pub fn hide_motion(&mut self) {
        self.stand = None;
        if std::mem::take(&mut self.stand_models_on_gpu) {
            match self.game.as_ref() {
                Some(game) => self.renderer.set_models(game.properties.models()),
                None => self.renderer.set_models(&ModelTable::default()),
            };
        }
    }
}

impl Engine {
    fn stand_viewport(&self) -> [f64; 2] {
        let viewport = self.renderer.window_size_css();
        [f64::from(viewport[0]), f64::from(viewport[1])]
    }

    fn stand_camera(&self) -> Option<(&Stand, Camera3d)> {
        let stand = self.stand.as_ref()?;
        Some((stand, stand.camera(self.stand_viewport())))
    }

    /// Кадр стенда вместо кадра игры.
    pub(super) fn draw_stand(&mut self) {
        let Some(stand) = self.stand.as_ref() else {
            return;
        };
        // Модели стенда лежат на видеокарте, пока он есть: так просмотр переживает отказ загрузки игры, а скелет
        // модели в нём — с поправками движения.
        if !self.stand_models_on_gpu {
            self.renderer.set_models(stand.game().properties.models());
            self.stand_models_on_gpu = true;
        }
        let viewport = self.renderer.window_size_css();
        let view = View::Space(stand.camera(self.stand_viewport()));
        if let Err(e) = render_world(
            &mut self.renderer,
            stand.game(),
            &self.stand_motion,
            None,
            &[],
            &[],
            None,
            &mut self.stand_shapes,
            &view,
            viewport,
            &UiLayers::default(),
        ) {
            web_sys::console::error_1(&JsValue::from_str(&format!("отрисовка не удалась: {e}")));
        }
    }
}

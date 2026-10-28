//! «Редактор» → «Анимация»: стенд анимации. Одна модель со скелетом и одно её движение `motions.json` на пустой
//! ровной земле при свете игры. Ни видеокарты, ни браузера: стенд собирает мир просмотра, ставит позу в долю
//! движения, говорит, где на холсте суставы, какая кость под точкой и где кольца поворота, и считает, где ступни
//! скользят и сходится ли конец движения с началом; рисует мир `wasm::Engine`.

use crate::core::camera::{self, Camera3d, EDITOR_PITCH_RANGE, EditorCamera};
use crate::core::code::ObjectParser;
use crate::core::game::{self, Game};
use crate::core::math3::{Mat4, Vec3};
use crate::core::model::{Model, ModelTable};
use crate::core::motion::MotionState;
use crate::core::property::{self, PropertyTable};
use crate::core::rules::RuleSet;
use crate::core::scene::{ObjectSpec, Projection};
use crate::core::skeleton::{Blend, Playing, PoseScratch, Skeleton};
use crate::core::value::Value;

use super::load::{END_GAP_LIMIT, rebuild_skeleton};

/// Высота модели в клетках мира просмотра: рост человека в игре.
const HEIGHT: f64 = 1.8;

/// Сторона площадки под моделью в клетках.
const FOOTPRINT: f64 = 0.8;

/// Какую долю высоты окна занимает модель от ступней до макушки, когда вид только поставлен.
const FILL: f64 = 0.78;

/// Запас расстояния камеры сверх того, что едва вмещает площадку модели по ширине окна.
const WIDTH_MARGIN: f64 = 1.1;

/// Пределы расстояния камеры, между которыми ищется то, с которого модель занимает `FILL` окна.
const FIT_DISTANCE: (f64, f64) = (0.5, 200.0);

/// Сколько раз подряд уточняются расстояние и высота взгляда: от каждого зависит другое.
const FIT_ROUNDS: usize = 4;

/// Сколько раз делится пополам промежуток поиска.
const FIT_STEPS: usize = 48;

/// Наклон камер «сбоку» и «спереди» в градусах.
const LEVEL_PITCH: f64 = 10.0;

/// Ступня стоит на земле, пока её сустав выше самой низкой своей точки за движение не больше чем на эту долю роста.
const FOOT_ON_GROUND: f32 = 0.03;

/// Ступня на земле скользит, пока её скорость по земле отличается от скорости земли больше чем на эту долю роста в
/// секунду.
const FOOT_SLIP_SPEED: f32 = 0.1;

/// Сколько промежутков круга проверяется на скольжение, не меньше.
const MIN_SLIP_STEPS: usize = 48;

/// Пол стенда: сторона квадрата, клетка сетки и ширина её линий в клетках мира, цвета пола и линий.
const FLOOR_SIDE: f64 = 6.0;
const FLOOR_CELL: f64 = 0.5;
const FLOOR_LINE: f64 = 0.03;
const FLOOR_COLOR: [f32; 4] = [0.78, 0.78, 0.78, 1.0];
const FLOOR_LINE_COLOR: [f32; 4] = [0.6, 0.6, 0.6, 1.0];

/// Радиус колец поворота в долях роста модели.
const RING_RADIUS: f64 = 0.14;

/// Точек на кольцо.
const RING_POINTS: usize = 48;

/// Как близко к кости (в точках окна) щелчок считается щелчком по ней.
const PICK_RADIUS: f64 = 22.0;

/// Как близко к кольцу (в точках окна) оно считается схваченным.
const RING_PICK_RADIUS: f64 = 9.0;

/// С какой стороны смотрит камера стенда.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandView {
    Side,
    Front,
    Game,
}

impl StandView {
    pub fn parse(name: &str) -> Option<StandView> {
        match name {
            "side" => Some(StandView::Side),
            "front" => Some(StandView::Front),
            "game" => Some(StandView::Game),
            _ => None,
        }
    }
}

/// Кость скелета в дереве стенда.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneInfo {
    pub name: String,
    pub parent: Option<usize>,
}

/// Доли движения, где ступня стоит на земле и сдвигается по ней.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slip {
    /// 0 — первая ступня из `feet`, 1 — вторая.
    pub foot: usize,
    pub from: f32,
    pub to: f32,
}

/// То, что не зависит от доли: скольжение ступней и расхождение конца движения с началом.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    pub slips: Vec<Slip>,
    /// Наибольший поворот кости между началом и концом в градусах и номер кости.
    pub end_gap: Option<(f32, usize)>,
}

/// Кольцо поворота: центр на холсте и точки трёх колец; `None` — точка за камерой.
#[derive(Debug, Clone, PartialEq)]
pub struct Rings {
    pub center: Option<[f64; 2]>,
    pub rings: [Vec<Option<[f64; 2]>>; 3],
}

/// Схваченное кольцо: центр, плоскость и общий угол, что ушёл с хвата.
#[derive(Debug, Clone, Copy)]
struct Grab {
    center: Vec3,
    normal: Vec3,
    first: Vec3,
    second: Vec3,
    last: f64,
    total: f64,
}

pub struct Stand {
    game: Game,
    model: usize,
    object: u32,
    clip: usize,
    center: [f64; 2],
    scale: f64,
    camera: EditorCamera,
    projection: Projection,
    scratch: PoseScratch,
    grab: Option<Grab>,
    /// Повторяется ли движение: у движения без `loop` на стенде — да.
    looped: bool,
    pub bones: Vec<BoneInfo>,
    pub length: f32,
    pub frames: usize,
    pub analysis: Analysis,
}

impl Stand {
    /// Стенд для движения `name` из `motions_text` на модели игры `source`. Ошибки проверки — строками.
    pub fn build(
        source: &Game,
        motions_text: &str,
        name: &str,
        view: StandView,
        viewport: [f64; 2],
    ) -> Result<Stand, Vec<String>> {
        let models = source.properties.models();
        let names: Vec<String> = models.names().map(str::to_string).collect();
        let motion_model = motion_model_of(motions_text, name)
            .ok_or_else(|| vec![format!("в motions.json нет движения \"{name}\"")])?;
        let model = models
            .index_of(&motion_model)
            .ok_or_else(|| vec![format!("модель \"{motion_model}\" не объявлена")])?;
        let source_model = models
            .get(model)
            .filter(|model| model.skeleton.is_some() && !model.parts.is_empty())
            .ok_or_else(|| vec![format!("у модели \"{motion_model}\" нет скелета")])?;
        if !source.scene.is_3d() {
            return Err(vec![
                "стенд анимации есть только в трёхмерной сцене".to_string(),
            ]);
        }
        let mut skeleton = source_model.skeleton.as_ref().map_or_else(
            || Err(vec![format!("у модели \"{motion_model}\" нет скелета")]),
            |skeleton| rebuild_skeleton(skeleton, &motion_model, &names, motions_text),
        )?;
        let clip = skeleton
            .clip_named(name)
            .ok_or_else(|| vec![format!("движение \"{name}\" не легло на скелет")])?;
        let looped = skeleton.clips[clip]
            .retouch
            .as_mut()
            .is_none_or(|retouch| *retouch.looped.get_or_insert(true));
        let table: Vec<Model> = models
            .iter()
            .enumerate()
            .map(|(at, entry)| {
                if at == model {
                    Model {
                        skeleton: Some(skeleton.clone()),
                        ..entry.clone()
                    }
                } else {
                    Model::placeholder(&entry.name)
                }
            })
            .collect();
        let (length, frames) = measure(&skeleton, clip);
        let analysis = analyse(&skeleton, clip, looped, source_model.size[2] as f32, frames);
        let bones = skeleton
            .bones
            .iter()
            .map(|bone| BoneInfo {
                name: bone.name.clone(),
                parent: bone.parent,
            })
            .collect();
        let mut stand = Stand::world(source, table, &motion_model, model)?;
        stand.clip = clip;
        stand.looped = looped;
        stand.bones = bones;
        stand.length = length;
        stand.frames = frames;
        stand.analysis = analysis;
        stand.set_view(view, viewport);
        stand.pose(0.0);
        Ok(stand)
    }

    fn world(
        source: &Game,
        table: Vec<Model>,
        model_name: &str,
        model: usize,
    ) -> Result<Stand, Vec<String>> {
        let mut properties = PropertyTable::new();
        properties.set_three_d(true);
        properties.set_models(ModelTable::new(table));
        let scene = source.scene;
        let center = [f64::from(scene.width) / 2.0, f64::from(scene.height) / 2.0];
        let shown = std::iter::once(model_name).map(|name| ObjectSpec {
            values: vec![
                (property::MODEL, Value::Text(name.to_string())),
                (
                    property::POSITION,
                    Value::Vec2([center[0] - FOOTPRINT / 2.0, center[1] - FOOTPRINT / 2.0]),
                ),
                (property::SIZE, Value::Vec2([FOOTPRINT, FOOTPRINT])),
                (property::HEIGHT, Value::Number(HEIGHT)),
            ],
            grid: None,
            keys: None,
            on_click: None,
        });
        let specs: Vec<ObjectSpec> = shown.chain(floor_specs(center)).collect();
        let world = game::world_from_scene(&properties, &specs);
        let mut scene = scene;
        scene.player_camera = None;
        let game = Game::new(
            properties,
            world,
            RuleSet::default(),
            scene,
            16,
            Vec::new(),
            [0.0, 0.0],
            0,
            specs,
            0,
            None,
            String::new(),
            Vec::new(),
            Vec::new(),
            true,
            Vec::new(),
            ObjectParser::default(),
        );
        let Some(object) = game.world.ids().next() else {
            return Err(vec!["мир просмотра остался без модели".to_string()]);
        };
        let size = game
            .world
            .models()
            .get(model)
            .map_or(1.0, |model| model.scale(Some(HEIGHT)));
        Ok(Stand {
            game,
            model,
            object,
            clip: 0,
            center,
            scale: size,
            camera: EditorCamera {
                target: center,
                target_z: HEIGHT / 2.0,
                yaw: 0.0,
                pitch: LEVEL_PITCH,
                distance: 4.0,
            },
            projection: Projection::Perspective,
            scratch: PoseScratch::default(),
            grab: None,
            looped: true,
            bones: Vec::new(),
            length: 0.0,
            frames: 1,
            analysis: Analysis::default(),
        })
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Номер модели движения в таблице мира просмотра.
    pub fn model(&self) -> usize {
        self.model
    }

    /// Камера, которой мир просмотра виден в окне `viewport`.
    pub fn camera(&self, viewport: [f64; 2]) -> Camera3d {
        self.camera.clamped().camera_in(self.projection, viewport)
    }

    /// Ставит камеру на вид `view`: модель от ступней до макушки занимает `FILL` высоты окна и стоит посередине него.
    pub fn set_view(&mut self, view: StandView, viewport: [f64; 2]) {
        if viewport[0] < 1.0 || viewport[1] < 1.0 {
            return;
        }
        let game_camera = self.game.scene.camera;
        let pitch = match view {
            StandView::Side | StandView::Front => LEVEL_PITCH,
            StandView::Game => game_camera.map_or(55.0, |camera| camera.pitch),
        }
        .clamp(EDITOR_PITCH_RANGE.0, EDITOR_PITCH_RANGE.1);
        let projection = match (view, game_camera) {
            (StandView::Game, Some(camera)) => camera.projection,
            _ => Projection::Perspective,
        };
        let yaw = if view == StandView::Side { 90.0 } else { 0.0 };
        let looking = |target_z: f64, distance: f64| EditorCamera {
            target: self.center,
            target_z,
            yaw,
            pitch,
            distance,
        };
        let ends = |camera: &EditorCamera| {
            let camera = camera.camera_in(projection, viewport);
            let feet = camera.project([self.center[0], self.center[1], 0.0])?;
            let head = camera.project([self.center[0], self.center[1], HEIGHT])?;
            Some((feet[1], head[1]))
        };
        let distance_for = |target_z: f64| {
            bisect(FIT_DISTANCE, |distance| {
                ends(&looking(target_z, distance))
                    .is_none_or(|(feet, head)| (feet - head).abs() > FILL * viewport[1])
            })
        };
        let target_for = |distance: f64| {
            bisect((0.0, HEIGHT), |target_z| {
                ends(&looking(target_z, distance))
                    .is_some_and(|(feet, head)| feet + head < viewport[1])
            })
        };
        let mut target_z = HEIGHT / 2.0;
        for _ in 0..FIT_ROUNDS {
            target_z = target_for(distance_for(target_z));
        }
        let half = FOOTPRINT / 2.0;
        let corners = [[-half, -half], [half, -half], [half, half], [-half, half]].map(|corner| {
            [
                self.center[0] + corner[0],
                self.center[1] + corner[1],
                target_z,
            ]
        });
        let wide = camera::fit_distance(
            projection,
            &corners,
            self.center,
            target_z,
            yaw,
            pitch,
            viewport,
        );
        self.projection = projection;
        self.camera = looking(target_z, distance_for(target_z).max(wide * WIDTH_MARGIN));
    }

    /// Камера прежнего стенда `old` — вид не сбрасывается, когда стенд собран заново после правки.
    pub fn keep_camera(&mut self, old: &Stand) {
        self.camera = old.camera;
        self.projection = old.projection;
    }

    /// Колесо мыши приближает и отдаляет модель.
    pub fn zoom(&mut self, delta_y: f64) {
        self.camera = EditorCamera {
            distance: self.camera.distance * (delta_y * 0.002).exp(),
            ..self.camera
        }
        .clamped();
    }

    /// Правая кнопка с перетаскиванием вращает вид вокруг модели: сдвиг мыши в точках окна.
    pub fn turn(&mut self, dx: f64, dy: f64) {
        self.camera = EditorCamera {
            yaw: self.camera.yaw - dx * 0.4,
            pitch: self.camera.pitch + dy * 0.4,
            ..self.camera
        }
        .clamped();
    }

    /// Ставит позу движения в долю `phase` от 0 до 1 — в мире просмотра для рисования и в рабочих буферах стенда.
    pub fn pose(&mut self, phase: f32) {
        let time = phase.clamp(0.0, 1.0) * self.length;
        self.game
            .world
            .set_motion(self.object, MotionState::pinned(self.clip, time));
        let Some(skeleton) = self
            .game
            .world
            .models()
            .get(self.model)
            .and_then(|model| model.skeleton.as_ref())
        else {
            return;
        };
        skeleton.pose(&self.blend(time), &mut self.scratch);
    }

    fn blend(&self, time: f32) -> Blend {
        Blend::single(Playing {
            clip: Some(self.clip),
            time,
            looped: self.looped,
        })
    }

    fn world_point(&self, matrix: &Mat4) -> Vec3 {
        [
            self.center[0] + f64::from(matrix[3][0]) * self.scale,
            self.center[1] + f64::from(matrix[3][1]) * self.scale,
            f64::from(matrix[3][2]) * self.scale,
        ]
    }

    /// Суставы на холсте в позе, что стоит сейчас: по кости на каждую, `None` — за камерой.
    pub fn joints(&self, camera: &Camera3d) -> Vec<Option<[f64; 2]>> {
        self.scratch
            .globals()
            .iter()
            .map(|matrix| camera.project(self.world_point(matrix)))
            .collect()
    }

    /// Кость, ближайшая к точке холста `point` не дальше `PICK_RADIUS`: расстояние — до отрезка от кости до любого
    /// её потомка, у кости без потомков — до самой точки.
    pub fn bone_at(&self, camera: &Camera3d, point: [f64; 2]) -> Option<usize> {
        let joints = self.joints(camera);
        let mut best: Option<(f64, usize)> = None;
        for bone in 0..self.bones.len() {
            let Some(head) = joints[bone] else {
                continue;
            };
            let tails: Vec<[f64; 2]> = self
                .bones
                .iter()
                .enumerate()
                .filter(|(_, child)| child.parent == Some(bone))
                .filter_map(|(child, _)| joints[child])
                .collect();
            let reach = if tails.is_empty() {
                distance_to_segment(point, head, head)
            } else {
                tails
                    .iter()
                    .map(|&tail| distance_to_segment(point, head, tail))
                    .fold(f64::INFINITY, f64::min)
            };
            if reach <= PICK_RADIUS && best.is_none_or(|(nearest, _)| reach < nearest) {
                best = Some((reach, bone));
            }
        }
        best.map(|(_, bone)| bone)
    }

    /// Оси кости `bone` в мире просмотра: центр и три направления её осей.
    fn axes_of(&self, bone: usize) -> Option<(Vec3, [Vec3; 3])> {
        let matrix = self.scratch.globals().get(bone)?;
        let axis = |column: usize| {
            let v = [
                f64::from(matrix[column][0]),
                f64::from(matrix[column][1]),
                f64::from(matrix[column][2]),
            ];
            let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
            v.map(|c| c / length)
        };
        Some((self.world_point(matrix), [axis(0), axis(1), axis(2)]))
    }

    fn ring_basis(&self, bone: usize, axis: usize) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
        let (center, axes) = self.axes_of(bone)?;
        Some((
            center,
            axes[axis],
            axes[(axis + 1) % 3],
            axes[(axis + 2) % 3],
        ))
    }

    /// Кольца поворота кости `bone` вокруг трёх её осей на холсте.
    pub fn rings(&self, camera: &Camera3d, bone: usize) -> Option<Rings> {
        let radius = HEIGHT * RING_RADIUS;
        let mut rings: [Vec<Option<[f64; 2]>>; 3] = Default::default();
        let (center, _) = self.axes_of(bone)?;
        for (axis, ring) in rings.iter_mut().enumerate() {
            let (_, _, first, second) = self.ring_basis(bone, axis)?;
            for step in 0..RING_POINTS {
                let angle = std::f64::consts::TAU * step as f64 / RING_POINTS as f64;
                let point = std::array::from_fn(|k| {
                    center[k] + radius * (angle.cos() * first[k] + angle.sin() * second[k])
                });
                ring.push(camera.project(point));
            }
        }
        Some(Rings {
            center: camera.project(center),
            rings,
        })
    }

    /// Кольцо кости `bone` под точкой холста: 0, 1 или 2 — ось `x`, `y` или `z`.
    pub fn ring_at(&self, camera: &Camera3d, bone: usize, point: [f64; 2]) -> Option<usize> {
        let rings = self.rings(camera, bone)?;
        let mut best: Option<(f64, usize)> = None;
        for (axis, ring) in rings.rings.iter().enumerate() {
            for step in 0..ring.len() {
                let (Some(a), Some(b)) = (ring[step], ring[(step + 1) % ring.len()]) else {
                    continue;
                };
                let reach = distance_to_segment(point, a, b);
                if reach <= RING_PICK_RADIUS && best.is_none_or(|(nearest, _)| reach < nearest) {
                    best = Some((reach, axis));
                }
            }
        }
        best.map(|(_, axis)| axis)
    }

    /// Хват кольца `axis` кости `bone` в точке холста: с этого мига `drag_ring` считает угол от хвата.
    pub fn grab_ring(&mut self, camera: &Camera3d, bone: usize, axis: usize, point: [f64; 2]) {
        self.grab = None;
        let Some((center, normal, first, second)) = self.ring_basis(bone, axis) else {
            return;
        };
        let mut grab = Grab {
            center,
            normal,
            first,
            second,
            last: 0.0,
            total: 0.0,
        };
        if let Some(angle) = ring_angle(camera, &grab, point) {
            grab.last = angle;
            self.grab = Some(grab);
        }
    }

    /// Угол в градусах, на который точка `point` ушла по кольцу от хвата: положительный разворачивает кость вокруг
    /// выбранной оси в сторону прибавки к её повороту. `None` без хвата или когда луч не пересекает плоскость кольца.
    pub fn drag_ring(&mut self, camera: &Camera3d, point: [f64; 2]) -> Option<f64> {
        let grab = self.grab.as_mut()?;
        let angle = ring_angle(camera, grab, point)?;
        let mut step = angle - grab.last;
        if step > std::f64::consts::PI {
            step -= std::f64::consts::TAU;
        } else if step < -std::f64::consts::PI {
            step += std::f64::consts::TAU;
        }
        grab.total += step;
        grab.last = angle;
        Some(grab.total.to_degrees())
    }

    pub fn release_ring(&mut self) {
        self.grab = None;
    }
}

/// Пол стенда: квадрат `FLOOR_SIDE` со серединой в `center`, поверх него линии сетки через `FLOOR_CELL` — плоские
/// объекты с цветом, свет и тени ложатся на них, как на любую землю. Линии рисуются после пола: они выше в порядке
/// слоёв.
fn floor_specs(center: [f64; 2]) -> Vec<ObjectSpec> {
    let flat = |position: [f64; 2], size: [f64; 2], color: [f32; 4], layer: i32| ObjectSpec {
        values: vec![
            (property::POSITION, Value::Vec2(position)),
            (property::SIZE, Value::Vec2(size)),
            (property::COLOR, Value::Color(color)),
            (property::LAYER, Value::Layer(layer)),
        ],
        grid: None,
        keys: None,
        on_click: None,
    };
    let half = FLOOR_SIDE / 2.0;
    let corner = [center[0] - half, center[1] - half];
    let mut specs = vec![flat(corner, [FLOOR_SIDE; 2], FLOOR_COLOR, 0)];
    let lines = (FLOOR_SIDE / FLOOR_CELL).round() as usize;
    for line in 0..=lines {
        let along = line as f64 * FLOOR_CELL - FLOOR_LINE / 2.0;
        specs.push(flat(
            [corner[0] + along, corner[1]],
            [FLOOR_LINE, FLOOR_SIDE],
            FLOOR_LINE_COLOR,
            1,
        ));
        specs.push(flat(
            [corner[0], corner[1] + along],
            [FLOOR_SIDE, FLOOR_LINE],
            FLOOR_LINE_COLOR,
            1,
        ));
    }
    specs
}

/// Модель движения `name` в тексте `motions.json`; `None` — текст не читается или такого движения нет.
fn motion_model_of(text: &str, name: &str) -> Option<String> {
    let table: serde_json::Value = serde_json::from_str(text).ok()?;
    table.get(name)?.get("model")?.as_str().map(str::to_string)
}

/// Длина движения в секундах и число кадров в нём.
fn measure(skeleton: &Skeleton, clip: usize) -> (f32, usize) {
    let Some(clip) = skeleton.clips.get(clip) else {
        return (0.0, 1);
    };
    let base = clip.retouch.as_ref().map_or(0, |retouch| retouch.base);
    let keys = skeleton.key_count(base);
    let frames = if keys > 1 {
        keys - 1
    } else {
        ((clip.length * 30.0).round() as usize).max(1)
    };
    (clip.length, frames)
}

/// Скольжение ступней и расхождение конца с началом движения `clip` у модели высотой `height`.
fn analyse(skeleton: &Skeleton, clip: usize, looped: bool, height: f32, frames: usize) -> Analysis {
    Analysis {
        slips: slips_of(skeleton, clip, looped, height, frames),
        end_gap: skeleton
            .ends_gap(clip, true)
            .filter(|&(degrees, _)| looped && degrees > END_GAP_LIMIT),
    }
}

fn slips_of(
    skeleton: &Skeleton,
    clip: usize,
    looped: bool,
    height: f32,
    frames: usize,
) -> Vec<Slip> {
    let (Some(legs), Some(playing)) = (skeleton.legs, skeleton.clips.get(clip)) else {
        return Vec::new();
    };
    if !playing.is_playable() || height <= 0.0 {
        return Vec::new();
    }
    let steps = frames.max(MIN_SLIP_STEPS);
    let mut scratch = PoseScratch::default();
    let samples: Vec<[[f32; 3]; 2]> = (0..=steps)
        .map(|step| {
            let time = playing.length * step as f32 / steps as f32;
            skeleton.pose(
                &Blend::single(Playing {
                    clip: Some(clip),
                    time,
                    looped,
                }),
                &mut scratch,
            );
            legs.map(|leg| {
                let m = scratch.globals()[leg.foot];
                [m[3][0], m[3][1], m[3][2]]
            })
        })
        .collect();
    let seconds = playing.length / steps as f32;
    let lowest: [f32; 2] = std::array::from_fn(|foot| {
        samples
            .iter()
            .map(|feet| feet[foot][2])
            .fold(f32::INFINITY, f32::min)
    });
    let on_ground =
        |foot: usize, point: [f32; 3]| point[2] - lowest[foot] < FOOT_ON_GROUND * height;
    let mut moves: Vec<(usize, usize, [f32; 2])> = Vec::new();
    for foot in 0..2 {
        for (step, pair) in samples.windows(2).enumerate() {
            let (a, b) = (pair[0][foot], pair[1][foot]);
            if on_ground(foot, a) && on_ground(foot, b) {
                moves.push((
                    foot,
                    step,
                    [(b[0] - a[0]) / seconds, (b[1] - a[1]) / seconds],
                ));
            }
        }
    }
    let ground_speed = [0, 1].map(|axis| median(moves.iter().map(|(_, _, speed)| speed[axis])));
    let mut slips: Vec<Slip> = Vec::new();
    for (foot, step, speed) in moves {
        let off = (speed[0] - ground_speed[0]).hypot(speed[1] - ground_speed[1]);
        if off <= FOOT_SLIP_SPEED * height {
            continue;
        }
        let (from, to) = (step as f32 / steps as f32, (step + 1) as f32 / steps as f32);
        match slips.last_mut() {
            Some(last) if last.foot == foot && (last.to - from).abs() < 1e-6 => last.to = to,
            _ => slips.push(Slip { foot, from, to }),
        }
    }
    slips
}

/// Число из промежутка `range`, с которого `above` перестаёт быть истиной: `above(x)` — искомое число выше `x`.
fn bisect(range: (f64, f64), above: impl Fn(f64) -> bool) -> f64 {
    let (mut low, mut high) = range;
    for _ in 0..FIT_STEPS {
        let middle = (low + high) / 2.0;
        if above(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    high
}

/// Середина упорядоченных чисел `values`; без чисел — ноль.
fn median(values: impl Iterator<Item = f32>) -> f32 {
    let mut sorted: Vec<f32> = values.collect();
    sorted.sort_by(f32::total_cmp);
    match sorted.len() {
        0 => 0.0,
        len if len % 2 == 1 => sorted[len / 2],
        len => (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0,
    }
}

fn distance_to_segment(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let length2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if length2 < 1e-12 {
        0.0
    } else {
        (((point[0] - a[0]) * ab[0] + (point[1] - a[1]) * ab[1]) / length2).clamp(0.0, 1.0)
    };
    let nearest = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    ((point[0] - nearest[0]).powi(2) + (point[1] - nearest[1]).powi(2)).sqrt()
}

/// Угол точки холста на плоскости кольца от `first` к `second`, радианы.
fn ring_angle(camera: &Camera3d, grab: &Grab, point: [f64; 2]) -> Option<f64> {
    let direction = camera.ray_direction(point);
    let along = dot(direction, grab.normal);
    if along.abs() < 1e-9 {
        return None;
    }
    let origin = camera.ray_origin(point);
    let reach = dot(
        [
            grab.center[0] - origin[0],
            grab.center[1] - origin[1],
            grab.center[2] - origin[2],
        ],
        grab.normal,
    ) / along;
    if reach <= 0.0 {
        return None;
    }
    let offset = std::array::from_fn(|k| origin[k] + direction[k] * reach - grab.center[k]);
    Some(dot(offset, grab.second).atan2(dot(offset, grab.first)))
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

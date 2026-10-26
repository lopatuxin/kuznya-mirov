use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::keys::{KeyEdit, KeyTable};
use super::model::{Model, ModelTable};
use super::property::{self, PropertyId, PropertyTable};
use super::road_ground::{RoadGround, RoadInput};
use super::road_kinds::{LayoutForm, RoadKindTable};
use super::roads::{RoadForm, RoadShape};
use super::run_jump::JumpState;
use super::surface;
use super::terrain::Terrain;
use super::value::{FollowAxis, GridSpec, ImageId, PropKind, Rotation, Shape, Value, Vec2};

static NEXT_SURFACE_REVISION: AtomicU64 = AtomicU64::new(1);
static NEXT_TERRAIN_REVISION: AtomicU64 = AtomicU64::new(1);

/// Свойства, от которых зависят место и верх настила.
const DECK_GEOMETRY: [PropertyId; 5] = [
    property::POSITION,
    property::SIZE,
    property::ROTATION,
    property::HEIGHT,
    property::SHAPE,
];

/// Свойства, от которых зависят ось дороги, её высоты и поправки земли под ней.
const ROAD_GEOMETRY: [PropertyId; 6] = [
    property::POSITION,
    property::SIZE,
    property::ROTATION,
    property::BEND,
    property::ROAD,
    property::DECK,
];

#[derive(Debug, Clone)]
enum Column {
    Flag(Vec<bool>),
    Number(Vec<Option<f64>>),
    Time(Vec<Option<i64>>),
    Timer(Vec<Option<i64>>),
    Vec2(Vec<Option<Vec2>>),
    Color(Vec<Option<[f32; 4]>>),
    Layer(Vec<Option<i32>>),
    Text(Vec<Option<String>>),
    Grid(Vec<Option<GridSpec>>),
    Keys(Vec<Option<KeyTable>>),
    Image(Vec<Option<ImageId>>),
    ImageList(Vec<Option<Vec<ImageId>>>),
    Rotation(Vec<Option<Rotation>>),
    FollowMouse(Vec<Option<FollowAxis>>),
    Shape(Vec<Option<Shape>>),
    OnClick(Vec<Option<Vec<KeyEdit>>>),
}

impl Column {
    fn new(kind: PropKind) -> Self {
        match kind {
            PropKind::Flag => Column::Flag(Vec::new()),
            PropKind::Number => Column::Number(Vec::new()),
            PropKind::Time => Column::Time(Vec::new()),
            PropKind::Timer => Column::Timer(Vec::new()),
            PropKind::Vec2 => Column::Vec2(Vec::new()),
            PropKind::Color => Column::Color(Vec::new()),
            PropKind::Layer => Column::Layer(Vec::new()),
            PropKind::Text => Column::Text(Vec::new()),
            PropKind::Grid => Column::Grid(Vec::new()),
            PropKind::Keys => Column::Keys(Vec::new()),
            PropKind::Image => Column::Image(Vec::new()),
            PropKind::ImageList => Column::ImageList(Vec::new()),
            PropKind::Rotation => Column::Rotation(Vec::new()),
            PropKind::FollowMouse => Column::FollowMouse(Vec::new()),
            PropKind::Shape => Column::Shape(Vec::new()),
            PropKind::OnClick => Column::OnClick(Vec::new()),
        }
    }

    fn push_empty(&mut self) {
        match self {
            Column::Flag(v) => v.push(false),
            Column::Number(v) => v.push(None),
            Column::Time(v) => v.push(None),
            Column::Timer(v) => v.push(None),
            Column::Vec2(v) => v.push(None),
            Column::Color(v) => v.push(None),
            Column::Layer(v) => v.push(None),
            Column::Text(v) => v.push(None),
            Column::Grid(v) => v.push(None),
            Column::Keys(v) => v.push(None),
            Column::Image(v) => v.push(None),
            Column::ImageList(v) => v.push(None),
            Column::Rotation(v) => v.push(None),
            Column::FollowMouse(v) => v.push(None),
            Column::Shape(v) => v.push(None),
            Column::OnClick(v) => v.push(None),
        }
    }

    fn clear(&mut self, id: usize) {
        match self {
            Column::Flag(v) => v[id] = false,
            Column::Number(v) => v[id] = None,
            Column::Time(v) => v[id] = None,
            Column::Timer(v) => v[id] = None,
            Column::Vec2(v) => v[id] = None,
            Column::Color(v) => v[id] = None,
            Column::Layer(v) => v[id] = None,
            Column::Text(v) => v[id] = None,
            Column::Grid(v) => v[id] = None,
            Column::Keys(v) => v[id] = None,
            Column::Image(v) => v[id] = None,
            Column::ImageList(v) => v[id] = None,
            Column::Rotation(v) => v[id] = None,
            Column::FollowMouse(v) => v[id] = None,
            Column::Shape(v) => v[id] = None,
            Column::OnClick(v) => v[id] = None,
        }
    }

    fn has(&self, id: usize) -> bool {
        match self {
            Column::Flag(v) => v[id],
            Column::Number(v) => v[id].is_some(),
            Column::Time(v) => v[id].is_some(),
            Column::Timer(v) => v[id].is_some(),
            Column::Vec2(v) => v[id].is_some(),
            Column::Color(v) => v[id].is_some(),
            Column::Layer(v) => v[id].is_some(),
            Column::Text(v) => v[id].is_some(),
            Column::Grid(v) => v[id].is_some(),
            Column::Keys(v) => v[id].is_some(),
            Column::Image(v) => v[id].is_some(),
            Column::ImageList(v) => v[id].is_some(),
            Column::Rotation(v) => v[id].is_some(),
            Column::FollowMouse(v) => v[id].is_some(),
            Column::Shape(v) => v[id].is_some(),
            Column::OnClick(v) => v[id].is_some(),
        }
    }
}

#[derive(Debug)]
pub struct World {
    alive: Vec<bool>,
    free_list: Vec<u32>,
    columns: Vec<Column>,
    grid_counter: Vec<i64>,
    /// «Код игры»: bumped on every `create()` (fresh slot or reused one) so a code handle taken
    /// while an object was alive can tell it apart from a later, unrelated object that reused the
    /// same freed slot — see `generation`.
    generation: Vec<u32>,
    /// «Трёхмерная сцена»: мир трёхмерной игры — `rotation` поворачивает и место на земле, а не только
    /// вид (`core::footprint`). Берётся из таблицы свойств, по которой мир строится.
    three_d: bool,
    /// «Рельеф»: земля трёхмерной сцены вместе с поправками дорог без `z`; `None` — ровная на высоте 0. Берётся
    /// из таблицы свойств и правится под дорогами мира.
    terrain: Option<Arc<Terrain>>,
    /// «Рельеф» → «Дороги на рельефе»: земля такой, какой её отдала таблица свойств, — без поправок дорог.
    plain: Option<Arc<Terrain>>,
    /// «Езда по дорогам»: высоты концов дорог и охваты их поправок земли.
    road_ground: RoadGround,
    /// Дороги правятся пачкой (сборка мира): пересчёт ждёт `settle_roads`.
    roads_suspended: bool,
    roads_dirty: bool,
    /// Номер состояния высот земли: по нему перестраивается сетка рельефа для рисования.
    terrain_revision: u64,
    /// «Рельеф»: высота основания записана (третье число `position`, `z` кода) — дорога с ней ровная.
    leveled: Vec<bool>,
    /// «Трёхмерная сцена»: наклон машины на дороге вдоль её длины, радианы; нос выше при положительном.
    tilt: Vec<f64>,
    /// «Трёхмерная сцена» → «Модели и движения»: модели игры; объект называет свою свойством `model`.
    /// Берутся из таблицы свойств.
    models: Arc<ModelTable>,
    /// «Трёхмерная сцена» → «Вид дороги»: виды дорог игры; дорога называет свой свойством `road_kind`. Берутся
    /// из таблицы свойств.
    road_kinds: Arc<RoadKindTable>,
    /// «Рельеф»: высота основания `z` каждого объекта — третье число `position` живого мира.
    base_z: Vec<f64>,
    /// «Рельеф»: третье число `walk_to`, если оно задано.
    walk_to_z: Vec<Option<f64>>,
    /// «Бег и прыжок»: память нажатия, запас после края и признак подъёма каждого объекта — в мире, а не
    /// в свойствах: повтор записи даёт ту же партию шаг в шаг.
    jump: Vec<JumpState>,
    /// «Мир на экране» → «Камера игрока»: сколько раз объектам записали `camera_follows` — признак
    /// «записали», а не «есть»: запись тому же объекту заново тоже возвращает слежение.
    camera_follow_writes: u64,
    /// «Рельеф» → «Высота объектов»: номера объектов с `deck` по возрастанию — настил ищется среди них, а не
    /// по всему миру.
    decks: Vec<u32>,
    /// «Рельеф» → «Вид»: номер состояния земли и настилов. Новый — у каждого нового мира, при смене рельефа и
    /// при каждой правке настила; счётчик общий на все миры, так что форма, посчитанная в другом мире, не
    /// подойдёт.
    surface_revision: u64,
}

fn next_surface_revision() -> u64 {
    NEXT_SURFACE_REVISION.fetch_add(1, Ordering::Relaxed)
}

fn next_terrain_revision() -> u64 {
    NEXT_TERRAIN_REVISION.fetch_add(1, Ordering::Relaxed)
}

impl World {
    pub fn new(properties: &PropertyTable) -> Self {
        let columns = properties
            .iter()
            .map(|(_, def)| Column::new(def.kind))
            .collect();
        World {
            alive: Vec::new(),
            free_list: Vec::new(),
            columns,
            grid_counter: Vec::new(),
            generation: Vec::new(),
            three_d: properties.three_d(),
            terrain: properties.terrain().cloned(),
            plain: properties.terrain().cloned(),
            road_ground: RoadGround::default(),
            roads_suspended: false,
            roads_dirty: false,
            terrain_revision: next_terrain_revision(),
            leveled: Vec::new(),
            tilt: Vec::new(),
            models: Arc::clone(properties.models()),
            road_kinds: Arc::clone(properties.road_kinds()),
            base_z: Vec::new(),
            walk_to_z: Vec::new(),
            jump: Vec::new(),
            camera_follow_writes: 0,
            decks: Vec::new(),
            surface_revision: next_surface_revision(),
        }
    }

    /// Номера настилов (объектов с `deck`) по возрастанию.
    pub fn deck_ids(&self) -> &[u32] {
        &self.decks
    }

    /// Номер состояния земли и настилов: меняется вместе с рельефом и с любой правкой настила — наличием
    /// `deck`, местом, верхом.
    pub fn surface_revision(&self) -> u64 {
        self.surface_revision
    }

    fn surface_changed(&mut self) {
        self.surface_revision = next_surface_revision();
    }

    /// Запись свойства `prop` объекта `id` дошла до колонки: список настилов и номер состояния идут за ней.
    fn note_write(&mut self, id: u32, prop: PropertyId) {
        if prop == property::DECK {
            let member = self.flag(id, property::DECK);
            match self.decks.binary_search(&id) {
                Ok(at) if !member => {
                    self.decks.remove(at);
                }
                Err(at) if member => self.decks.insert(at, id),
                _ => return,
            }
            self.surface_changed();
        } else if DECK_GEOMETRY.contains(&prop) {
            self.note_deck_moved(id);
        }
        if ROAD_GEOMETRY.contains(&prop)
            && (prop == property::ROAD || self.flag(id, property::ROAD))
        {
            self.roads_changed();
        }
    }

    fn roads_changed(&mut self) {
        if self.roads_suspended {
            self.roads_dirty = true;
        } else {
            self.refresh_roads();
        }
    }

    /// «Езда по дорогам»: дороги правятся пачкой — например, при сборке мира: пересчёт высот и поправок земли
    /// ждёт `settle_roads`.
    pub fn suspend_roads(&mut self) {
        self.roads_suspended = true;
    }

    /// Кончает пачку правок дорог и пересчитывает высоты и землю под ними, если дороги правили.
    pub fn settle_roads(&mut self) {
        self.roads_suspended = false;
        if self.roads_dirty {
            self.refresh_roads();
        }
    }

    /// Дороги мира, как их видит расчёт высот: живые объекты с признаком `road`, `position` и `size`, по
    /// возрастанию номера. Дорога ровная, если `z` записан или она настил.
    fn road_inputs(&self) -> Vec<RoadInput> {
        self.ids()
            .filter(|&id| self.flag(id, property::ROAD))
            .filter_map(|id| {
                let level = self.is_level(id) || self.flag(id, property::DECK);
                Some(RoadInput {
                    shape: RoadShape {
                        id,
                        position: self.vec2(id, property::POSITION)?,
                        size: self.vec2(id, property::SIZE)?,
                        sin_cos: self
                            .rotation(id, property::ROTATION)
                            .map_or((0.0, 1.0), Rotation::sin_cos),
                        z: if level { self.base_z(id) } else { 0.0 },
                    },
                    bend: self
                        .number_like(id, property::BEND)
                        .filter(|bend| !bend.is_nan())
                        .unwrap_or(0.0),
                    level,
                })
            })
            .collect()
    }

    /// Ставит высоты концов дорог без `z` и правит землю под ними: только там, где дороги изменились.
    fn refresh_roads(&mut self) {
        self.roads_dirty = false;
        let inputs = self.road_inputs();
        if inputs.is_empty() && self.road_ground.is_empty() {
            return;
        }
        let plain: &Terrain = self.plain.as_deref().unwrap_or_else(|| Terrain::flat());
        let patch = self.road_ground.settle(&inputs, plain);
        for (input, form) in inputs.iter().zip(self.road_ground.forms()) {
            if !input.level
                && let Some(slot) = self.base_z.get_mut(input.shape.id as usize)
            {
                *slot = form.shape.z.min(form.end_z);
            }
        }
        let (Some(patch), Some(plain)) = (patch, self.plain.as_ref()) else {
            return;
        };
        let terrain = Arc::make_mut(self.terrain.get_or_insert_with(|| Arc::clone(plain)));
        if terrain.adjust_for_roads(patch.area.clone(), |column, row| {
            patch.lift(plain, column, row)
        }) {
            self.terrain_revision = next_terrain_revision();
            self.surface_changed();
        }
    }

    /// Номер состояния высот земли: новый у каждого мира и при каждой правке земли дорогами; счётчик общий на все миры.
    pub fn terrain_revision(&self) -> u64 {
        self.terrain_revision
    }

    /// Форма дороги `id` с высотами обоих концов; `None`, если объект не дорога.
    pub fn road_form(&self, id: u32) -> Option<&RoadForm> {
        self.road_ground.form_of(id)
    }

    /// Записана ли объекту высота основания (`z` в данных или в коде).
    pub fn is_level(&self, id: u32) -> bool {
        self.leveled.get(id as usize).copied().unwrap_or(false)
    }

    /// Записывает высоту основания ровно на `z`: дорога с ней ровная.
    pub fn set_level_z(&mut self, id: u32, z: f64) {
        if let Some(slot) = self.leveled.get_mut(id as usize) {
            *slot = true;
        }
        self.set_base_z(id, z);
    }

    /// Наклон объекта вдоль его длины, радианы; ноль, пока ничего не наклоняло.
    pub fn tilt(&self, id: u32) -> f64 {
        self.tilt.get(id as usize).copied().unwrap_or(0.0)
    }

    pub fn set_tilt(&mut self, id: u32, angle: f64) {
        if let Some(slot) = self.tilt.get_mut(id as usize) {
            *slot = angle;
        }
    }

    /// Выпрямляет все наклонённые объекты, кроме `keep`.
    pub fn level_tilts_except(&mut self, keep: &std::collections::HashSet<u32>) {
        for (id, angle) in self.tilt.iter_mut().enumerate() {
            if *angle != 0.0 && !keep.contains(&(id as u32)) {
                *angle = 0.0;
            }
        }
    }

    fn note_deck_moved(&mut self, id: u32) {
        if self.decks.binary_search(&id).is_ok() {
            self.surface_changed();
        }
    }

    pub fn three_d(&self) -> bool {
        self.three_d
    }

    /// «Рельеф»: земля этого мира; без файла рельефа — ровная на высоте 0.
    pub fn terrain(&self) -> &Terrain {
        match &self.terrain {
            Some(terrain) => terrain,
            None => Terrain::flat(),
        }
    }

    /// Модели игры.
    pub fn models(&self) -> &ModelTable {
        &self.models
    }

    /// Виды дорог игры.
    pub fn road_kinds(&self) -> &RoadKindTable {
        &self.road_kinds
    }

    /// Модель, которую рисует объект `id`: названная в его `model`; `None` без `model` и если такой модели
    /// нет.
    pub fn model_of(&self, id: u32) -> Option<&Model> {
        let name = self.text(id, property::MODEL)?;
        self.models.get(self.models.index_of(name)?)
    }

    /// Меняет землю мира на `terrain` из таблицы свойств, ничего не пересаживая: годится, когда высоты
    /// и отпечатки те же, а другое — например, покрытия.
    pub fn replace_terrain(&mut self, terrain: Option<Arc<Terrain>>) {
        self.plain = terrain.clone();
        self.terrain = terrain;
        self.road_ground.reset();
        self.surface_changed();
        self.roads_changed();
    }

    /// Высота основания объекта; 0, пока ничего не ставило её.
    pub fn base_z(&self, id: u32) -> f64 {
        self.base_z.get(id as usize).copied().unwrap_or(0.0)
    }

    /// Ставит основание ровно на `z`, ничего не пересаживая.
    pub fn set_base_z(&mut self, id: u32, z: f64) {
        if let Some(slot) = self.base_z.get_mut(id as usize) {
            *slot = z;
        }
        self.note_deck_moved(id);
        if self.is_alive(id) && self.flag(id, property::ROAD) {
            self.roads_changed();
        }
    }

    /// «Езда по дорогам»: дороги мира — живые объекты с признаком `road`, `position` и `size`, по
    /// возрастанию номера, — с высотами обоих концов и изгибом, и предельная скорость каждой; `NaN` в `road_speed`
    /// считается закрытой.
    pub fn collect_roads(&self, forms: &mut Vec<RoadForm>, speeds: &mut Vec<Option<f64>>) {
        forms.clear();
        speeds.clear();
        for form in self.road_ground.forms() {
            forms.push(*form);
            speeds.push(
                self.number_like(form.shape.id, property::ROAD_SPEED)
                    .map(|speed| if speed.is_nan() { 0.0 } else { speed }),
            );
        }
    }

    /// «Вид дороги»: все дороги мира для раскладки видов — вместе с закрытыми и без вида, они нужны связям, —
    /// по возрастанию номера, и вид каждой.
    pub fn collect_layout_roads(&self, roads: &mut Vec<LayoutForm>) {
        roads.clear();
        roads.extend(self.road_ground.forms().map(|form| {
            LayoutForm {
                form: *form,
                kind: self
                    .text(form.shape.id, property::ROAD_KIND)
                    .and_then(|name| self.road_kinds.index_of(name)),
            }
        }));
    }

    pub fn jump_state(&self, id: u32) -> JumpState {
        self.jump.get(id as usize).copied().unwrap_or_default()
    }

    pub fn set_jump_state(&mut self, id: u32, state: JumpState) {
        if let Some(slot) = self.jump.get_mut(id as usize) {
            *slot = state;
        }
    }

    pub fn walk_to_z(&self, id: u32) -> Option<f64> {
        self.walk_to_z.get(id as usize).copied().flatten()
    }

    /// Третье число пары: высота основания у `position` трёхмерного мира, высота цели у `walk_to`,
    /// если задана; у остальных пар его нет.
    pub fn placed_z(&self, id: u32, prop: PropertyId) -> Option<f64> {
        match prop {
            property::POSITION if self.three_d => Some(self.base_z(id)),
            property::WALK_TO => self.walk_to_z(id),
            _ => None,
        }
    }

    pub fn create(&mut self) -> u32 {
        if let Some(id) = self.free_list.pop() {
            self.alive[id as usize] = true;
            self.generation[id as usize] += 1;
            self.base_z[id as usize] = 0.0;
            self.leveled[id as usize] = false;
            self.tilt[id as usize] = 0.0;
            self.walk_to_z[id as usize] = None;
            self.jump[id as usize] = JumpState::default();
            return id;
        }
        let id = self.alive.len() as u32;
        self.alive.push(true);
        self.grid_counter.push(0);
        self.generation.push(0);
        self.base_z.push(0.0);
        self.leveled.push(false);
        self.tilt.push(0.0);
        self.walk_to_z.push(None);
        self.jump.push(JumpState::default());
        for column in &mut self.columns {
            column.push_empty();
        }
        id
    }

    /// «Код игры»: the object currently occupying slot `id` was created by this many `create()`
    /// calls into that slot — a code handle is stale (points at a deleted object even though its
    /// slot may already hold a new one) when this no longer matches the generation it was taken at.
    pub fn generation(&self, id: u32) -> u32 {
        self.generation[id as usize]
    }

    /// «Код игры»: clears one property on a live object without deleting the object itself — the
    /// runtime write side for `obj.prop = nil`. A no-op past the slot count, same as every other
    /// per-property accessor here.
    pub fn clear_property(&mut self, id: u32, prop: PropertyId) {
        self.columns[prop as usize].clear(id as usize);
        if prop == property::WALK_TO {
            self.walk_to_z[id as usize] = None;
        }
        self.note_write(id, prop);
    }

    /// «Редактор», требование 32: a number past `slot_count` — a stale replay `delete`, or one
    /// from another project's recording — is simply skipped, not a panicking index.
    pub fn delete(&mut self, id: u32) {
        let idx = id as usize;
        if !self.alive.get(idx).copied().unwrap_or(false) {
            return;
        }
        let was_road = self.flag(id, property::ROAD);
        self.alive[idx] = false;
        if let Ok(at) = self.decks.binary_search(&id) {
            self.decks.remove(at);
            self.surface_changed();
        }
        for column in &mut self.columns {
            column.clear(idx);
        }
        self.base_z[idx] = 0.0;
        self.leveled[idx] = false;
        self.tilt[idx] = 0.0;
        self.walk_to_z[idx] = None;
        self.jump[idx] = JumpState::default();
        self.free_list.push(id);
        if was_road {
            self.roads_changed();
        }
    }

    pub fn is_alive(&self, id: u32) -> bool {
        self.alive.get(id as usize).copied().unwrap_or(false)
    }

    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }

    pub fn slot_count(&self) -> usize {
        self.alive.len()
    }

    /// Ascending object ids currently alive.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.alive
            .iter()
            .enumerate()
            .filter(|&(_, &a)| a)
            .map(|(i, _)| i as u32)
    }

    pub fn has(&self, id: u32, prop: PropertyId) -> bool {
        self.alive[id as usize] && self.columns[prop as usize].has(id as usize)
    }

    pub fn has_all(&self, id: u32, props: &[PropertyId]) -> bool {
        props.iter().all(|&p| self.has(id, p))
    }

    pub fn has_none(&self, id: u32, props: &[PropertyId]) -> bool {
        props.iter().all(|&p| !self.has(id, p))
    }

    pub fn vec2(&self, id: u32, prop: PropertyId) -> Option<Vec2> {
        match &self.columns[prop as usize] {
            Column::Vec2(v) => v[id as usize],
            _ => None,
        }
    }

    /// Пишет пару. В трёхмерном мире `position`, у которого сменились `x` или `y`, сразу пересаживает
    /// объект на поверхность под новым местом (`surface::seat_after_shift`), а `walk_to` без третьего
    /// числа забывает прежнее.
    pub fn set_vec2(&mut self, id: u32, prop: PropertyId, value: Vec2) {
        let previous = self.vec2(id, prop);
        if let Column::Vec2(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
        self.note_write(id, prop);
        if !self.three_d {
            return;
        }
        if prop == property::WALK_TO {
            self.walk_to_z[id as usize] = None;
        } else if prop == property::POSITION && previous.is_some_and(|old| old != value) {
            surface::seat_after_shift(self, id);
        }
    }

    /// Как `set_position_exact`, но дорога, что сама стоит концами на земле, такой и остаётся: правило
    /// двигает её без записи `z`, и концы встают на землю заново.
    pub fn set_position_keeping_grade(&mut self, id: u32, position: Vec2, z: f64) {
        if surface::is_graded_road(self, id) && !self.is_level(id) {
            self.set_vec2(id, property::POSITION, position);
        } else {
            self.set_position_exact(id, position, z);
        }
    }

    /// Ставит `position` вместе с высотой основания ровно на `z`: объект держит её, пока не сдвинется.
    pub fn set_position_exact(&mut self, id: u32, position: Vec2, z: f64) {
        if let Column::Vec2(v) = &mut self.columns[property::POSITION as usize] {
            v[id as usize] = Some(position);
        }
        self.base_z[id as usize] = z;
        self.leveled[id as usize] = true;
        self.note_write(id, property::POSITION);
    }

    /// Ставит `walk_to` вместе с высотой; без `z` — точка на верхней поверхности.
    pub fn set_walk_to(&mut self, id: u32, target: Vec2, z: Option<f64>) {
        if let Column::Vec2(v) = &mut self.columns[property::WALK_TO as usize] {
            v[id as usize] = Some(target);
        }
        self.walk_to_z[id as usize] = z;
    }

    pub fn number_like(&self, id: u32, prop: PropertyId) -> Option<f64> {
        match &self.columns[prop as usize] {
            Column::Number(v) => v[id as usize],
            Column::Time(v) | Column::Timer(v) => v[id as usize].map(|t| t as f64),
            _ => None,
        }
    }

    pub fn time(&self, id: u32, prop: PropertyId) -> Option<i64> {
        match &self.columns[prop as usize] {
            Column::Time(v) => v[id as usize],
            _ => None,
        }
    }

    /// «Свойства» → `timer`: the value in steps, still counting down toward zero — `None` when
    /// the object doesn't carry this property at all.
    pub fn timer(&self, id: u32, prop: PropertyId) -> Option<i64> {
        match &self.columns[prop as usize] {
            Column::Timer(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_number(&mut self, id: u32, prop: PropertyId, value: f64) {
        if let Column::Number(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
        self.note_write(id, prop);
    }

    pub fn set_time(&mut self, id: u32, prop: PropertyId, value: i64) {
        if let Column::Time(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    /// «Свойства» → `timer`: writes a value already in steps, clamped so it never goes below
    /// zero — `add`/`set`/a key edit/code all funnel through this rather than clamping at each
    /// call site.
    pub fn set_timer(&mut self, id: u32, prop: PropertyId, value: i64) {
        if let Column::Timer(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value.max(0));
        }
    }

    /// Stage 3: one step's worth of countdown — stays at zero once it gets there, never deletes
    /// the object.
    pub fn tick_timer(&mut self, id: u32, prop: PropertyId) {
        if let Column::Timer(v) = &mut self.columns[prop as usize]
            && let Some(cur) = v[id as usize].as_mut()
        {
            *cur = (*cur - 1).max(0);
        }
    }

    pub fn add_number_like(&mut self, id: u32, prop: PropertyId, delta: f64) {
        match &mut self.columns[prop as usize] {
            Column::Number(v) => {
                if let Some(cur) = v[id as usize].as_mut() {
                    *cur += delta;
                }
            }
            Column::Time(v) => {
                if let Some(cur) = v[id as usize].as_mut() {
                    *cur += delta.round() as i64;
                }
            }
            Column::Timer(v) => {
                if let Some(cur) = v[id as usize].as_mut() {
                    *cur = (*cur + delta.round() as i64).max(0);
                }
            }
            _ => {}
        }
        self.note_write(id, prop);
    }

    /// «Правила игры», требование 11: writes a resolved `add`/`set` number to a `number`/`time`/
    /// `timer` property, unconditionally (like `set_value`, not the "already present" guard
    /// `add_number_like` has) — dispatched by column kind, so the caller needs no `PropKind` of
    /// its own; a `time`/`timer` value must already be in steps (see `resolve_number_expr`).
    pub fn set_number_like(&mut self, id: u32, prop: PropertyId, value: f64) {
        match &mut self.columns[prop as usize] {
            Column::Number(v) => v[id as usize] = Some(value),
            Column::Time(v) => v[id as usize] = Some(value.round() as i64),
            Column::Timer(v) => v[id as usize] = Some((value.round() as i64).max(0)),
            _ => {}
        }
        self.note_write(id, prop);
    }

    pub fn flag(&self, id: u32, prop: PropertyId) -> bool {
        matches!(&self.columns[prop as usize], Column::Flag(v) if v[id as usize])
    }

    pub fn set_flag(&mut self, id: u32, prop: PropertyId, value: bool) {
        if let Column::Flag(v) = &mut self.columns[prop as usize] {
            v[id as usize] = value;
        }
        if prop == property::CAMERA_FOLLOWS && value {
            self.camera_follow_writes += 1;
        }
        self.note_write(id, prop);
    }

    pub fn camera_follow_writes(&self) -> u64 {
        self.camera_follow_writes
    }

    pub fn color(&self, id: u32, prop: PropertyId) -> Option<[f32; 4]> {
        match &self.columns[prop as usize] {
            Column::Color(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_color(&mut self, id: u32, prop: PropertyId, value: [f32; 4]) {
        if let Column::Color(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn layer(&self, id: u32, prop: PropertyId) -> Option<i32> {
        match &self.columns[prop as usize] {
            Column::Layer(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_layer(&mut self, id: u32, prop: PropertyId, value: i32) {
        if let Column::Layer(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn text(&self, id: u32, prop: PropertyId) -> Option<&str> {
        match &self.columns[prop as usize] {
            Column::Text(v) => v[id as usize].as_deref(),
            _ => None,
        }
    }

    pub fn set_text(&mut self, id: u32, prop: PropertyId, value: String) {
        if let Column::Text(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn grid(&self, id: u32, prop: PropertyId) -> Option<GridSpec> {
        match &self.columns[prop as usize] {
            Column::Grid(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_grid(&mut self, id: u32, prop: PropertyId, value: GridSpec) {
        if let Column::Grid(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn keys(&self, id: u32, prop: PropertyId) -> Option<&KeyTable> {
        match &self.columns[prop as usize] {
            Column::Keys(v) => v[id as usize].as_ref(),
            _ => None,
        }
    }

    pub fn set_keys(&mut self, id: u32, prop: PropertyId, value: KeyTable) {
        if let Column::Keys(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn image(&self, id: u32, prop: PropertyId) -> Option<ImageId> {
        match &self.columns[prop as usize] {
            Column::Image(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_image(&mut self, id: u32, prop: PropertyId, value: ImageId) {
        if let Column::Image(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn image_list(&self, id: u32, prop: PropertyId) -> Option<&[ImageId]> {
        match &self.columns[prop as usize] {
            Column::ImageList(v) => v[id as usize].as_deref(),
            _ => None,
        }
    }

    pub fn set_image_list(&mut self, id: u32, prop: PropertyId, value: Vec<ImageId>) {
        if let Column::ImageList(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn rotation(&self, id: u32, prop: PropertyId) -> Option<Rotation> {
        match &self.columns[prop as usize] {
            Column::Rotation(v) => v[id as usize],
            _ => None,
        }
    }

    /// В трёхмерном мире смена `rotation` сразу пересаживает объект, как сдвиг; объект без `rotation`
    /// стоял с нулевым поворотом.
    pub fn set_rotation(&mut self, id: u32, prop: PropertyId, value: Rotation) {
        let mut turned = false;
        if let Column::Rotation(v) = &mut self.columns[prop as usize] {
            let old = v[id as usize].unwrap_or(Rotation::from_quarters(0));
            turned = old != value;
            v[id as usize] = Some(value);
        }
        self.note_write(id, prop);
        if turned && self.three_d && prop == property::ROTATION {
            surface::seat_after_shift(self, id);
        }
    }

    pub fn follow_mouse(&self, id: u32, prop: PropertyId) -> Option<FollowAxis> {
        match &self.columns[prop as usize] {
            Column::FollowMouse(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_follow_mouse(&mut self, id: u32, prop: PropertyId, value: FollowAxis) {
        if let Column::FollowMouse(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn shape(&self, id: u32, prop: PropertyId) -> Option<Shape> {
        match &self.columns[prop as usize] {
            Column::Shape(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_shape(&mut self, id: u32, prop: PropertyId, value: Shape) {
        if let Column::Shape(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
        self.note_write(id, prop);
    }

    pub fn on_click(&self, id: u32, prop: PropertyId) -> Option<&[KeyEdit]> {
        match &self.columns[prop as usize] {
            Column::OnClick(v) => v[id as usize].as_deref(),
            _ => None,
        }
    }

    pub fn set_on_click(&mut self, id: u32, prop: PropertyId, value: Vec<KeyEdit>) {
        if let Column::OnClick(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn grid_counter(&self, id: u32) -> i64 {
        self.grid_counter[id as usize]
    }

    pub fn set_grid_counter(&mut self, id: u32, value: i64) {
        self.grid_counter[id as usize] = value;
    }

    /// Sets a property to a generically-typed constant. `value`'s variant must match the
    /// column's kind; validation at load time guarantees this.
    pub fn set_value(&mut self, id: u32, prop: PropertyId, value: &Value) {
        match value {
            Value::Flag(b) => self.set_flag(id, prop, *b),
            Value::Number(n) => self.set_number(id, prop, *n),
            Value::Time(t) => self.set_time(id, prop, *t),
            Value::Timer(t) => self.set_timer(id, prop, *t),
            Value::Vec2(v) => self.set_vec2(id, prop, *v),
            Value::Vec3([x, y, z]) => match prop {
                property::POSITION => self.set_position_exact(id, [*x, *y], *z),
                property::WALK_TO => self.set_walk_to(id, [*x, *y], Some(*z)),
                _ => self.set_vec2(id, prop, [*x, *y]),
            },
            Value::Color(c) => self.set_color(id, prop, *c),
            Value::Layer(l) => self.set_layer(id, prop, *l),
            Value::Text(s) => self.set_text(id, prop, s.clone()),
            Value::Image(i) => self.set_image(id, prop, *i),
            Value::ImageList(list) => self.set_image_list(id, prop, list.clone()),
            Value::Rotation(r) => self.set_rotation(id, prop, *r),
            Value::FollowMouse(a) => self.set_follow_mouse(id, prop, *a),
            Value::Shape(s) => self.set_shape(id, prop, *s),
        }
    }

    pub fn get_value(&self, id: u32, prop: PropertyId, kind: PropKind) -> Option<Value> {
        match kind {
            PropKind::Flag => Some(Value::Flag(self.flag(id, prop))),
            PropKind::Number => self.number_like(id, prop).map(Value::Number),
            PropKind::Time => self.time(id, prop).map(Value::Time),
            PropKind::Timer => self.timer(id, prop).map(Value::Timer),
            PropKind::Vec2 => self.vec2(id, prop).map(|xy| match self.placed_z(id, prop) {
                Some(z) => Value::Vec3([xy[0], xy[1], z]),
                None => Value::Vec2(xy),
            }),
            PropKind::Color => self.color(id, prop).map(Value::Color),
            PropKind::Layer => self.layer(id, prop).map(Value::Layer),
            PropKind::Text => self.text(id, prop).map(|s| Value::Text(s.to_string())),
            PropKind::Image => self.image(id, prop).map(Value::Image),
            PropKind::ImageList => self
                .image_list(id, prop)
                .map(|list| Value::ImageList(list.to_vec())),
            PropKind::Rotation => self.rotation(id, prop).map(Value::Rotation),
            PropKind::FollowMouse => self.follow_mouse(id, prop).map(Value::FollowMouse),
            PropKind::Shape => self.shape(id, prop).map(Value::Shape),
            PropKind::Grid | PropKind::Keys | PropKind::OnClick => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::{self, PropertyTable};

    #[test]
    fn create_then_delete_frees_slot_for_reuse() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let a = world.create();
        world.set_vec2(a, property::POSITION, [1.0, 2.0]);
        world.delete(a);
        assert!(!world.is_alive(a));
        let b = world.create();
        assert_eq!(a, b, "freed slot is reused");
        assert_eq!(
            world.vec2(b, property::POSITION),
            None,
            "stale value is gone"
        );
    }

    #[test]
    fn delete_past_slot_count_is_ignored_not_a_panic() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        world.delete(999);
        assert!(!world.is_alive(999));
    }

    #[test]
    fn presence_is_independent_per_property() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let a = world.create();
        world.set_vec2(a, property::POSITION, [0.0, 0.0]);
        assert!(world.has(a, property::POSITION));
        assert!(!world.has(a, property::VELOCITY));
    }
}

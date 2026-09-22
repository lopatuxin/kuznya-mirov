//! «Код игры»: исполнитель кода игры на Lua поверх `luars`.
//!
//! Один [`Runner`] живёт одну партию: `new_game` строит его заново из текста файла, `quit`
//! выбрасывает вместе с миром. Между вызовами он держит песочницу Lua и объявленные ею функции;
//! [`Runner::run`] вызывает одну из них с объектами-аргументами по месту вызова правила.
//!
//! Объекты и пары в коде — userdata `luars` ([`ObjRef`]/[`PairRef`]) с общей метатаблицей,
//! строятся заново на каждый возврат в Lua через собственные `IntoLua`. Не таблицы — решение
//! «Объект и пара в коде — userdata `luars`, а не таблица» (Журнал Кузни, 2026-09-22): `type` их
//! называет `userdata`, не `table`. Ни одно зарегистрированное здесь замыкание не захватывает
//! `LuaTable`/`LuaFunction` (или их контейнер) по значению: у этих типов `Drop` обращается назад
//! в `GlobalState` того же `Lua`, а замыкания сами хранятся внутри него же — получилась бы такая
//! пара, чей порядок разрушения `Lua` никаким порядком полей уже не выправить. Держать разрешено
//! только copy-значения (`LuaValue`) и `Rc` обычных, не-Lua данных.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use luars::{
    FromLua, IntoLua, Lua, LuaApi, LuaError, LuaFunction, LuaResult, LuaSandboxApi, LuaState,
    LuaTable, LuaUserdata, LuaValue, SafeOption, SandboxConfig, Stdlib, UserDataTrait,
    lua_float_to_string,
};

use super::property::{self, PropertyId, PropertyTable};
use super::rng::Rng;
use super::sound::SoundMarks;
use super::value::{PropKind, Rotation};
use super::world::World;

/// «Код игры»: предел операций Lua — на прогон файла при загрузке (свой, отдельный бюджет) и на
/// все вызовы `run` одного шага партии вместе (общий бюджет). Своя копия `luars` держит этот
/// остаток прямо в `LuaState` и списывает с него на каждой операции для всех вызовов
/// `run`/`execute_sandboxed` вместе, пока хозяин не выставит бюджет заново (`Lua::
/// set_instruction_budget`/`instruction_budget_remaining` — публичные методы `LuaSandboxApi`) —
/// `pcall` его не сбрасывает и не обходит. `SandboxConfig::instruction_limit` остаётся отдельным,
/// per-call пределом одного вызова `execute_sandboxed`, как у автора, и здесь не используется.
/// Бюджет выставляется в `build` перед прогоном верхнего уровня файла и в
/// `Runner::reset_step_budget` в начале каждого шага партии.
const INSTRUCTION_LIMIT: u64 = 1_000_000;
/// «Код игры»: предел памяти исполнителя — на весь его срок жизни, не на отдельный вызов
/// (`SafeOption::max_memory_limit`, а не временный `SandboxConfig::memory_limit_bytes`).
const MEMORY_LIMIT_BYTES: isize = 16 * 1024 * 1024;

/// Имена, которые уже есть в свежей песочнице до того, как файл игры объявит свои — «Код игры»:
/// первое условие проверки «объявлено и не используется» отличает объявленное файлом от
/// встроенного по этому списку.
const RESERVED_GLOBALS: &[&str] = &[
    "_VERSION",
    "assert",
    "error",
    "getmetatable",
    "ipairs",
    "next",
    "pairs",
    "pcall",
    "print",
    "rawequal",
    "rawget",
    "rawlen",
    "rawset",
    "select",
    "setmetatable",
    "tonumber",
    "tostring",
    "type",
    "warn",
    "xpcall",
    "math",
    "string",
    "table",
    "utf8",
    "find",
    "delete",
    "play_sound",
    "_G",
    "_ENV",
];

/// Одна ошибка кода — при проверке перед запуском или во время партии. `line` — строка внутри
/// файла кода, когда её удалось разобрать из сообщения Lua. `function`, `rule` и `step` — только
/// у ошибки во время партии («Код игры» → требование 20: текст называет функцию, правило и шаг);
/// `function` заполняет `Runner::run` (единственное место, знающее вызванную функцию), `rule` —
/// `step::run_code` (знает индекс правила), `step` — `Game::step` (знает номер шага). Ошибка
/// проверки перед запуском ни одно из трёх не заполняет — до партии ни правила, ни шага ещё нет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeError {
    pub message: String,
    pub line: Option<u32>,
    pub function: Option<String>,
    pub rule: Option<String>,
    pub step: Option<u64>,
}

/// Raw-указатель, общий на все зарегистрированные замыкания Lua — единственный способ дать им
/// доступ к меняющемуся от вызова к вызову хостовому состоянию: типизированные функции `luars`
/// `'static` и не получают никакого хендла на исполнитель сами. Выставляется прямо перед вызовом
/// Lua и снимается сразу после — см. `set_ctx`/`restore_ctx`.
type CtxCell = Rc<Cell<*mut ()>>;

/// Жив всё время, пока исполняется любой код в этом исполнителе — включая проверку при загрузке.
struct BaseCtx<'a> {
    rng: &'a mut Rng,
    messages: &'a mut Vec<String>,
}

/// Жив только пока по-настоящему исполняется `run` во время шага — во время проверки при загрузке
/// его нет, этим и объясняется «мира ещё нет» у `find`/`delete`/`play_sound` на верхнем уровне.
struct WorldCtx<'a> {
    world: &'a mut World,
    deletes: &'a mut Vec<u32>,
    moved: &'a mut [bool],
    marks: SoundMarks<'a>,
}

fn base_ptr(cell: &CtxCell) -> Result<*mut BaseCtx<'static>, String> {
    let ptr = cell.get();
    if ptr.is_null() {
        Err("исполнитель кода не готов".to_string())
    } else {
        Ok(ptr as *mut BaseCtx<'static>)
    }
}

fn world_ptr(cell: &CtxCell) -> Result<*mut WorldCtx<'static>, String> {
    let ptr = cell.get();
    if ptr.is_null() {
        Err("мира ещё нет".to_string())
    } else {
        Ok(ptr as *mut WorldCtx<'static>)
    }
}

fn set_ctx<T>(cell: &CtxCell, ctx: &mut T) -> *mut () {
    let prev = cell.get();
    cell.set(ctx as *mut T as *mut ());
    prev
}

fn restore_ctx(cell: &CtxCell, prev: *mut ()) {
    cell.set(prev);
}

fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let s = s.strip_prefix('#')?;
    let component = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    match s.len() {
        6 => Some([
            component(0)? as f32 / 255.0,
            component(2)? as f32 / 255.0,
            component(4)? as f32 / 255.0,
            1.0,
        ]),
        8 => Some([
            component(0)? as f32 / 255.0,
            component(2)? as f32 / 255.0,
            component(4)? as f32 / 255.0,
            component(6)? as f32 / 255.0,
        ]),
        _ => None,
    }
}

fn format_hex_color(c: [f32; 4]) -> String {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if c[3] >= 1.0 {
        format!("#{:02x}{:02x}{:02x}", to_u8(c[0]), to_u8(c[1]), to_u8(c[2]))
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            to_u8(c[0]),
            to_u8(c[1]),
            to_u8(c[2]),
            to_u8(c[3])
        )
    }
}

fn seconds_to_steps_delta(seconds: f64) -> i64 {
    (seconds * 60.0).round() as i64
}

fn require_integer(n: f64) -> Result<i32, String> {
    if n.fract() != 0.0 {
        return Err(format!("layer должен быть целым числом, получено {n}"));
    }
    Ok(n as i32)
}

/// Объект в коде: номер слота и его поколение — по ним каждое обращение проверяет, что объект
/// ещё жив (`live_object`), а не занял ли его слот уже другой. Без `Clone`, как и [`PairRef`]:
/// у `luars` есть общий `FromLua` для любого `UserDataTrait + Clone` с английским текстом
/// ошибки, а свой `FromLua` ниже даёт русский, как у остальных ошибок кода.
#[derive(PartialEq, Eq)]
struct ObjRef {
    id: u32,
    generation: u32,
}

/// Пара, прочитанная у объекта: запись в её `x`/`y` меняет свойство `prop` самого объекта.
struct PairRef {
    id: u32,
    generation: u32,
    prop: PropertyId,
}

impl UserDataTrait for ObjRef {
    fn type_name(&self) -> &'static str {
        "object"
    }

    /// «Код игры»: два обработчика одного объекта равны; объект не равен своей же паре и
    /// никакому другому значению.
    fn lua_eq(&self, other: &dyn UserDataTrait) -> Option<bool> {
        Some(other.as_any().downcast_ref::<ObjRef>() == Some(self))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl UserDataTrait for PairRef {
    fn type_name(&self) -> &'static str {
        "pair"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Вид значения для текста ошибки: объект и пару — по-русски, а не общим `userdata`.
fn kind_name(value: LuaValue) -> &'static str {
    match value.as_userdata_mut() {
        Some(ud) if ud.downcast_ref::<ObjRef>().is_some() => "объект",
        Some(ud) if ud.downcast_ref::<PairRef>().is_some() => "пара",
        _ => value.type_name(),
    }
}

fn userdata_arg<T: UserDataTrait, R>(
    value: LuaValue,
    what: &str,
    read: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    value
        .as_userdata_mut()
        .and_then(|ud| ud.downcast_ref::<T>())
        .map(read)
        .ok_or_else(|| format!("ожидался {what}, получено {}", kind_name(value)))
}

impl ObjRef {
    fn from_value(value: LuaValue) -> Result<Self, String> {
        userdata_arg(value, "объект", |obj: &ObjRef| ObjRef {
            id: obj.id,
            generation: obj.generation,
        })
    }
}

impl FromLua for ObjRef {
    fn from_lua(value: LuaValue, _state: &mut LuaState) -> Result<Self, String> {
        ObjRef::from_value(value)
    }
}

impl PairRef {
    fn from_value(value: LuaValue) -> Result<Self, String> {
        userdata_arg(value, "пара", |pair: &PairRef| PairRef {
            id: pair.id,
            generation: pair.generation,
            prop: pair.prop,
        })
    }
}

impl FromLua for PairRef {
    fn from_lua(value: LuaValue, _state: &mut LuaState) -> Result<Self, String> {
        PairRef::from_value(value)
    }
}

/// Строит userdata с общей метатаблицей — единственное место, откуда когда-либо появляется
/// объект или пара в Lua. Требует `state`, поэтому вызывается только из `IntoLua::into_lua`,
/// где он на секунду доступен.
fn new_handle<T: UserDataTrait>(
    state: &mut LuaState,
    data: T,
    mt: LuaValue,
) -> Result<LuaValue, String> {
    let mt = mt
        .as_table_ptr()
        .ok_or_else(|| "внутренняя ошибка исполнителя кода: нет метатаблицы".to_string())?;
    state
        .create_userdata(LuaUserdata::with_metatable(data, mt))
        .map_err(|e| format!("{e:?}"))
}

/// Обработчик живого объекта на выходе в Lua — «Код игры»: строится заново при каждом
/// возвращении объекта (из `find`, из вызова `run`), не переиспользуется.
struct ObjHandle {
    obj: ObjRef,
    mt: LuaValue,
}

impl IntoLua for ObjHandle {
    fn into_lua(self, state: &mut LuaState) -> Result<usize, String> {
        let value = new_handle(state, self.obj, self.mt)?;
        state.push_value(value).map_err(|e| format!("{e:?}"))?;
        Ok(1)
    }
}

/// Значение, читаемое кодом, до того как оно попадёт в Lua — «Код игры»: строку и пару можно
/// завернуть в `LuaValue`, только зная исполнитель (`state.create_string`/`create_userdata`), а
/// простые замыкания `luars` этого хендла не получают, только само преобразование на выходе.
enum PropValue {
    Nil,
    Bool(bool),
    Number(f64),
    Str(String),
    Vec2 { pair: PairRef, mt: LuaValue },
}

impl IntoLua for PropValue {
    fn into_lua(self, state: &mut LuaState) -> Result<usize, String> {
        match self {
            PropValue::Nil => Option::<bool>::None.into_lua(state),
            PropValue::Bool(b) => b.into_lua(state),
            PropValue::Number(n) => n.into_lua(state),
            PropValue::Str(s) => s.into_lua(state),
            PropValue::Vec2 { pair, mt } => {
                let value = new_handle(state, pair, mt)?;
                state.push_value(value).map_err(|e| format!("{e:?}"))?;
                Ok(1)
            }
        }
    }
}

/// Разобранное входное значение записи — пару Lua отдаёт либо таблицей `{x = .., y = ..}`, чьи
/// поля читаются только через `state`, либо парой, прочитанной у объекта (`b.position =
/// a.position`), чьё значение лежит в мире и берётся уже в `write_property`; так что разбор —
/// собственный `FromLua`, а не общий по `LuaValue`.
enum AnyValue {
    Nil,
    Bool(bool),
    Number(f64),
    Str(String),
    Vec2 { x: f64, y: f64 },
    Pair(PairRef),
}

impl FromLua for AnyValue {
    fn from_lua(value: LuaValue, state: &mut LuaState) -> Result<Self, String> {
        if value.is_nil() {
            return Ok(AnyValue::Nil);
        }
        if let Some(b) = value.as_boolean() {
            return Ok(AnyValue::Bool(b));
        }
        if let Some(i) = value.as_integer() {
            return Ok(AnyValue::Number(i as f64));
        }
        if let Some(n) = value.as_number() {
            return Ok(AnyValue::Number(n));
        }
        if let Some(s) = value.as_str() {
            return Ok(AnyValue::Str(s.to_string()));
        }
        if let Some(table) = state.to_table_ref(value) {
            let x: f64 = table
                .get_typed("x")
                .map_err(|_| "пара без числового x".to_string())?;
            let y: f64 = table
                .get_typed("y")
                .map_err(|_| "пара без числового y".to_string())?;
            return Ok(AnyValue::Vec2 { x, y });
        }
        if let Ok(pair) = PairRef::from_value(value) {
            return Ok(AnyValue::Pair(pair));
        }
        Err(format!("неожиданное значение вида {}", kind_name(value)))
    }
}

/// Обёртка над таблицей-аргументом `find{has=..., without=...}` — заворачивает
/// `state.to_table_ref` в `FromLua`, не переживает вызов, и потому безопасна как параметр
/// замыкания (в отличие от `LuaTable`, которую то же замыкание захватило бы по значению).
struct TableArg(luars::LuaTableRef);

impl FromLua for TableArg {
    fn from_lua(value: LuaValue, state: &mut LuaState) -> Result<Self, String> {
        state
            .to_table_ref(value)
            .map(TableArg)
            .ok_or_else(|| format!("ожидалась таблица, получено {}", value.type_name()))
    }
}

/// Живой ли ещё обработчик объекта — «объект удалён» это ошибка на любое обращение к
/// устаревшему хендлу, каким бы свойством оно ни было.
fn live_object(world: &World, obj: &ObjRef) -> Result<u32, String> {
    if world.is_alive(obj.id) && world.generation(obj.id) == obj.generation {
        Ok(obj.id)
    } else {
        Err("объект удалён".to_string())
    }
}

fn resolve_property(properties: &PropertyTable, name: &str) -> Result<PropertyId, String> {
    properties
        .resolve(name)
        .ok_or_else(|| format!("неизвестное свойство \"{name}\""))
}

fn read_property(
    world: &World,
    properties: &PropertyTable,
    image_names: &[String],
    id: u32,
    prop: PropertyId,
    vec2_mt: LuaValue,
) -> Result<PropValue, String> {
    match properties.kind(prop) {
        PropKind::Flag => Ok(PropValue::Bool(world.flag(id, prop))),
        PropKind::Number => Ok(world
            .number_like(id, prop)
            .map_or(PropValue::Nil, PropValue::Number)),
        PropKind::Time => Ok(world.time(id, prop).map_or(PropValue::Nil, |steps| {
            PropValue::Number(steps as f64 / 60.0)
        })),
        // «Код игры» → пункт 28: `timer` читается в секундах, как `time`.
        PropKind::Timer => Ok(world.timer(id, prop).map_or(PropValue::Nil, |steps| {
            PropValue::Number(steps as f64 / 60.0)
        })),
        PropKind::Rotation => Ok(world
            .rotation(id, prop)
            .map_or(PropValue::Nil, |r| PropValue::Number(r.degrees() as f64))),
        PropKind::Layer => Ok(world
            .layer(id, prop)
            .map_or(PropValue::Nil, |l| PropValue::Number(l as f64))),
        PropKind::Text => Ok(world
            .text(id, prop)
            .map_or(PropValue::Nil, |s| PropValue::Str(s.to_string()))),
        PropKind::Color => Ok(world
            .color(id, prop)
            .map_or(PropValue::Nil, |c| PropValue::Str(format_hex_color(c)))),
        PropKind::Image => Ok(
            match world.image(id, prop).and_then(|i| image_names.get(i)) {
                Some(name) => PropValue::Str(name.clone()),
                None => PropValue::Nil,
            },
        ),
        PropKind::Vec2 => {
            if world.vec2(id, prop).is_none() {
                return Ok(PropValue::Nil);
            }
            Ok(PropValue::Vec2 {
                pair: PairRef {
                    id,
                    generation: world.generation(id),
                    prop,
                },
                mt: vec2_mt,
            })
        }
        // «Код игры» → пункт 28: `follow_mouse` недоступен коду, как `keys` и `grid`.
        PropKind::Grid | PropKind::Keys | PropKind::FollowMouse => Err(format!(
            "свойство \"{}\" недоступно коду",
            properties.name(prop)
        )),
    }
}

fn write_property(
    world: &mut World,
    properties: &PropertyTable,
    image_names: &[String],
    id: u32,
    prop: PropertyId,
    value: AnyValue,
    moved: &mut [bool],
) -> Result<(), String> {
    let kind = properties.kind(prop);
    if matches!(
        kind,
        PropKind::Grid | PropKind::Keys | PropKind::FollowMouse
    ) {
        return Err(format!(
            "свойство \"{}\" недоступно коду",
            properties.name(prop)
        ));
    }
    let value = match value {
        AnyValue::Nil => {
            world.clear_property(id, prop);
            return Ok(());
        }
        AnyValue::Pair(pair) => {
            let (_, [x, y]) = live_pair(world, &pair)?;
            AnyValue::Vec2 { x, y }
        }
        other => other,
    };
    match (kind, value) {
        (PropKind::Flag, AnyValue::Bool(b)) => {
            world.set_flag(id, prop, b);
            Ok(())
        }
        (PropKind::Flag, _) => Err("ожидался признак (true/false/nil)".to_string()),
        (PropKind::Number, AnyValue::Number(n)) => {
            world.set_number(id, prop, n);
            Ok(())
        }
        (PropKind::Number, _) => Err("ожидалось число".to_string()),
        (PropKind::Time, AnyValue::Number(seconds)) => {
            world.set_time(id, prop, seconds_to_steps_delta(seconds));
            Ok(())
        }
        (PropKind::Time, _) => Err("ожидалось число секунд".to_string()),
        // «Код игры» → пункт 28: `timer` пишется секундами, как `time`; отрицательное значение
        // становится нулём — `World::set_timer` уже так и клампит.
        (PropKind::Timer, AnyValue::Number(seconds)) => {
            world.set_timer(id, prop, seconds_to_steps_delta(seconds));
            Ok(())
        }
        (PropKind::Timer, _) => Err("ожидалось число секунд".to_string()),
        (PropKind::Rotation, AnyValue::Number(degrees)) => {
            match Rotation::from_degrees_exact(degrees) {
                Some(r) => {
                    world.set_rotation(id, prop, r);
                    Ok(())
                }
                None => Err(format!(
                    "rotation должен быть 0, 90, 180 или 270, получено {degrees}"
                )),
            }
        }
        (PropKind::Rotation, _) => Err("ожидалось число (0, 90, 180 или 270)".to_string()),
        (PropKind::Layer, AnyValue::Number(n)) => {
            world.set_layer(id, prop, require_integer(n)?);
            Ok(())
        }
        (PropKind::Layer, _) => Err("ожидалось целое число".to_string()),
        (PropKind::Text, AnyValue::Str(s)) => {
            world.set_text(id, prop, s);
            Ok(())
        }
        (PropKind::Text, _) => Err("ожидалась строка".to_string()),
        (PropKind::Color, AnyValue::Str(s)) => match parse_hex_color(&s) {
            Some(c) => {
                world.set_color(id, prop, c);
                Ok(())
            }
            None => Err(format!(
                "цвет должен быть вида \"#rrggbb\" или \"#rrggbbaa\", получено \"{s}\""
            )),
        },
        (PropKind::Color, _) => Err("ожидалась строка цвета".to_string()),
        (PropKind::Image, AnyValue::Str(name)) => match image_names.iter().position(|n| *n == name)
        {
            Some(image_id) => {
                world.set_image(id, prop, image_id);
                Ok(())
            }
            None => {
                let declared = image_names
                    .iter()
                    .map(|n| format!("\"{n}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!("картинки \"{name}\" нет; объявлены {declared}"))
            }
        },
        (PropKind::Image, _) => Err("ожидалось имя картинки строкой".to_string()),
        (PropKind::Vec2, AnyValue::Vec2 { x, y }) => {
            world.set_vec2(id, prop, [x, y]);
            if prop == property::POSITION
                && let Some(slot) = moved.get_mut(id as usize)
            {
                *slot = true;
            }
            Ok(())
        }
        (PropKind::Vec2, _) => Err("ожидалась пара {x=.., y=..}".to_string()),
        (PropKind::Grid | PropKind::Keys | PropKind::FollowMouse, _) => {
            unreachable!("checked above")
        }
    }
}

/// Всё, что не меняется всю партию и разделяется всеми зарегистрированными замыканиями —
/// `Rc<T>` обычных данных и copy-значения `LuaValue`, ничего, что захватывало бы `LuaTable`/
/// `LuaFunction` по значению (см. заметку в начале файла).
#[derive(Clone)]
struct Env {
    properties: Rc<PropertyTable>,
    image_names: Rc<Vec<String>>,
    sound_names: Rc<Vec<String>>,
    obj_mt: LuaValue,
    vec2_mt: LuaValue,
}

/// «Код игры»: `_ENV` и `_G` кода — userdata без собственных полей, а не пустая прокси-таблица —
/// решение «`_ENV` и `_G` кода игры — userdata, а не пустая таблица» (Журнал Кузни, 2026-09-22):
/// `type(_G)` даёт `userdata`, `rawget`/`next`/`#` на `_G` — ошибка, а не пустой ответ.
struct Globals;

impl UserDataTrait for Globals {
    fn type_name(&self) -> &'static str {
        "globals"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// «Код игры»: `rawset` не смотрит на `__newindex` — `rawset(env, 'x', 1)` писал бы в настоящий
/// стол глобальных мимо ловушки в обход запрета «запись в глобальную переменную из функции».
/// Замыкание отказывает тем же текстом, когда первый аргумент — прокси `_ENV`/`_G` (сам `rawset`
/// userdata не принял бы, но с чужим текстом) или настоящее окружение, а для любой другой таблицы
/// (свои собственные таблицы кода) работает как обычный `rawset`. Никакой Lua-обёртки между этим
/// замыканием и game-кодом больше нет — место у ошибки «table expected» и любой другой отсюда
/// потому остаётся строкой самого game-кода: `luars` берёт для ошибки C-функции место кадра,
/// который её вызвал, а вызывающий здесь — сам вызов `rawset(...)` в файле игры, не промежуточная
/// Lua-функция.
fn install_protected_rawset(lua: &mut Lua, env: &LuaTable, proxy_env: LuaValue) -> LuaResult<()> {
    // SAFETY: `env` stays alive on `Runner` for the whole party (it is `Runner::env`); the copy
    // is only compared for identity inside a closure that only runs while that same `Runner`
    // (and so `lua`, and so this table) is alive — see the note at the top of this file on why a
    // `LuaValue` copy is used here instead of holding `LuaTable` itself.
    let real_env = unsafe { env.to_value() };
    let wrapped = lua.create_function(
        move |table: LuaValue, key: LuaValue, value: LuaValue| -> Result<ProtectedRawSet, String> {
            if table == real_env || table == proxy_env {
                return Err(format!(
                    "запись в глобальную переменную из функции: {}",
                    tostring_like(&key)
                ));
            }
            Ok(ProtectedRawSet { table, key, value })
        },
    )?;
    env.set("rawset", wrapped)
}

/// A `rawset` call's arguments, once known not to target a protected environment — checked and
/// applied here, not in the closure `install_protected_rawset` registers, because that needs
/// `&mut LuaState` (`LuaState::to_table_ref`), which this file only ever gets through
/// `FromLua`/`IntoLua` (see the note at the top of this file); an error from here still names the
/// same calling line as one from that closure, since `into_lua` runs within the same call.
struct ProtectedRawSet {
    table: LuaValue,
    key: LuaValue,
    value: LuaValue,
}

impl IntoLua for ProtectedRawSet {
    fn into_lua(self, state: &mut LuaState) -> Result<usize, String> {
        let table = state
            .to_table_ref(self.table)
            .ok_or_else(|| "bad argument #1 to 'rawset' (table expected)".to_string())?;
        if self.key.as_number().is_some_and(f64::is_nan) {
            return Err("table index is NaN".to_string());
        }
        table
            .rawset_typed(self.key, self.value)
            .map_err(|e| format!("{e:?}"))?;
        self.table.into_lua(state)
    }
}

fn install_object_metatable(
    lua: &mut Lua,
    world_cell: &CtxCell,
    env: &Env,
    mt: &LuaTable,
) -> LuaResult<()> {
    let index_env = env.clone();
    let index_cell = world_cell.clone();
    let index_fn = lua.create_function(
        move |obj: ObjRef, key: String| -> Result<PropValue, String> {
            let ptr = world_ptr(&index_cell)?;
            // SAFETY: `Runner::run` sets this pointer to a live `WorldCtx` right before calling
            // into Lua and clears it right after; Lua is single-threaded, so this closure only
            // ever runs synchronously inside that span.
            let ctx = unsafe { &mut *ptr };
            let id = live_object(ctx.world, &obj)?;
            let prop = resolve_property(&index_env.properties, &key)?;
            read_property(
                ctx.world,
                &index_env.properties,
                &index_env.image_names,
                id,
                prop,
                index_env.vec2_mt,
            )
        },
    )?;
    mt.set("__index", index_fn)?;

    let newindex_env = env.clone();
    let newindex_cell = world_cell.clone();
    let newindex_fn = lua.create_function(
        move |obj: ObjRef, key: String, value: AnyValue| -> Result<(), String> {
            let ptr = world_ptr(&newindex_cell)?;
            // SAFETY: see `index_fn` above — same call-scoped pointer.
            let ctx = unsafe { &mut *ptr };
            let id = live_object(ctx.world, &obj)?;
            let prop = resolve_property(&newindex_env.properties, &key)?;
            write_property(
                ctx.world,
                &newindex_env.properties,
                &newindex_env.image_names,
                id,
                prop,
                value,
                ctx.moved,
            )
        },
    )?;
    mt.set("__newindex", newindex_fn)?;

    Ok(())
}

/// Номер объекта-владельца пары, если он ещё жив, и сама пара — ошибка, когда объект удалён
/// или код уже убрал у него это свойство (`obj.velocity = nil`).
fn live_pair(world: &World, pair: &PairRef) -> Result<(u32, [f64; 2]), String> {
    let id = live_object(
        world,
        &ObjRef {
            id: pair.id,
            generation: pair.generation,
        },
    )?;
    let value = world
        .vec2(id, pair.prop)
        .ok_or_else(|| "у объекта больше нет этой пары".to_string())?;
    Ok((id, value))
}

fn install_vec2_metatable(lua: &mut Lua, world_cell: &CtxCell, mt: &LuaTable) -> LuaResult<()> {
    let index_cell = world_cell.clone();
    let index_fn =
        lua.create_function(move |pair: PairRef, key: String| -> Result<f64, String> {
            let ptr = world_ptr(&index_cell)?;
            // SAFETY: see `install_object_metatable` — same call-scoped pointer discipline.
            let ctx = unsafe { &mut *ptr };
            let (_, value) = live_pair(ctx.world, &pair)?;
            match key.as_str() {
                "x" => Ok(value[0]),
                "y" => Ok(value[1]),
                other => Err(format!("у пары нет поля \"{other}\"")),
            }
        })?;
    mt.set("__index", index_fn)?;

    let newindex_cell = world_cell.clone();
    let newindex_fn = lua.create_function(
        move |pair: PairRef, key: String, value: LuaValue| -> Result<(), String> {
            let ptr = world_ptr(&newindex_cell)?;
            // SAFETY: see `install_object_metatable` — same call-scoped pointer discipline.
            let ctx = unsafe { &mut *ptr };
            let value = value
                .as_number()
                .ok_or_else(|| format!("у пары x и y — числа, получено {}", kind_name(value)))?;
            let (id, mut current) = live_pair(ctx.world, &pair)?;
            match key.as_str() {
                "x" => current[0] = value,
                "y" => current[1] = value,
                other => return Err(format!("у пары нет поля \"{other}\"")),
            }
            ctx.world.set_vec2(id, pair.prop, current);
            if pair.prop == property::POSITION
                && let Some(slot) = ctx.moved.get_mut(id as usize)
            {
                *slot = true;
            }
            Ok(())
        },
    )?;
    mt.set("__newindex", newindex_fn)?;

    Ok(())
}

/// Результат `find{...}` — собственный `IntoLua`: единственное место, где обычное замыкание
/// всё же получает `&mut LuaState` — на выходе, когда значение уже готово и осталось протолкнуть
/// его в Lua (см. `new_handle`).
struct FoundList(Vec<ObjHandle>);

impl IntoLua for FoundList {
    fn into_lua(self, state: &mut LuaState) -> Result<usize, String> {
        let arr = state
            .create_table_ref(self.0.len(), 0)
            .map_err(|e| format!("{e:?}"))?;
        for (i, handle) in self.0.into_iter().enumerate() {
            let value = new_handle(state, handle.obj, handle.mt)?;
            arr.rawseti_typed((i + 1) as i64, value)
                .map_err(|e| format!("{e:?}"))?;
        }
        state
            .push_value(arr.to_value())
            .map_err(|e| format!("{e:?}"))?;
        Ok(1)
    }
}

fn find_selector(
    table: Option<TableArg>,
    properties: &PropertyTable,
) -> Result<(Vec<PropertyId>, Vec<PropertyId>), String> {
    let Some(table) = table else {
        return Ok((Vec::new(), Vec::new()));
    };
    let read_list = |key: &str| -> Result<Vec<PropertyId>, String> {
        let Ok(sub) = table.0.get_typed::<_, TableArg>(key) else {
            return Ok(Vec::new());
        };
        let names: Vec<String> = sub.0.sequence_values().map_err(|e| format!("{e:?}"))?;
        names
            .iter()
            .map(|n| resolve_property(properties, n))
            .collect()
    };
    Ok((read_list("has")?, read_list("without")?))
}

fn install_find(lua: &mut Lua, world_cell: &CtxCell, env: &Env) -> LuaResult<LuaFunction> {
    let env = env.clone();
    let world_cell = world_cell.clone();
    lua.create_function(
        move |selector: Option<TableArg>| -> Result<FoundList, String> {
            let ptr = world_ptr(&world_cell)?;
            // SAFETY: see `install_object_metatable` — same call-scoped pointer discipline.
            let ctx = unsafe { &mut *ptr };
            let (has, without) = find_selector(selector, &env.properties)?;
            let mut out = Vec::new();
            for id in ctx.world.ids() {
                if ctx.world.has_all(id, &has) && ctx.world.has_none(id, &without) {
                    out.push(ObjHandle {
                        obj: ObjRef {
                            id,
                            generation: ctx.world.generation(id),
                        },
                        mt: env.obj_mt,
                    });
                }
            }
            Ok(FoundList(out))
        },
    )
}

fn install_delete(lua: &mut Lua, world_cell: &CtxCell) -> LuaResult<LuaFunction> {
    let world_cell = world_cell.clone();
    // «Код игры»: мир проверяется раньше аргумента — на верхнем уровне файла объекта взять
    // неоткуда, и `delete(...)` там с любым аргументом — «мира ещё нет».
    lua.create_function(move |value: LuaValue| -> Result<(), String> {
        let ptr = world_ptr(&world_cell)?;
        let obj = ObjRef::from_value(value)?;
        // SAFETY: see `install_object_metatable` — same call-scoped pointer discipline.
        let ctx = unsafe { &mut *ptr };
        let id = live_object(ctx.world, &obj)?;
        if !ctx.deletes.contains(&id) {
            ctx.deletes.push(id);
        }
        Ok(())
    })
}

fn install_play_sound(
    lua: &mut Lua,
    world_cell: &CtxCell,
    sound_names: Rc<Vec<String>>,
) -> LuaResult<LuaFunction> {
    let world_cell = world_cell.clone();
    lua.create_function(move |name: String| -> Result<(), String> {
        let ptr = world_ptr(&world_cell)?;
        // SAFETY: see `install_object_metatable` — same call-scoped pointer discipline.
        let ctx = unsafe { &mut *ptr };
        match sound_names.iter().position(|n| *n == name) {
            Some(id) => {
                ctx.marks.raise(id);
                Ok(())
            }
            None => {
                let declared = sound_names
                    .iter()
                    .map(|n| format!("\"{n}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!("звука \"{name}\" нет; объявлены {declared}"))
            }
        }
    })
}

/// «Код игры»: те же строки, что `tostring` без `__tostring` — ничего из того, что попадает в
/// код игры, его не определяет — но не через сам `Display` `LuaValue`: для чисел с плавающей
/// точкой он расходится с `tostring` (`NaN`/полное разложение больших степеней десяти вместо
/// `-nan`/`1e300` с точкой). Числа поэтому идут через `lua_float_to_string`, тот же форматтер,
/// что использует `tostring` библиотеки; остальные виды значений — через `Display`, который для
/// них с `tostring` совпадает.
fn tostring_like(value: &LuaValue) -> String {
    if value.is_float() {
        return lua_float_to_string(value.as_number().expect("is_float"));
    }
    value.to_string()
}

/// «Код игры»: печатает через табуляцию, как обычный Lua, с любым числом аргументов, включая
/// `nil` посередине — `Fn(Vec<LuaValue>)` видит настоящее число переданных аргументов (в отличие
/// от фиксированной арности, где отсутствующий и переданный `nil` неразличимы). Каждый аргумент
/// идёт через `tostring_like`, а не через настоящий `tostring` как значение из `get_global`:
/// `LuaFunction`, захваченная здесь по значению, была бы ровно тем захватом, что запрещает
/// заметка в начале файла — её `Drop` обращается назад в `GlobalState`, а сама она хранилась бы
/// внутри него же, в этом самом замыкании.
fn install_print(lua: &mut Lua, base_cell: &CtxCell) -> LuaResult<LuaFunction> {
    let base_cell = base_cell.clone();
    lua.create_function(move |args: Vec<LuaValue>| -> Result<(), String> {
        let ptr = base_ptr(&base_cell)?;
        // SAFETY: `Runner::compile`/`Runner::run` set this pointer for the whole span of the
        // call into Lua and clear it right after; Lua is single-threaded.
        let ctx = unsafe { &mut *ptr };
        let parts: Vec<String> = args.iter().map(tostring_like).collect();
        ctx.messages.push(format!("print: {}", parts.join("\t")));
        Ok(())
    })
}

/// «Код игры»: без аргументов `math.random` — дробное число, с аргументами — Lua integer
/// (`tostring(math.random(3))` даёт `"1"`, не `"1.0"`), так что оба случая идут через один
/// `IntoLua`, а не единый возвращаемый тип.
enum RandomValue {
    Frac(f64),
    Int(i64),
}

impl IntoLua for RandomValue {
    fn into_lua(self, state: &mut LuaState) -> Result<usize, String> {
        match self {
            RandomValue::Frac(f) => f.into_lua(state),
            RandomValue::Int(i) => i.into_lua(state),
        }
    }
}

/// «Код игры»: `math.random(m [, n])` требует целочисленных аргументов, как в Lua 5.4 —
/// `LuaValue::as_integer` уже делает ровно ту проверку (`m.fract() == 0.0` и в диапазоне `i64`),
/// которую настоящий Lua делает над числом-аргументом, включая float-литералы с целым значением
/// (`2^32` — Lua-float, но `as_integer` признаёт его целым).
fn require_random_int(value: LuaValue) -> Result<i64, String> {
    if let Some(i) = value.as_integer() {
        return Ok(i);
    }
    match value.as_number() {
        Some(n) => Err(format!(
            "math.random: аргумент должен быть целым числом, получено {n}"
        )),
        None => Err(format!(
            "math.random: аргумент должен быть числом, получено {}",
            value.type_name()
        )),
    }
}

/// Целое равномерно в `[low, high]`, как настоящий `math.random` — без `as u32`, который бы
/// обрубал верхние биты диапазона (`math.random(1, 2^32)`) или переполнял вычитание в `i64`
/// (`math.random(math.mininteger, math.maxinteger)`, паника в debug-сборке): ширина диапазона
/// считается в `u64` через `wrapping_sub`/`wrapping_add` над битовым представлением `i64`, что
/// корректно и на самой широкой паре `mininteger`/`maxinteger` (ширина — все `2^64` значений,
/// шире, чем вмещает счётчик остатка, — тогда подходит любой `u64`, без деления по модулю).
fn random_in_range(rng: &mut Rng, low: i64, high: i64) -> Result<i64, String> {
    if low > high {
        return Err("math.random: interval is empty".to_string());
    }
    let width_minus_one = (high as u64).wrapping_sub(low as u64);
    let offset = if width_minus_one == u64::MAX {
        rng.next_u64()
    } else {
        rng.next_u64() % (width_minus_one + 1)
    };
    Ok((low as u64).wrapping_add(offset) as i64)
}

fn install_math_random(lua: &mut Lua, base_cell: &CtxCell) -> LuaResult<LuaFunction> {
    let base_cell = base_cell.clone();
    lua.create_function(
        move |m: Option<LuaValue>, n: Option<LuaValue>| -> Result<RandomValue, String> {
            let ptr = base_ptr(&base_cell)?;
            // SAFETY: see `install_print` — same call-scoped pointer discipline.
            let ctx = unsafe { &mut *ptr };
            match (m, n) {
                (None, None) => Ok(RandomValue::Frac(
                    ctx.rng.next_below(1 << 24) as f64 / (1u64 << 24) as f64,
                )),
                (Some(m), None) => {
                    let m = require_random_int(m)?;
                    Ok(RandomValue::Int(random_in_range(ctx.rng, 1, m)?))
                }
                (Some(m), Some(n)) => {
                    let m = require_random_int(m)?;
                    let n = require_random_int(n)?;
                    Ok(RandomValue::Int(random_in_range(ctx.rng, m, n)?))
                }
                (None, Some(_)) => Err("math.random: неверные аргументы".to_string()),
            }
        },
    )
}

/// `Runner::build`'s result before `compile` assembles it into a `Runner` — named only to keep
/// `clippy::type_complexity` quiet.
type BuildOutput = (LuaTable, LuaTable, LuaTable, Vec<String>);

/// Один загруженный, готовый к работе файл кода игры — создаётся заново и для проверки при
/// загрузке, и для каждой партии («каждая партия создаёт свежий исполнитель»).
pub struct Runner {
    // «Rust conventions»: поля роняются в порядке объявления. `env`/`obj_mt`/`vec2_mt` держат
    // Lua-таблицы, которые ссылаются на `GlobalState` внутри `lua` — она должна упасть последней,
    // иначе их `Drop` обратится в уже освобождённую память, так что она объявлена здесь
    // последней, а не первой.
    chunk_name: String,
    env: LuaTable,
    obj_mt: LuaTable,
    /// Never read back — kept alive only so the `LuaValue` copy `Env::vec2_mt` closures hold
    /// keeps naming a live, rooted table for the whole party (see the note at the top of this
    /// file on why closures hold a `LuaValue` copy rather than this `LuaTable` itself).
    #[allow(dead_code)]
    vec2_mt: LuaTable,
    declared_functions: Vec<String>,
    base_cell: CtxCell,
    world_cell: CtxCell,
    lua: Lua,
}

impl std::fmt::Debug for Runner {
    /// `luars::Lua` итself has no `Debug` impl — printed as the pieces that matter for tests and
    /// logging instead of the VM's own internals.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("chunk_name", &self.chunk_name)
            .field("declared_functions", &self.declared_functions)
            .finish_non_exhaustive()
    }
}

impl Runner {
    /// Собирает `source` и один раз выполняет его верхний уровень — с `find`/`delete`/
    /// `play_sound`, недоступными за отсутствием мира (пункт 3 — «мира ещё нет»), и `print`/
    /// `math.random`, работающими через переданные `rng`/`messages`. Используется и проверкой при
    /// загрузке (одноразовые `rng`/`messages`), и `new_game` (настоящие, партии).
    #[allow(clippy::too_many_arguments)]
    pub fn compile(
        source: &str,
        chunk_name: &str,
        properties: &PropertyTable,
        image_names: &[String],
        sound_names: &[String],
        rng: &mut Rng,
        messages: &mut Vec<String>,
    ) -> Result<Runner, CodeError> {
        let mut lua = Lua::new(SafeOption {
            max_memory_limit: MEMORY_LIMIT_BYTES,
            ..SafeOption::default()
        });

        let base_cell: CtxCell = Rc::new(Cell::new(std::ptr::null_mut()));
        let world_cell: CtxCell = Rc::new(Cell::new(std::ptr::null_mut()));

        let mut base_ctx = BaseCtx { rng, messages };
        let prev = set_ctx(&base_cell, &mut base_ctx);
        let result = Self::build(
            &mut lua,
            source,
            chunk_name,
            properties,
            image_names,
            sound_names,
            &base_cell,
            &world_cell,
        );
        restore_ctx(&base_cell, prev);

        match result {
            Ok((env, obj_mt, vec2_mt, declared_functions)) => Ok(Runner {
                chunk_name: chunk_name.to_string(),
                env,
                obj_mt,
                vec2_mt,
                declared_functions,
                base_cell,
                world_cell,
                lua,
            }),
            Err(err) => {
                let full = lua.get_error_message(err);
                Err(match err {
                    LuaError::InstructionBudgetExceeded => {
                        budget_exceeded_error(deepest_code_line(chunk_name, &full))
                    }
                    _ => parse_full_error(chunk_name, full),
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        lua: &mut Lua,
        source: &str,
        chunk_name: &str,
        properties: &PropertyTable,
        image_names: &[String],
        sound_names: &[String],
        base_cell: &CtxCell,
        world_cell: &CtxCell,
    ) -> LuaResult<BuildOutput> {
        // «Код игры»: `Lua::new` не ставит стандартную библиотеку сама — `create_sandbox_env`
        // только копирует из настоящих глобальных то, что там уже есть, так что без этого
        // `math`/`string`/`table`/`utf8` и базовые функции в песочнице были бы просто пусты.
        lua.open_stdlibs(&[
            Stdlib::Basic,
            Stdlib::Math,
            Stdlib::String,
            Stdlib::Table,
            Stdlib::Utf8,
        ])?;
        // «Код игры»: предел операций — целиком на бюджете `LuaState` (`INSTRUCTION_LIMIT`), не
        // здесь — `SandboxConfig::instruction_limit` остаётся отдельным, per-call пределом одного
        // вызова `execute_sandboxed`, как у автора, и тут не используется.
        let base_config = SandboxConfig {
            basic: true,
            math: true,
            string: true,
            table: true,
            utf8: true,
            ..SandboxConfig::default()
        };
        let chunk_fn: LuaFunction = lua
            .load(source)
            .set_name(chunk_name)
            .with_sandbox(&base_config)
            .into_function()?;
        // Index 1, not searched for by name: `chunk_fn` is the *main chunk* function, whose only
        // ever free variable is `_ENV` — unlike a nested function, which may also close over an
        // outer local and get its upvalues in either order.
        let (_, env): (String, LuaTable) = chunk_fn
            .get_upvalue(1)?
            .ok_or_else(|| lua.global_state_mut().error("код без _ENV".to_string()))?;

        lua.set_instruction_budget(INSTRUCTION_LIMIT);

        let obj_mt = lua.create_table()?;
        let vec2_mt = lua.create_table()?;
        // «Код игры»: `__metatable` — иначе `getmetatable(obj)`/`getmetatable(pair)` отдавали бы
        // эту самую, общую на все объекты игры таблицу, и код мог бы поменять её (`.__index = ...`)
        // разом для каждого объекта или пары в игре.
        obj_mt.set("__metatable", false)?;
        vec2_mt.set("__metatable", false)?;
        // SAFETY: `obj_mt` stays alive on `Runner` for the whole party; `vec2_mt`'s copied value
        // is only ever used by closures that run while that same `Runner` (and so `lua`, and so
        // this table) is alive — see the note at the top of this file on why a `LuaValue` copy is
        // used here instead of holding the `LuaTable` itself inside the closures below.
        let obj_mt_value = unsafe { obj_mt.to_value() };
        let vec2_mt_value = unsafe { vec2_mt.to_value() };

        let env_shared = Env {
            properties: Rc::new(properties.clone()),
            image_names: Rc::new(image_names.to_vec()),
            sound_names: Rc::new(sound_names.to_vec()),
            obj_mt: obj_mt_value,
            vec2_mt: vec2_mt_value,
        };

        install_object_metatable(lua, world_cell, &env_shared, &obj_mt)?;
        install_vec2_metatable(lua, world_cell, &vec2_mt)?;

        let find_fn = install_find(lua, world_cell, &env_shared)?;
        let delete_fn = install_delete(lua, world_cell)?;
        let play_sound_fn = install_play_sound(lua, world_cell, env_shared.sound_names.clone())?;
        let print_fn = install_print(lua, base_cell)?;
        let random_fn = install_math_random(lua, base_cell)?;

        env.set("find", find_fn)?;
        env.set("delete", delete_fn)?;
        env.set("play_sound", play_sound_fn)?;
        env.set("print", print_fn)?;
        let math_table: LuaTable = env.get("math")?;
        math_table.set("random", random_fn)?;
        math_table.raw_set("randomseed", LuaValue::nil())?;

        // «Код игры»: «функция, вызванная правилом, в глобальную переменную не пишет: любая
        // такая запись — ошибка», для ЛЮБОЙ функции — включая `local function` и запись через
        // `_G`/`_ENV`, не только именованные верхнеуровневые. Оба хода бьют по тому, что запись
        // блокировалась не у самой переменной `_ENV`, а у каждой найденной по имени функции
        // порознь: `local function` не входит в `env` (это Lua-локаль, не глобальная), и её
        // `_ENV` так и оставался настоящим, писать через неё; `_G`/`_ENV` внутри кода читались
        // как сам `env` (самоссылка, которую строит `create_sandbox_env`) — тот же настоящий
        // стол, без защиты.
        //
        // Правильное место для защиты — не отдельные функции, а сам `_ENV`-upvalue ГЛАВНОЙ
        // функции чанка: все вложенные замыкания чанка (именованные верхнеуровневые функции И
        // безымянные локальные), ссылающиеся на свободную переменную `_ENV`, делят с чанком один
        // и тот же upvalue-слот (обычная семантика Lua — вложенное замыкание, ссылающееся на
        // upvalue объемлющей функции, а не на её локаль, получает при создании ту же ссылку, не
        // копию), так что один `set_upvalue` на `chunk_fn` до его выполнения накрывает разом всех
        // потомков. Прокси — userdata [`Globals`] (почему не таблица — см. её описание) с
        // метатаблицей-ловушкой: `__index` на настоящий `env`, `__newindex` — Lua-замыкание,
        // срабатывающее на любую запись, с ходом «разрешено ли писать» — общей ячейкой
        // `loading`, а не одноразовой постройкой уже ПОСЛЕ загрузки: во время самого верхнего
        // уровня файла запись разрешена (`loading.on == true`) и уходит настоящим `rawset` в
        // `env`, а сразу после того, как верхний уровень отработал, `loading.on` навсегда
        // становится `false` — и та же прокси, тот же путь запрещает запись всем, кто вызовет
        // любую функцию файла позже. `_G`/`_ENV` внутри `env` тоже перенаправляются на прокси,
        // а не на сам `env`, иначе `_G.x = ...` писал бы прямо в настоящий стол в обход прокси.
        let loading = lua.create_table()?;
        loading.set("on", true)?;
        // «Код игры»: `__metatable` закрывает `getmetatable(_ENV).__index` — иначе он отдавал бы
        // настоящий `env` без защиты; `setmetatable(_ENV, ...)` на этой userdata и без того ошибка
        // («table expected, got userdata» — `setmetatable` требует таблицу первым аргументом), так
        // что снять саму ловушку через него нельзя в любом случае.
        let proxy_mt: LuaTable = lua
            .load(
                "local real, loading = ...\n\
                 return {__index = real, __newindex = function(t, k, v)\n\
                     if loading.on then\n\
                         rawset(real, k, v)\n\
                     else\n\
                         error('запись в глобальную переменную из функции: ' .. tostring(k), 2)\n\
                     end\n\
                 end, __metatable = false}",
            )
            .call((env.clone(), loading.clone()))?;
        // SAFETY: `proxy_mt` keeps the table registry-rooted until `build` returns; after that
        // the userdata's own metatable reference keeps it alive (the GC marks a userdata's
        // metatable).
        let proxy_mt_ptr = unsafe { proxy_mt.to_value() }
            .as_table_ptr()
            .ok_or_else(|| {
                lua.global_state_mut()
                    .error("нет метатаблицы _ENV".to_string())
            })?;
        let state = lua.global_state_mut();
        let proxy_value =
            state.create_userdata(LuaUserdata::with_metatable(Globals, proxy_mt_ptr))?;
        // Registry-rooted until `build` returns; after that `env._G` and the chunk's `_ENV`
        // upvalue keep it alive.
        let proxy = state.to_ref(proxy_value);
        chunk_fn.set_upvalue(1, proxy.to_value())?;
        env.set("_G", proxy.to_value())?;
        install_protected_rawset(lua, &env, proxy.to_value())?;
        env.set("_ENV", proxy.to_value())?;

        let mut call_config = SandboxConfig::default();
        lua.sandbox_insert_global(&mut call_config, "__load", chunk_fn)?;
        lua.execute_sandboxed("return __load()", &call_config)?;
        loading.set("on", false)?;

        let declared: Vec<(LuaValue, LuaValue)> = env.pairs_raw()?;
        let names: Vec<String> = declared
            .into_iter()
            .filter_map(|(k, v)| {
                let name = k.as_str()?.to_string();
                if RESERVED_GLOBALS.contains(&name.as_str()) {
                    return None;
                }
                v.as_lua_function().is_some().then_some(name)
            })
            .collect();

        Ok((env, obj_mt, vec2_mt, names))
    }

    /// Имена функций, объявленных на верхнем уровне файла — для собственной проверки `run`
    /// («run называет функцию, которой в коде нет») и для предупреждений «не используется».
    pub fn declared_functions(&self) -> &[String] {
        &self.declared_functions
    }

    /// «Код игры»: «предел на все вызовы кода за шаг» — `Game::step` calls this once per step,
    /// before any `run`, so the budget (see `INSTRUCTION_LIMIT`) covers every `run` call of that
    /// step together, not each call separately.
    pub fn reset_step_budget(&mut self) {
        self.lua.set_instruction_budget(INSTRUCTION_LIMIT);
    }

    /// Вызывает `function` с объектами-аргументами по месту вызова правила — формы, которые
    /// поддерживает `run` в «Код игры»: `effects.a`/`effects.b` (два объекта), `do` у `delete`/
    /// `spawn` (ноль или один).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        function: &str,
        args: &[u32],
        world: &mut World,
        rng: &mut Rng,
        deletes: &mut Vec<u32>,
        moved: &mut [bool],
        marks: SoundMarks<'_>,
        messages: &mut Vec<String>,
    ) -> Result<(), CodeError> {
        let target: Option<LuaFunction> =
            self.env.get(function).map_err(|e| self.wrap(e, function))?;
        let Some(target) = target else {
            return Err(CodeError {
                message: format!("функции \"{function}\" в коде нет"),
                line: None,
                function: None,
                rule: None,
                step: None,
            });
        };

        // SAFETY: see `build` — `self.obj_mt` stays alive on `Runner` for as long as `self.lua`
        // does, so this copy is valid for the duration of this call.
        let obj_mt_value = unsafe { self.obj_mt.to_value() };
        let mut call_config = SandboxConfig::default();
        let names = ["__a", "__b"];
        for (&id, name) in args.iter().zip(names) {
            let handle = ObjHandle {
                obj: ObjRef {
                    id,
                    generation: world.generation(id),
                },
                mt: obj_mt_value,
            };
            self.lua
                .sandbox_insert_global(&mut call_config, name, handle)
                .map_err(|e| self.wrap(e, function))?;
        }
        self.lua
            .sandbox_insert_global(&mut call_config, "__fn", target)
            .map_err(|e| self.wrap(e, function))?;

        let source = match args.len() {
            0 => "return __fn()",
            1 => "return __fn(__a)",
            _ => "return __fn(__a, __b)",
        };

        let mut world_ctx = WorldCtx {
            world,
            deletes,
            moved,
            marks,
        };
        let mut base_ctx = BaseCtx { rng, messages };

        let base_prev = set_ctx(&self.base_cell, &mut base_ctx);
        let world_prev = set_ctx(&self.world_cell, &mut world_ctx);
        let result = self.lua.execute_sandboxed(source, &call_config);
        restore_ctx(&self.world_cell, world_prev);
        restore_ctx(&self.base_cell, base_prev);

        result.map(|_| ()).map_err(|e| self.wrap(e, function))
    }

    fn wrap(&mut self, err: luars::LuaError, function: &str) -> CodeError {
        let full = self.lua.get_error_message(err);
        let mut error = match err {
            LuaError::InstructionBudgetExceeded => {
                budget_exceeded_error(deepest_code_line(&self.chunk_name, &full))
            }
            _ => parse_full_error(&self.chunk_name, full),
        };
        error.function = Some(function.to_string());
        error
    }
}

/// «Код игры»: тот же русский текст, что раньше давал счётный хук — своя копия `luars` различает
/// исчерпание бюджета как отдельный вариант `LuaError::InstructionBudgetExceeded` (проверяется до
/// разбора текста ошибки, не через её сообщение), но само сообщение об исчерпании — статичное
/// английское `"instruction budget exceeded"`, без места в коде и без числа операций: строку даёт
/// [`deepest_code_line`].
fn budget_exceeded_error(line: Option<u32>) -> CodeError {
    CodeError {
        message: format!("превышен предел операций кода ({INSTRUCTION_LIMIT}) за шаг"),
        line,
        function: None,
        rule: None,
        step: None,
    }
}

/// Строка кода игры в момент ошибки — самый глубокий (последний вызванный) кадр среди тех, что
/// `luars` вернула в `LuaFullError::frames`, чей источник совпадает с `chunk_name`. То же самое
/// когда-то находил `find_chunk_line` разбором текста трассировки: например, ошибка предела
/// операций внутри Lua-ловушки `__newindex` прокси `_ENV` (`install_object_metatable` рядом,
/// `proxy_mt` в `build`) случается в кадре ЭТОЙ ловушки, а не игрового файла — источник её кадра
/// не `chunk_name`, и он пропускается в пользу кадра игрового файла под ним. `frames` пуст для
/// ошибки разбора файла (до первого вызова ни одного Lua-кадра ещё нет) и для любой ошибки,
/// пойманной `pcall` и не долетевшей досюда — `luars` заново обходит живой стек кадров при каждой
/// ошибке, ничего не хранит между ними.
fn deepest_code_line(chunk_name: &str, full: &luars::LuaFullError) -> Option<u32> {
    full.frames()
        .iter()
        .find(|frame| frame.source == chunk_name)
        .map(|frame| frame.line)
}

/// Lua ставит место ошибки в начало текста — `[string "code.lua"]:7:` у ошибки разбора,
/// `code.lua:7:` у `error(...)` во время исполнения и у ошибки, которую вернула функция хозяина
/// (`lua_state.rs`, `luaL_error`-подобное место вызывающей строки); снимается здесь только с
/// текста сообщения. Сама строка берётся через [`deepest_code_line`], кроме ошибки разбора файла
/// (`LuaError::CompileError`) — до первого вызова кадров ещё нет, и её строка остаётся текстовой,
/// как и раньше.
fn parse_full_error(chunk_name: &str, full: luars::LuaFullError) -> CodeError {
    let text = full.message();
    let first_line = text.lines().next().unwrap_or(text);
    let prefixes = [
        format!("[string \"{chunk_name}\"]:"),
        format!("{chunk_name}:"),
    ];
    let stripped = prefixes.iter().find_map(|prefix| {
        let rest = first_line.strip_prefix(prefix.as_str())?;
        let digits = rest.find(|c: char| !c.is_ascii_digit())?;
        if digits == 0 {
            return None;
        }
        let line: u32 = rest[..digits].parse().ok()?;
        let message = rest[digits..].strip_prefix(':')?.trim_start().to_string();
        Some((line, message))
    });
    let text_line = stripped.as_ref().map(|(line, _)| *line);
    let message = stripped
        .map(|(_, message)| message)
        .unwrap_or_else(|| first_line.to_string());
    let line = deepest_code_line(chunk_name, &full).or(text_line);
    CodeError {
        message,
        line,
        function: None,
        rule: None,
        step: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::PropertyTable;

    #[test]
    fn compiles_trivial_source() {
        let properties = PropertyTable::new();
        let mut rng = Rng::new(1);
        let mut messages = Vec::new();
        let result = Runner::compile(
            "x = 1",
            "code.lua",
            &properties,
            &[],
            &[],
            &mut rng,
            &mut messages,
        );
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn run_reads_a_property_from_the_argument_object() {
        let mut properties = PropertyTable::new();
        properties
            .declare_author("score", PropKind::Number)
            .unwrap();
        let score = properties.resolve("score").unwrap();
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_number(id, score, 5.0);

        let mut rng = Rng::new(1);
        let mut messages = Vec::new();
        let mut runner = Runner::compile(
            "function touch(obj) return obj.score end",
            "code.lua",
            &properties,
            &[],
            &[],
            &mut rng,
            &mut messages,
        )
        .expect("compile");

        let mut deletes = Vec::new();
        let mut moved = vec![false; 1];
        let mut sound_window = super::super::sound::SoundWindow::new(0);
        let result = runner.run(
            "touch",
            &[id],
            &mut world,
            &mut rng,
            &mut deletes,
            &mut moved,
            sound_window.marks(),
            &mut messages,
        );
        assert!(result.is_ok(), "{:?}", result.err());
    }

    /// Регресс на изъян диспетчера метаметодов «своей» `luars` (замыкание Rust напрямую в роли
    /// метаметода, требование 10): `call_c_function` восстанавливал top вызывающего кадра по
    /// его `ci.top`, а `call_tm_res`/`call_tm_res1`/`call_tm_res_into` кладут метаметод ровно на
    /// `func_pos == ci.top`, так что однозначный результат оказывался на слот выше top —
    /// атомная фаза GC чистит стек выше top и могла обнулить его раньше, чем он попадёт в
    /// регистр вызывающей стороны. На шестом же ОТДЕЛЬНОМ вызове `Runner::run` подряд `__index`
    /// объекта начинал молча отдавать `nil` вместо только что записанного числа (тот же цикл
    /// внутри ОДНОГО вызова `run`, сколько угодно раз подряд, не ломался никогда — расходится
    /// именно число отдельных вызовов `execute_sandboxed`, то есть число атомных фаз GC).
    /// Исправлено в `luars/src/lua_vm/execute/call.rs`: top не опускается ниже
    /// `func_idx + nresults`, пока не пройдёт `check_gc_safe_point`. Тест держит регресс закрытым.
    #[test]
    fn object_property_survives_many_separate_run_calls() {
        let mut properties = PropertyTable::new();
        properties
            .declare_author("score", PropKind::Number)
            .unwrap();
        let score = properties.resolve("score").unwrap();
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_number(id, score, 0.0);

        let mut rng = Rng::new(1);
        let mut messages = Vec::new();
        let mut runner = Runner::compile(
            "function bump(obj) obj.score = obj.score + 1 end",
            "code.lua",
            &properties,
            &[],
            &[],
            &mut rng,
            &mut messages,
        )
        .expect("compile");

        let mut deletes = Vec::new();
        let mut moved = vec![false; 1];
        const CALLS: i64 = 20;
        for call in 0..CALLS {
            runner.reset_step_budget();
            let mut sound_window = super::super::sound::SoundWindow::new(0);
            let result = runner.run(
                "bump",
                &[id],
                &mut world,
                &mut rng,
                &mut deletes,
                &mut moved,
                sound_window.marks(),
                &mut messages,
            );
            assert!(result.is_ok(), "call {call}: {:?}", result.err());
        }
        assert_eq!(world.number_like(id, score), Some(CALLS as f64));
    }

    /// «Фаза 07» → нефункциональные требования: замер скорости, снятый до и после переключения
    /// исполнителя — до правок на скачанной `luars` со счётным хуком, после — на своей копии с
    /// библиотечным бюджетом. `compute` — вычислительная функция кода игры: цикл с арифметикой,
    /// записью в таблицу и вызовами функции `add`; каждый вызов `run` укладывается в бюджет шага,
    /// а между вызовами бюджет обнуляется, как между шагами партии (`Game::step`).
    #[test]
    #[ignore]
    fn benchmark_repeated_run_calls() {
        const LOOP_ITERATIONS: u32 = 5000;
        const CALLS: usize = 300;

        let mut properties = PropertyTable::new();
        properties
            .declare_author("score", PropKind::Number)
            .unwrap();
        let score = properties.resolve("score").unwrap();
        let mut world = World::new(&properties);
        let id = world.create();

        let mut rng = Rng::new(1);
        let mut messages = Vec::new();
        let source = format!(
            "function add(a, b) return a + b end\n\
             function compute(obj)\n\
                 local t = {{}}\n\
                 local acc = 0\n\
                 for i = 1, {LOOP_ITERATIONS} do\n\
                     t[i] = add(acc, i)\n\
                     acc = t[i]\n\
                 end\n\
                 obj.score = acc\n\
                 return acc\n\
             end"
        );
        let mut runner = Runner::compile(
            &source,
            "code.lua",
            &properties,
            &[],
            &[],
            &mut rng,
            &mut messages,
        )
        .expect("compile");

        let mut deletes = Vec::new();
        let mut moved = vec![false; world.slot_count()];

        let start = std::time::Instant::now();
        for _ in 0..CALLS {
            runner.reset_step_budget();
            let mut sound_window = super::super::sound::SoundWindow::new(0);
            let result = runner.run(
                "compute",
                &[id],
                &mut world,
                &mut rng,
                &mut deletes,
                &mut moved,
                sound_window.marks(),
                &mut messages,
            );
            assert!(result.is_ok(), "{:?}", result.err());
        }
        let elapsed = start.elapsed();
        println!(
            "{CALLS} вызовов run по {LOOP_ITERATIONS} итераций цикла: {elapsed:?} ({:?} на вызов)",
            elapsed / CALLS as u32
        );

        let expected_sum = (u64::from(LOOP_ITERATIONS) * u64::from(LOOP_ITERATIONS + 1) / 2) as f64;
        assert_eq!(world.number_like(id, score), Some(expected_sum));
    }

    /// Compiles `source` and runs `function` once (with a single live object argument when
    /// `with_obj`), returning the resulting error, if any — the shared harness for
    /// [`matrix_matches_head_message_and_line`].
    fn run_and_capture_error(source: &str, function: &str, with_obj: bool) -> Option<CodeError> {
        let mut properties = PropertyTable::new();
        properties
            .declare_author("score", PropKind::Number)
            .unwrap();
        let mut rng = Rng::new(1);
        let mut messages = Vec::new();
        let mut runner = Runner::compile(
            source,
            "code.lua",
            &properties,
            &[],
            &[],
            &mut rng,
            &mut messages,
        )
        .expect("compile");
        let mut world = World::new(&properties);
        let id = world.create();
        let args: &[u32] = if with_obj { &[id] } else { &[] };
        let mut deletes = Vec::new();
        let mut moved = vec![false; world.slot_count()];
        let mut sound_window = super::super::sound::SoundWindow::new(0);
        runner
            .run(
                function,
                args,
                &mut world,
                &mut rng,
                &mut deletes,
                &mut moved,
                sound_window.marks(),
                &mut messages,
            )
            .err()
    }

    /// Compiles `source`, returning the error from loading it, if any — the load-time
    /// counterpart of [`run_and_capture_error`].
    fn load_and_capture_error(source: &str) -> Option<CodeError> {
        let properties = PropertyTable::new();
        let mut rng = Rng::new(1);
        let mut messages = Vec::new();
        Runner::compile(
            source,
            "code.lua",
            &properties,
            &[],
            &[],
            &mut rng,
            &mut messages,
        )
        .err()
    }

    /// «Фаза 07» → ревью правки требования 12: `CodeError.line` — строка самого глубокого кадра
    /// чанка кода игры на стеке в момент ошибки (`deepest_code_line`), тот же кадр, что раньше
    /// находил `find_chunk_line` разбором текста трассировки. Таблица случаев снята вручную на
    /// HEAD (коммит с чистой копией `luars`, до правок этой фазы) через временный git worktree —
    /// `message` и `line` совпадают построчно с тем, что даёт HEAD; единственное намеренное
    /// расхождение — `setmetatable(obj, {})` (требование 9: обычный Lua отказывает, HEAD молчал).
    #[test]
    fn matrix_matches_head_message_and_line() {
        struct Case {
            name: &'static str,
            source: &'static str,
            function: &'static str,
            with_obj: bool,
            message: &'static str,
            line: Option<u32>,
        }

        let cases = [
            Case {
                name: "ошибка Lua в функции (арифметика с nil)",
                source: "function f(obj) return nil + 1 end",
                function: "f",
                with_obj: true,
                message: "attempt to perform arithmetic on a nil value",
                line: Some(1),
            },
            Case {
                name: "опечатка в имени свойства при чтении",
                source: "function f(obj) return obj.nosuch end",
                function: "f",
                with_obj: true,
                message: "неизвестное свойство \"nosuch\"",
                line: Some(1),
            },
            Case {
                name: "опечатка в имени свойства при записи",
                source: "function f(obj) obj.nosuch = 1 end",
                function: "f",
                with_obj: true,
                message: "неизвестное свойство \"nosuch\"",
                line: Some(1),
            },
            Case {
                name: "error('boom')",
                source: "function f() error('boom') end",
                function: "f",
                with_obj: false,
                message: "boom",
                line: Some(1),
            },
            Case {
                name: "error('boom', 2) прямо из функции правила",
                source: "function f() error('boom', 2) end",
                function: "f",
                with_obj: false,
                message: "boom",
                line: Some(1),
            },
            Case {
                name: "error('boom', 2) во вложенной функции",
                source: "function g() error('boom', 2) end\nfunction f() g() end",
                function: "f",
                with_obj: false,
                message: "boom",
                line: Some(1),
            },
            Case {
                name: "error('boom', 0)",
                source: "function f() error('boom', 0) end",
                function: "f",
                with_obj: false,
                message: "boom",
                line: Some(1),
            },
            Case {
                name: "assert(false, 'x')",
                source: "function f() assert(false, 'x') end",
                function: "f",
                with_obj: false,
                message: "x",
                line: Some(1),
            },
            Case {
                name: "rawset(1,2,3)",
                source: "function f() rawset(1, 2, 3) end",
                function: "f",
                with_obj: false,
                message: "bad argument #1 to 'rawset' (table expected)",
                line: Some(1),
            },
            Case {
                name: "запись в глобальную из функции",
                source: "function f() counter = 1 end",
                function: "f",
                with_obj: false,
                message: "запись в глобальную переменную из функции: counter",
                line: Some(1),
            },
            Case {
                name: "предел операций в шаге — простой цикл",
                source: "function f() while true do end end",
                function: "f",
                with_obj: false,
                message: "превышен предел операций кода (1000000) за шаг",
                line: Some(1),
            },
            Case {
                name: "предел операций в шаге — цикл в pcall",
                source: "function f() pcall(function() while true do end end) end",
                function: "f",
                with_obj: false,
                message: "превышен предел операций кода (1000000) за шаг",
                line: Some(1),
            },
            Case {
                name: "предел операций в шаге — while true do f() end",
                source: "function noop() end\nfunction f() while true do noop() end end",
                function: "f",
                with_obj: false,
                message: "превышен предел операций кода (1000000) за шаг",
                line: Some(2),
            },
        ];

        for case in cases {
            let err = run_and_capture_error(case.source, case.function, case.with_obj)
                .unwrap_or_else(|| panic!("{}: ожидалась ошибка", case.name));
            assert_eq!(err.message, case.message, "{}: {err:?}", case.name);
            assert_eq!(err.line, case.line, "{}: {err:?}", case.name);
        }

        // `error({})`/`error({}, 0)`: объект ошибки не строка — `tostring`-адрес меняется между
        // запусками, сравнивается форма, не байты.
        for source in [
            "function f() error({}) end",
            "function f() error({}, 0) end",
        ] {
            let err = run_and_capture_error(source, "f", false).expect("ожидалась ошибка");
            assert!(err.message.starts_with("table: 0x"), "{err:?}");
            assert_eq!(err.line, Some(1), "{err:?}");
        }

        // Нехватка памяти — само по себе и после пойманной `pcall`-ошибки: число байт у своей
        // копии `luars` отличается от HEAD (другой учёт аллокатора), сообщение и строка совпадают.
        for source in [
            "function f() local s = string.rep('x', 20*1024*1024) end",
            "function f() pcall(function() error('x') end) local s = string.rep('y', 20*1024*1024) end",
        ] {
            let err = run_and_capture_error(source, "f", false).expect("ожидалась ошибка");
            assert!(
                err.message
                    .starts_with("out of memory: Memory limit exceeded:"),
                "{err:?}"
            );
            assert_eq!(err.line, Some(1), "{err:?}");
        }

        // Предел операций при загрузке — свой отдельный бюджет, строка тоже совпадает с HEAD.
        for (source, line) in [
            ("while true do end", Some(1u32)),
            ("function f() end\nwhile true do y = 1 end", Some(2)),
        ] {
            let err = load_and_capture_error(source).expect("ожидалась ошибка");
            assert_eq!(
                err.message, "превышен предел операций кода (1000000) за шаг",
                "{err:?}"
            );
            assert_eq!(err.line, line, "{err:?}");
        }

        // Ошибка разбора файла — как на HEAD (до первого вызова кадров ещё нет, строка остаётся
        // текстовой).
        let err = load_and_capture_error("if true then\n").expect("ожидалась ошибка");
        assert_eq!(err.line, Some(2), "{err:?}");

        // Единственное намеренное расхождение с HEAD: на HEAD `setmetatable(obj, {})` не было
        // ошибкой (обычный `rawset`-обход не считался изъяном), теперь обычный Lua 5.4/5.5
        // отказывает на любом значении кроме таблицы (требование 9).
        let err = run_and_capture_error("function f(obj) setmetatable(obj, {}) end", "f", true)
            .expect("setmetatable(obj, {}) — намеренное расхождение с HEAD: теперь ошибка");
        assert_eq!(
            err.message,
            "bad argument #1 to 'setmetatable' (table expected, got userdata)"
        );
        assert_eq!(err.line, Some(1));
    }
}

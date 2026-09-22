//! «Код игры»: исполнитель кода игры на Lua поверх `luars`.
//!
//! Один [`Runner`] живёт одну партию: `new_game` строит его заново из текста файла, `quit`
//! выбрасывает вместе с миром. Между вызовами он держит песочницу Lua и объявленные ею функции;
//! [`Runner::run`] вызывает одну из них с объектами-аргументами по месту вызова правила.
//!
//! Объекты и пары в коде — userdata `luars` ([`ObjRef`]/[`PairRef`]) с общей метатаблицей,
//! строятся заново на каждый возврат в Lua через собственные `IntoLua`. Не таблицы: у таблицы
//! `luars` 0.26.3 запись `t.k = nil` обрабатывает сам, не зовя `__newindex` (`pset_shortstr`
//! отвечает «готово» и на отсутствующий ключ, а `op_set_field` принимает это за обновление
//! существующего), так что «запись `nil` убирает свойство» до движка бы не доходила; запись в
//! userdata всегда идёт через `__newindex` (`finishset`). Ни одно
//! зарегистрированное здесь замыкание не захватывает `LuaTable`/`LuaFunction` (или их контейнер)
//! по значению: у этих типов `Drop` обращается назад в `GlobalState` того же `Lua`, а замыкания
//! сами хранятся внутри него же — получилась бы такая пара, чей порядок разрушения `Lua` никаким
//! порядком полей уже не выправить. Держать разрешено только copy-значения (`LuaValue`) и `Rc`
//! обычных, не-Lua данных.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use luars::{
    FromLua, IntoLua, Lua, LuaApi, LuaFunction, LuaResult, LuaSandboxApi, LuaState, LuaTable,
    LuaUserdata, LuaValue, SafeOption, SandboxConfig, Stdlib, UserDataTrait,
};

use super::property::{self, PropertyId, PropertyTable};
use super::rng::Rng;
use super::sound::SoundMarks;
use super::value::PropKind;
use super::world::World;

/// «Код игры»: предел операций Lua — на прогон файла при загрузке (свой, отдельный бюджет) и на
/// все вызовы `run` одного шага партии вместе (общий бюджет). `SandboxConfig::instruction_limit`
/// (`luars`) ограничивает только один вызов `execute_sandboxed` и не отдаёт наружу, сколько
/// операций тот в действительности потратил (сам остаток живёт в `pub(crate)` поле `LuaState`,
/// см. `with_sandbox_runtime_limits`/`check_sandbox_runtime_limits` в `luars` 0.26.3) — счёт по
/// всем вызовам шага вместе даёт `debug.sethook(hook, "", 1)`: программный `LuaState::set_hook`
/// крейт тоже держит `pub(crate)`, но сам `debug` — обычная библиотека Lua, доступная изнутри
/// движка через настоящую (не песочную) `Lua::execute`. Счётный хук ставится один раз в `build`
/// и вызывается на каждой отдельной операции Lua: count=1 — единственное значение, застрахованное
/// от `hook_on_call`, который перевзводит остаток `count` при входе в любую функцию и с бо́льшим
/// `count` дал бы коду с частыми вызовами функций (цикл вида `while true do f() end`) обходить
/// предел, ни разу не исчерпав остаток между двумя вызовами (проверено по `execute/hook.rs` в
/// исходниках `luars` 0.26.3 и отдельным тестом на именно таком цикле). Хук копит сумму в
/// `Runner::step_ops`, общую для всех вызовов `run` до следующего `Game::step`
/// (`reset_step_budget`); то же поле, ещё не сброшенное ни разу, служит и бюджетом самого файла
/// при загрузке.
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
        PropKind::Grid | PropKind::Keys => Err(format!(
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
    if matches!(kind, PropKind::Grid | PropKind::Keys) {
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
        (PropKind::Grid | PropKind::Keys, _) => unreachable!("checked above"),
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

/// «Код игры»: диспетчер метаметодов `luars` вызывает как `__index`/`__newindex` только
/// настоящее Lua-замыкание или голый C-указатель на функцию — замыкание Rust с захваченным
/// состоянием («rclosure», то, что строит `create_function`) он там не распознаёт, и вызов падает
/// с «attempt to call a function value». Крохотная обёртка на Lua решает это в лоб: диспетчер
/// метаметода вызывает обёртку (настоящее Lua-замыкание), а та вызывает rclosure обычным
/// вызовом — путём, для которого rclosure и сделан: именно так уже работают `find`/`delete`/
/// `print`/`math.random`.
fn lua_wrap2(lua: &mut Lua, f: LuaFunction) -> LuaResult<LuaFunction> {
    lua.load("local f = ...\nreturn function(a, b) return f(a, b) end")
        .call(f)
}

fn lua_wrap3(lua: &mut Lua, f: LuaFunction) -> LuaResult<LuaFunction> {
    lua.load("local f = ...\nreturn function(a, b, c) return f(a, b, c) end")
        .call(f)
}

/// «Код игры»: installs the count hook that backs `INSTRUCTION_LIMIT` — see that constant's own
/// doc comment for why `debug.sethook`/count=1 rather than `SandboxConfig::instruction_limit`.
/// `ops` is bumped once per Lua VM instruction for the rest of this `Lua`'s life, across every
/// later `execute_sandboxed` call; `Runner::reset_step_budget` is the only thing that zeroes it.
fn install_instruction_hook(lua: &mut Lua, ops: Rc<Cell<u64>>) -> LuaResult<()> {
    let hook_fn = lua.create_function(
        move |_event: Option<LuaValue>, _line: Option<LuaValue>| -> Result<(), String> {
            let used = ops.get() + 1;
            ops.set(used);
            if used > INSTRUCTION_LIMIT {
                return Err(format!(
                    "превышен предел операций кода ({INSTRUCTION_LIMIT}) за шаг"
                ));
            }
            Ok(())
        },
    )?;
    lua.set_global("__kzhook", hook_fn)?;
    lua.execute("debug.sethook(__kzhook, '', 1)")?;
    lua.set_global("__kzhook", LuaValue::nil())?;
    Ok(())
}

/// «Код игры»: `rawset` не смотрит на `__newindex` — `rawset(_ENV, 'x', 1)` писал бы в саму
/// прокси-таблицу мимо ловушки в обход запрета «запись в глобальную переменную из функции», а
/// `rawset(_G, ...)`/настоящий `env`, до которого прокси лишь перенаправляет чтение, — та же
/// дыра в упор. Обёртка отказывает тем же текстом, когда первый аргумент — прокси или настоящее
/// окружение, а для любой другой таблицы (свои собственные таблицы кода) работает как обычный
/// `rawset`.
fn install_protected_rawset(
    lua: &mut Lua,
    env: &LuaTable,
    dynamic_env: &LuaTable,
) -> LuaResult<()> {
    let real_rawset: LuaFunction = env.get("rawset")?;
    let wrapped: LuaFunction = lua
        .load(
            "local real_rawset, real_env, proxy_env = ...\n\
             return function(t, k, v)\n\
                 if t == real_env or t == proxy_env then\n\
                     error('запись в глобальную переменную из функции: ' .. tostring(k), 2)\n\
                 end\n\
                 return real_rawset(t, k, v)\n\
             end",
        )
        .call((real_rawset, env.clone(), dynamic_env.clone()))?;
    env.set("rawset", wrapped)
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
    let index_fn = lua_wrap2(lua, index_fn)?;
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
    let newindex_fn = lua_wrap3(lua, newindex_fn)?;
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
    let index_fn = lua_wrap2(lua, index_fn)?;
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
    let newindex_fn = lua_wrap3(lua, newindex_fn)?;
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

/// «Код игры»: печатает через табуляцию, как обычный Lua, с любым числом аргументов, включая
/// `nil` посередине — типизированные замыкания `luars` не видят настоящее число аргументов
/// (`nil` по умолчанию за недостающие неотличим от переданного явно), так что счёт и склейку
/// делает тонкая Lua-обёртка через `select('#', ...)`, тем же приёмом, что `lua_wrap2`/
/// `lua_wrap3` оборачивают метаметоды, и отдаёт Rust уже готовую строку.
fn install_print(lua: &mut Lua, base_cell: &CtxCell) -> LuaResult<LuaFunction> {
    let base_cell = base_cell.clone();
    let sink = lua.create_function(move |line: String| -> Result<(), String> {
        let ptr = base_ptr(&base_cell)?;
        // SAFETY: `Runner::compile`/`Runner::run` set this pointer for the whole span of the
        // call into Lua and clear it right after; Lua is single-threaded.
        let ctx = unsafe { &mut *ptr };
        ctx.messages.push(format!("print: {line}"));
        Ok(())
    })?;
    lua.load(
        "local sink = ...\n\
         return function(...)\n\
             local n = select('#', ...)\n\
             local parts = {}\n\
             for i = 1, n do\n\
                 parts[i] = tostring(select(i, ...))\n\
             end\n\
             return sink(table.concat(parts, '\\t'))\n\
         end",
    )
    .call(sink)
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
type BuildOutput = (LuaTable, LuaTable, LuaTable, Vec<String>, Rc<Cell<u64>>);

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
    /// «Код игры»: операции, потраченные всеми вызовами `run` с последнего `reset_step_budget` —
    /// делит счётный хук, установленный в `build`, со всей жизнью `lua` (см. `INSTRUCTION_LIMIT`).
    step_ops: Rc<Cell<u64>>,
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
            Ok((env, obj_mt, vec2_mt, declared_functions, step_ops)) => Ok(Runner {
                chunk_name: chunk_name.to_string(),
                env,
                obj_mt,
                vec2_mt,
                declared_functions,
                base_cell,
                world_cell,
                step_ops,
                lua,
            }),
            Err(err) => {
                let full = lua.get_error_message(err);
                Err(parse_full_error(chunk_name, full))
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
        // `Debug` открывается только на настоящих (не песочных) глобальных, для собственного
        // использования движком (`debug.sethook` ниже) — `base_config.debug` остаётся `false`,
        // так что в песочный `_ENV` кода игры она не копируется.
        lua.open_stdlibs(&[
            Stdlib::Basic,
            Stdlib::Math,
            Stdlib::String,
            Stdlib::Table,
            Stdlib::Utf8,
            Stdlib::Debug,
        ])?;
        // «Код игры»: предел операций — целиком на счётном хуке ниже, не здесь — `SandboxConfig::
        // instruction_limit` сбрасывался бы заново на каждый вызов `execute_sandboxed` (свой
        // отдельный бюджет на один вызов) и срабатывал бы раньше общего хука на первом же вызове
        // шага, показывая английское сообщение `luars` вместо русского текста хука.
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

        let step_ops: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        install_instruction_hook(lua, step_ops.clone())?;

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
        // потомков. `dynamic_env` — та же прокси-таблица-ловушка, что и раньше (`__index` на
        // настоящий `env`, `__newindex` — Lua-замыкание, всегда срабатывающее, потому что сама
        // прокси остаётся вечно пустой), но с ходом «разрешено ли писать» — общей ячейкой
        // `loading`, а не одноразовой постройкой уже ПОСЛЕ загрузки: во время самого верхнего
        // уровня файла запись разрешена (`loading.on == true`) и уходит настоящим `rawset` в
        // `env`, а сразу после того, как верхний уровень отработал, `loading.on` навсегда
        // становится `false` — и та же прокси, тот же путь запрещает запись всем, кто вызовет
        // любую функцию файла позже. `_G`/`_ENV` внутри `env` тоже перенаправляются на
        // `dynamic_env`, а не на сам `env`, иначе `_G.x = ...` писал бы прямо в настоящий стол в
        // обход прокси.
        let loading = lua.create_table()?;
        loading.set("on", true)?;
        // «Код игры»: `__metatable` закрывает и `getmetatable(_ENV).__index` (настоящий `env` без
        // защиты), и `getmetatable(_ENV).__newindex = nil` (снятие самой ловушки) разом —
        // `getmetatable(_ENV)` отдаёт это значение вместо настоящей метатаблицы, а
        // `setmetatable` на защищённой таблице падает сам, обеих лазеек больше нет.
        let dynamic_env: LuaTable = lua
            .load(
                "local real, loading = ...\n\
                 return setmetatable({}, {__index = real, __newindex = function(t, k, v)\n\
                     if loading.on then\n\
                         rawset(real, k, v)\n\
                     else\n\
                         error('запись в глобальную переменную из функции: ' .. tostring(k), 2)\n\
                     end\n\
                 end, __metatable = false})",
            )
            .call((env.clone(), loading.clone()))?;
        chunk_fn.set_upvalue(1, dynamic_env.clone())?;
        env.set("_G", dynamic_env.clone())?;
        install_protected_rawset(lua, &env, &dynamic_env)?;
        env.set("_ENV", dynamic_env)?;

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

        Ok((env, obj_mt, vec2_mt, names, step_ops))
    }

    /// Имена функций, объявленных на верхнем уровне файла — для собственной проверки `run`
    /// («run называет функцию, которой в коде нет») и для предупреждений «не используется».
    pub fn declared_functions(&self) -> &[String] {
        &self.declared_functions
    }

    /// «Код игры»: «предел на все вызовы кода за шаг» — `Game::step` calls this once per step,
    /// before any `run`, so the budget the instruction hook enforces (see `INSTRUCTION_LIMIT`)
    /// covers every `run` call of that step together, not each call separately.
    pub fn reset_step_budget(&mut self) {
        self.step_ops.set(0);
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
        let mut error = parse_full_error(&self.chunk_name, full);
        error.function = Some(function.to_string());
        error
    }
}

/// The line a chunk-position prefix (Lua's own `luaO_chunkid` convention for a chunk loaded from
/// a string: `[string "code.lua"]:12: ...`) names for `chunk_name`, wherever it first appears in
/// `text` — not only as `text`'s own prefix. A `luars` error raised directly from Lua code
/// (`error(...)`, a parse error) carries this prefix on `text` itself, but one raised from a Rust
/// closure (`Err(String)` from `find`/`delete`/a property read or write — everything this file
/// registers) never does: `luars` only stamps a raised error's message with source position when
/// the frame raising it is itself a Lua frame, and the frame active inside a Rust closure is the
/// C/host frame the VM pushed to call it, not a Lua one (confirmed against `luars` 0.26.3's own
/// `LuaState::add_runtime_error_info`). That closure's error still shows up as the first line of
/// `text`'s own stack traceback instead — `[C]: in function` immediately followed by the deepest
/// Lua frame that called it, which is exactly the line coding this call — so scanning the whole
/// text for the first occurrence of the chunk's own prefix, not just checking it as a prefix,
/// finds the right line either way.
fn find_chunk_line(chunk_name: &str, text: &str) -> Option<u32> {
    let prefix = format!("[string \"{chunk_name}\"]:");
    let rest = &text[text.find(prefix.as_str())? + prefix.len()..];
    rest[..rest.find(':')?].trim().parse().ok()
}

fn parse_full_error(chunk_name: &str, full: luars::LuaFullError) -> CodeError {
    let text = full.message();
    let first_line = text.lines().next().unwrap_or(text);
    // Lua ставит место ошибки в начало текста — `[string "code.lua"]:7:` у ошибки разбора и
    // `code.lua:7:` у `error(...)` во время исполнения; строка уже лежит в `line`, в тексте ей
    // не место.
    let prefixes = [
        format!("[string \"{chunk_name}\"]:"),
        format!("{chunk_name}:"),
    ];
    let message = prefixes
        .iter()
        .find_map(|prefix| {
            let rest = first_line.strip_prefix(prefix.as_str())?;
            let digits = rest.find(|c: char| !c.is_ascii_digit())?;
            if digits == 0 {
                return None;
            }
            rest[digits..].strip_prefix(':').map(str::trim_start)
        })
        .unwrap_or(first_line)
        .to_string();
    CodeError {
        message,
        line: find_chunk_line(chunk_name, text),
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
}

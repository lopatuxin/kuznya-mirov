//! «Экраны и состояние» + «Интерфейс игры»: screens, their elements, and the mouse-driven state
//! machine that moves between them. The engine itself knows nothing about "menu" or "pause" —
//! only a screen's name, whether the world runs on it, and its list of elements.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::game::Game;
use super::input::{ButtonKey, MouseEvent, MouseState, UiEvent, UiQueue};
use super::interface::{self, Part, Placed, SCROLL_POINTS_PER_ROW};
use super::particles::Settings;
use super::property::{self, PropertyId, PropertyTable};
use super::rules::{Outcome, Selector};
use super::value::{ImageId, PropKind, Value};
use super::world::World;

pub type ScreenId = usize;
pub type FontId = usize;
/// Index into `files.music`, in declaration order — «Звук»: a screen's `music`
/// field resolves its name to one of these at load time, the same way `font` resolves to `FontId`.
pub type MusicId = usize;

/// «Картинки» → «Где появляется картинка»: a panel or button's own fill is exactly one of these,
/// never both — checked at load time. Image and color are drawn the same way further down the
/// pipeline (a color fill is the atlas's white pixel, stretched and multiplied by the color), but
/// the data itself keeps them apart: a color fill's own alpha already carries opacity
/// (`#rrggbbaa`), so `opacity` only ever appears on the image side.
#[derive(Debug, Clone, Copy)]
pub enum Fill {
    Color([f32; 4]),
    Image { image: ImageId, opacity: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// `(fx, fy)`: the fractional point on the window this anchor names — 0 for an edge on the
    /// near side, 1 for the far side, 0.5 for the midline. «Интерфейс игры» → «Раскладка». Also
    /// used by `core::world_elements` for the same nine points on an *object's* rectangle instead
    /// of the window's.
    pub(crate) fn fractions(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }

    /// This anchor's own point on a rectangle at `position`/`size` — shared by
    /// `world_elements::element_top_left` (the point on an *object's* rectangle a world element
    /// centers on) and `render::atlas` (both the object's own anchor point and, at `position:
    /// [0, 0]`, the same fractional point on an own-size image's rectangle — «Картинки»,
    /// требования 8–9).
    pub(crate) fn point_on(self, position: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        let (fx, fy) = self.fractions();
        [position[0] + fx * size[0], position[1] + fy * size[1]]
    }

    pub fn parse(s: &str) -> Option<Anchor> {
        Some(match s {
            "top_left" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "top_right" => Anchor::TopRight,
            "left" => Anchor::Left,
            "center" => Anchor::Center,
            "right" => Anchor::Right,
            "bottom_left" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottom_right" => Anchor::BottomRight,
            _ => return None,
        })
    }

    /// «Картинки», требование 8: mirrors left and right, leaves an anchor on the vertical midline
    /// (`Top`/`Center`/`Bottom`) unchanged — `render::atlas::own_size_rect` uses this together with
    /// negating `offset.x` to place a `flip_x` object's own-size image.
    pub(crate) fn flip_x(self) -> Anchor {
        match self {
            Anchor::TopLeft => Anchor::TopRight,
            Anchor::TopRight => Anchor::TopLeft,
            Anchor::Left => Anchor::Right,
            Anchor::Right => Anchor::Left,
            Anchor::BottomLeft => Anchor::BottomRight,
            Anchor::BottomRight => Anchor::BottomLeft,
            other => other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub fn parse(s: &str) -> Option<Align> {
        Some(match s {
            "left" => Align::Left,
            "center" => Align::Center,
            "right" => Align::Right,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub anchor: Anchor,
    pub offset: [f32; 2],
    pub size: [f32; 2],
}

impl Placement {
    /// The element's top-left corner in window pixels, given the window's current size.
    /// «Интерфейс игры» → «Раскладка».
    pub fn top_left(&self, viewport: [f32; 2]) -> [f32; 2] {
        let (fx, fy) = self.anchor.fractions();
        let x = if fx == 0.0 {
            self.offset[0]
        } else if fx == 1.0 {
            viewport[0] - self.size[0] - self.offset[0]
        } else {
            (viewport[0] - self.size[0]) / 2.0 + self.offset[0]
        };
        let y = if fy == 0.0 {
            self.offset[1]
        } else if fy == 1.0 {
            viewport[1] - self.size[1] - self.offset[1]
        } else {
            (viewport[1] - self.size[1]) / 2.0 + self.offset[1]
        };
        [x, y]
    }

    pub fn contains(&self, viewport: [f32; 2], point: [f32; 2]) -> bool {
        let [x, y] = self.top_left(viewport);
        point[0] >= x && point[0] < x + self.size[0] && point[1] >= y && point[1] < y + self.size[1]
    }
}

/// One piece of a `text` field after parsing: literal characters, or a `{object.prop}`
/// substitution naming an object by its `name` property and one of its properties.
#[derive(Debug, Clone)]
pub enum TextPart {
    Literal(String),
    Value {
        object_name: String,
        prop: PropertyId,
    },
    /// «Интерфейс игры» → «Элемент при объекте»: `{свойство}` без имени объекта — свойство объекта
    /// элемента с `for` или строки списка.
    Own(PropertyId),
}

/// «Интерфейс игры» → «Текст»: resolves every `{object.prop}` substitution against `world`,
/// finding the object by its `name` property (the one with the lowest id wins if several share
/// a name) — literal text passes through unchanged. A named object that doesn't currently exist
/// substitutes empty, not an error: it can legitimately have been deleted mid-partie.
pub fn format_text(parts: &[TextPart], world: &World, properties: &PropertyTable) -> String {
    format_text_for(parts, world, properties, None)
}

/// То же для элемента при объекте `own`: `{свойство}` без имени берёт свойство этого объекта.
pub fn format_text_for(
    parts: &[TextPart],
    world: &World,
    properties: &PropertyTable,
    own: Option<u32>,
) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TextPart::Literal(s) => out.push_str(s),
            TextPart::Value { object_name, prop } => {
                if let Some(id) = find_named_object(world, object_name) {
                    out.push_str(&format_property(world, id, *prop, properties));
                }
            }
            TextPart::Own(prop) => {
                if let Some(id) = own {
                    out.push_str(&format_property(world, id, *prop, properties));
                }
            }
        }
    }
    out
}

fn find_named_object(world: &World, name: &str) -> Option<u32> {
    world
        .ids()
        .find(|&id| world.text(id, property::NAME) == Some(name))
}

/// «Числа печатаются как есть» — целые без дробной части, дробные со своей. `{}` на `f64`
/// already omits a trailing `.0`, so no special-casing is needed here. `pub(crate)` — «Надписи и
/// полоски в мире», требование 9: `core::world_elements` formats a label's own substitution with
/// this exact function, not a second copy of it.
pub(crate) fn format_property(
    world: &World,
    id: u32,
    prop: PropertyId,
    properties: &PropertyTable,
) -> String {
    match properties.kind(prop) {
        PropKind::Number => world
            .number_like(id, prop)
            .map(|n| format!("{n}"))
            .unwrap_or_default(),
        // «Значение свойства вида "время" подставляется в секундах» — переведено обратно из
        // шагов, в которых оно хранится внутри движка.
        PropKind::Time => world
            .time(id, prop)
            .map(|steps| format!("{}", steps as f64 / 60.0))
            .unwrap_or_default(),
        PropKind::Timer => world
            .timer(id, prop)
            .map(|steps| format!("{}", steps as f64 / 60.0))
            .unwrap_or_default(),
        PropKind::Rotation => world
            .rotation(id, prop)
            .map(|r| format!("{}", r.angle()))
            .unwrap_or_default(),
        PropKind::Shape => world
            .shape(id, prop)
            .map(|shape| shape.as_str().to_string())
            .unwrap_or_default(),
        PropKind::Flag => {
            if world.flag(id, prop) {
                "да".to_string()
            } else {
                String::new()
            }
        }
        PropKind::Text => world.text(id, prop).unwrap_or_default().to_string(),
        PropKind::Vec2
        | PropKind::Color
        | PropKind::Layer
        | PropKind::Grid
        | PropKind::Keys
        | PropKind::Image
        | PropKind::ImageList
        | PropKind::FollowMouse
        | PropKind::OnClick => String::new(),
    }
}

/// Index into `ScreensConfig::initial_values` — «Экраны и состояние» → «Начальные значения у
/// new_game», требование 29: `new_game`'s optional third element, one set per distinct list a
/// button or a screen key declares.
pub type InitialValuesId = usize;

/// Номер вызова функции кода в `ScreensConfig::runs`.
pub type RunId = usize;

/// `["run", "<функция>"]` кнопки или клавиши экрана: какую функцию кода вызвать и как назвать место в
/// ошибке — «экран "main", кнопка "Взять"».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCall {
    pub function: String,
    pub origin: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonCommand {
    ShowScreen(ScreenId),
    NewGame(ScreenId, Option<InitialValuesId>),
    Resume,
    Quit,
    /// «Звук» → «Два вида звука»: touches no world property, so it needs
    /// none of `apply_command`'s screen-switching machinery — just flips `ScreenState`'s own
    /// `sound_enabled`.
    ToggleSound,
    /// «Экраны и состояние» → «Команды»: вызывает функцию кода игры с объектом кнопки.
    Run(RunId),
    /// «Экраны и состояние» → «Команды»: выход из игры — мир уничтожается, страница уходит к списку игр
    /// (в редакторе партия останавливается, как «Стоп»).
    Exit,
}

/// Элемент экрана. Панель, надпись и кнопка вне строки списка могут иметь отбор `for_` — «Элемент при
/// объекте»: пока объекта отбора нет, элемента нет. Внутри `row` списка `for_` пуст, а `placement` —
/// сдвиг и размер от левого верхнего угла строки.
#[derive(Debug, Clone)]
pub enum Element {
    Panel {
        placement: Placement,
        fill: PanelFill,
        /// «Картинки» → «Растягивание рамки»: толщина рамки картинки с полями `slice` на экране.
        image_scale: f32,
        /// «Подсказка»: текст, всплывающий над элементом.
        tooltip: Option<Vec<TextPart>>,
        for_: Option<Selector>,
        /// «Наведение»: имя места наведения.
        name: Option<String>,
        /// «Наведение»: номер элемента экрана — места, пока курсор над которым панель рисуется.
        show_on_hover: Option<usize>,
        /// «Наведение», «Видео на панели»: номер места, пока курсор над которым видео панели идёт вперёд.
        play_on_hover: Option<usize>,
        /// «Видео на панели»: секунда ролика, где поза та же, что на последнем кадре; `None` — назад играет весь ролик.
        rest_at: Option<f32>,
        /// «Видео на панели»: ролик покоя — видео по кругу, которое панель показывает вместо своего, пока то стоит
        /// на первом кадре без курсора.
        idle: Option<ImageId>,
        /// «Ветер и частицы» → «На экране»: свойства частиц панели; `Some` — панель названа источником,
        /// даже если ни один эффект не включён.
        particles: Option<Settings>,
    },
    Label {
        placement: Placement,
        text: Vec<TextPart>,
        font: FontId,
        font_size: f32,
        color: [f32; 4],
        align: Align,
        /// «Текст»: перенос по словам по ширине элемента, строки сверху.
        wrap: bool,
        for_: Option<Selector>,
        /// «Наведение»: номер элемента экрана — места, пока курсор над которым надпись рисуется.
        show_on_hover: Option<usize>,
    },
    Button {
        placement: Placement,
        text: Vec<TextPart>,
        font: FontId,
        font_size: f32,
        text_color: [f32; 4],
        fill: Fill,
        fill_hover: Fill,
        fill_pressed: Fill,
        /// Одна на все три вида кнопки.
        image_scale: f32,
        tooltip: Option<Vec<TextPart>>,
        on_click: ButtonCommand,
        for_: Option<Selector>,
        /// «Наведение»: имя места наведения.
        name: Option<String>,
    },
    /// «Шкала»: подложка на весь прямоугольник и заливка от левого края на долю `value / max`
    /// объекта элемента — объекта отбора `for_` или строки списка.
    Bar {
        placement: Placement,
        value: PropertyId,
        max: MaxSpec,
        fill: Fill,
        back: Option<Fill>,
        image_scale: f32,
        tooltip: Option<Vec<TextPart>>,
        for_: Option<Selector>,
    },
    /// «Список»: строка `row` на каждый живой объект отбора `for_`.
    List {
        placement: Placement,
        for_: Selector,
        row_height: f32,
        gap: f32,
        sort_by: Option<PropertyId>,
        descending: bool,
        row: Vec<Element>,
        scroll_color: [f32; 4],
    },
}

impl Element {
    pub fn placement(&self) -> &Placement {
        match self {
            Element::Panel { placement, .. }
            | Element::Label { placement, .. }
            | Element::Button { placement, .. }
            | Element::Bar { placement, .. }
            | Element::List { placement, .. } => placement,
        }
    }

    /// «Подсказка»: текст подсказки панели, кнопки или шкалы; у надписи и списка её нет.
    pub fn tooltip(&self) -> Option<&[TextPart]> {
        match self {
            Element::Panel { tooltip, .. }
            | Element::Button { tooltip, .. }
            | Element::Bar { tooltip, .. } => tooltip.as_deref(),
            Element::Label { .. } | Element::List { .. } => None,
        }
    }

    /// Панели, надписи и кнопки элемента: он сам или элементы строки списка.
    pub fn leaves(&self) -> impl Iterator<Item = &Element> {
        let (single, row) = match self {
            Element::List { row, .. } => (None, row.as_slice()),
            other => (Some(other), &[][..]),
        };
        single.into_iter().chain(row)
    }

    /// «Наведение»: имя места наведения панели или кнопки.
    pub fn place_name(&self) -> Option<&str> {
        match self {
            Element::Panel { name, .. } | Element::Button { name, .. } => name.as_deref(),
            _ => None,
        }
    }

    /// «Наведение»: место, пока курсор над которым панель или надпись рисуется; без него рисуется всегда.
    pub fn show_on_hover(&self) -> Option<usize> {
        match self {
            Element::Panel { show_on_hover, .. } | Element::Label { show_on_hover, .. } => {
                *show_on_hover
            }
            _ => None,
        }
    }

    /// «Наведение»: рисуется ли элемент, когда курсор над местом `place` (`None` — ни над каким): без
    /// `show_on_hover` рисуется всегда.
    pub fn visible_over(&self, place: Option<usize>) -> bool {
        self.show_on_hover()
            .is_none_or(|target| place == Some(target))
    }

    /// «Наведение», «Видео на панели»: место, пока курсор над которым видео панели идёт вперёд.
    pub fn play_on_hover(&self) -> Option<usize> {
        match self {
            Element::Panel { play_on_hover, .. } => *play_on_hover,
            _ => None,
        }
    }

    /// «Ветер и частицы» → «На экране»: свойства частиц панели-источника.
    pub fn particles(&self) -> Option<&Settings> {
        match self {
            Element::Panel { particles, .. } => particles.as_ref(),
            _ => None,
        }
    }

    /// «Интерфейс игры» → «Мышь»: панель-источник частиц без `name` прозрачна для мыши — туча пуха не
    /// отнимает курсор у героев под ней. С `name` панель — место наведения, как всякая.
    pub fn passes_mouse(&self) -> bool {
        self.particles().is_some() && self.place_name().is_none()
    }

    /// Отбор `for` элемента при объекте; у списка свой отбор строк, он не в счёт.
    pub fn selector(&self) -> Option<&Selector> {
        match self {
            Element::Panel { for_, .. }
            | Element::Label { for_, .. }
            | Element::Button { for_, .. }
            | Element::Bar { for_, .. } => for_.as_ref(),
            Element::List { .. } => None,
        }
    }
}

/// «Экраны и состояние» → «Клавиши экрана»: name-to-command, the same five commands as
/// `on_click`. A key named here never reaches the world, on either press or release.
pub type ScreenKeyTable = HashMap<String, ButtonCommand>;

#[derive(Debug, Clone)]
pub struct Screen {
    pub name: String,
    pub world_runs: bool,
    pub elements: Vec<Element>,
    pub keys: ScreenKeyTable,
    /// «Звук» → «Два вида звука»: `None` is silence, same as the pause screen.
    pub music: Option<MusicId>,
    /// «Ветер и частицы» → «На экране»: клетка экрана — сколько точек `screens.json` в одной клетке частиц;
    /// `None`, когда у экрана её нет (частиц на нём тогда быть не может).
    pub cell_pixels: Option<f64>,
    /// «Ветер и частицы» → «На экране»: ровный ветер частиц экрана, клеток экрана в секунду.
    pub wind: [f64; 2],
}

#[derive(Debug, Clone, Default)]
pub struct ScreensConfig {
    pub screens: Vec<Screen>,
    pub start_screen: ScreenId,
    pub win_screen: Option<ScreenId>,
    pub loss_screen: Option<ScreenId>,
    /// «Начальные значения у new_game», требование 29: every distinct value list a `new_game`
    /// command declares, indexed by `InitialValuesId`.
    pub initial_values: Vec<Vec<(String, PropertyId, Value)>>,
    /// «Надписи и полоски в мире», требование 1: the optional `world_elements` list, in file
    /// order — empty when `screens.json` names none.
    pub world_elements: Vec<WorldElement>,
    /// `["run", ...]` кнопок и клавиш экранов: `ButtonCommand::Run` называет вызов по номеру.
    pub runs: Vec<RunCall>,
    /// «Подсказка»: вид подсказок, один на игру; `None`, когда блока `tooltip` в `screens.json` нет.
    pub tooltip: Option<TooltipStyle>,
    /// «Раскладка экрана»: размер окна, под который расставлены элементы экранов; `None` — точка в точку.
    pub ui_reference: Option<[f32; 2]>,
}

/// «Подсказка»: блок `tooltip` в `screens.json`.
#[derive(Debug, Clone, Copy)]
pub struct TooltipStyle {
    pub fill: Fill,
    pub image_scale: f32,
    pub font: FontId,
    pub font_size: f32,
    pub text_color: [f32; 4],
    /// `[по бокам, сверху и снизу]`.
    pub padding: [f32; 2],
    /// Расстояние от элемента.
    pub gap: f32,
    /// Секунды времени окна до появления.
    pub delay: f64,
}

/// «Раскладка экрана»: во сколько раз интерфейс экранов крупнее точек `screens.json` и какого размера
/// окно в этих точках.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiFrame {
    pub scale: f32,
    pub viewport: [f32; 2],
}

impl UiFrame {
    /// Точка окна в точках `screens.json`.
    pub fn point(&self, window_point: [f32; 2]) -> [f32; 2] {
        [window_point[0] / self.scale, window_point[1] / self.scale]
    }
}

impl ScreensConfig {
    /// «Видео на панели»: картинки-видео, которые играют панели с `play_on_hover`.
    pub fn panel_videos(&self) -> Vec<ImageId> {
        self.panel_video_images(true, |_| true)
    }

    /// «Видео на панели»: картинки-видео, которые панели крутят по кругу, — без `play_on_hover`, и ролики
    /// покоя `idle`. `is_video` говорит, какая картинка игры видео: экран этого не знает.
    pub fn loop_videos(&self, is_video: impl Fn(ImageId) -> bool) -> Vec<ImageId> {
        let mut videos = self.panel_video_images(false, is_video);
        for element in self.screens.iter().flat_map(|screen| &screen.elements) {
            if let Element::Panel {
                idle: Some(idle), ..
            } = element
                && !videos.contains(idle)
            {
                videos.push(*idle);
            }
        }
        videos
    }

    fn panel_video_images(
        &self,
        on_hover: bool,
        is_video: impl Fn(ImageId) -> bool,
    ) -> Vec<ImageId> {
        let mut videos = Vec::new();
        for element in self.screens.iter().flat_map(|screen| &screen.elements) {
            if let Element::Panel {
                fill: PanelFill::Fixed(Fill::Image { image, .. }),
                play_on_hover,
                ..
            } = element
                && play_on_hover.is_some() == on_hover
                && is_video(*image)
                && !videos.contains(image)
            {
                videos.push(*image);
            }
        }
        videos
    }

    /// Интерфейс окна `window` (в точках CSS): с `ui_reference` всё умножено на
    /// `min(ширина окна / ширина, высота окна / высота)`, без него — как есть.
    pub fn ui_frame(&self, window: [f32; 2]) -> UiFrame {
        let scale = self
            .ui_reference
            .map_or(1.0, |[w, h]| (window[0] / w).min(window[1] / h));
        UiFrame {
            scale,
            viewport: [window[0] / scale, window[1] / scale],
        }
    }
}

/// «Надписи и полоски в мире», требование 6: one `{property}` substitution over a world element's
/// *own* object — unlike a screen's `TextPart`, this never names an object, only a property of
/// whichever object the element is drawn over.
#[derive(Debug, Clone)]
pub enum WorldTextPart {
    Literal(String),
    Value(PropertyId),
}

/// «Надписи и полоски в мире», требование 51: a `bar`'s `max` — a positive constant or a
/// `number`-kind property, resolved once at load time the same way `value` already is.
#[derive(Debug, Clone, Copy)]
pub enum MaxSpec {
    Const(f32),
    Property(PropertyId),
}

/// «Надписи и полоски в мире», требование 5: `color`/`back_color` — a plain color, or a color
/// picked from a list by a `number`-kind property, exactly the way a rule's `add`/`set` picks a
/// number from `{table, by}» → [[Правила игры]].
#[derive(Debug, Clone)]
pub enum WorldColor {
    Solid([f32; 4]),
    Table {
        colors: Vec<[f32; 4]>,
        by: PropertyId,
    },
}

/// «Надписи, полоски и картинки в мире», требование 1: какую картинку рисует `image` — названную в
/// `files.images` сразу или ту, чьё имя хранит свойство самого объекта (`{свойство}`).
#[derive(Debug, Clone)]
pub enum ImageRef {
    Fixed(ImageId),
    /// `names` — имена `files.images` в порядке `ImageId`: имя из свойства ищется среди них.
    Own {
        property: PropertyId,
        names: Arc<[String]>,
    },
}

impl ImageRef {
    /// Требование 3: у объекта нет свойства, оно пустое или называет картинку, которой нет, — `None`.
    pub fn resolve(&self, world: &World, id: u32, properties: &PropertyTable) -> Option<ImageId> {
        match self {
            ImageRef::Fixed(image) => Some(*image),
            ImageRef::Own { property, names } => {
                let name = format_property(world, id, *property, properties);
                names.iter().position(|declared| *declared == name)
            }
        }
    }
}

/// Заливка панели: цвет или картинка из `files.images` — или «Элемент при объекте»: картинка, чьё имя
/// хранит строковое свойство объекта элемента (`image: "{свойство}"` у панели строки списка или с `for`).
#[derive(Debug, Clone)]
pub enum PanelFill {
    Fixed(Fill),
    /// `image` — всегда `ImageRef::Own`. Нет свойства, оно пустое, называет картинку, которой нет, или
    /// картинку, которую панель не берёт (видео, `frame_by`, `size`), — панели не видно.
    Own {
        image: ImageRef,
        opacity: f32,
    },
}

#[derive(Debug, Clone)]
pub enum WorldElementKind {
    Label {
        text: Vec<WorldTextPart>,
        font: FontId,
        font_size: f32,
        align: Align,
    },
    Bar {
        value: PropertyId,
        max: MaxSpec,
        back_color: Option<[f32; 4]>,
    },
    Image {
        image: ImageRef,
    },
}

/// «Надписи и полоски в мире», требования 1–17: one entry of `screens.json`'s `world_elements`
/// list, fully resolved — drawn over every live object `for_` selects. `placement` is the same
/// `anchor`/`offset`/`size` a screen element carries, but read differently: `core::world_elements`
/// centers the element on the anchor point of the *object's own rectangle*, offset by `offset`,
/// rather than hugging a corner of the window.
#[derive(Debug, Clone)]
pub struct WorldElement {
    pub for_: Selector,
    pub placement: Placement,
    /// У `image` не читается: картинка цвета не имеет.
    pub color: WorldColor,
    pub kind: WorldElementKind,
}

/// The active screen and the one remembered screen `resume` returns to — «Экраны и состояние» → «Команды»: at most one level deep, no further history.
#[derive(Debug, Clone)]
pub struct ScreenState {
    active: ScreenId,
    previous: Option<ScreenId>,
    /// «Звук» → «Два вида звука»: belongs to the circle, next
    /// to the active screen, not to the world — `new_game`/`quit` don't touch it, and it starts
    /// `true` on every load, since the engine keeps no storage for it at all.
    sound_enabled: bool,
    /// «Список»: на сколько строк прокручен каждый список — по экрану и номеру его элемента. Новая
    /// партия сбрасывает прокрутку.
    scrolls: HashMap<(ScreenId, usize), ListScroll>,
    /// «Экраны и состояние» → «Команды»: `exit` сработал — игра просит страницу уйти к списку игр.
    exit_requested: bool,
    /// Номер последнего открытия экрана: частицы и видео экрана начинаются заново при каждом открытии, и
    /// номер говорит об этом, даже когда экран открыт заново между двумя кадрами.
    opened: u64,
}

/// Следующий свободный номер открытия для нового `ScreenState`: у каждого состояния свой отрезок номеров, так что
/// новая партия не совпадает по номеру с экраном, открытым в прежней.
static NEXT_OPENED: AtomicU64 = AtomicU64::new(0);
/// Размер отрезка номеров одного `ScreenState`.
const OPENINGS_PER_STATE: u64 = 1 << 32;

/// Прокрутка одного списка: первая видимая строка и накопленная доля следующей строки. Строка
/// сменяется, только когда колесо набрало целую строку в одну сторону — и вниз, и вверх.
#[derive(Debug, Clone, Copy, Default)]
struct ListScroll {
    row: f64,
    pending: f64,
}

impl ScreenState {
    pub fn new(start_screen: ScreenId) -> Self {
        ScreenState {
            active: start_screen,
            previous: None,
            sound_enabled: true,
            scrolls: HashMap::new(),
            exit_requested: false,
            opened: NEXT_OPENED.fetch_add(OPENINGS_PER_STATE, Ordering::Relaxed),
        }
    }

    /// Номер последнего открытия экрана: растёт при каждой смене экрана, в том числе на тот же, а у нового
    /// `ScreenState` начинается с номера, которого не было ни у одного прежнего. Значим только
    /// порядок и равенство двух номеров, не само число.
    pub fn opened(&self) -> u64 {
        self.opened
    }

    /// На сколько строк прокручен список `element` экрана `screen`; не больше, чем позволяют его строки,
    /// ограничивает раскладка.
    pub fn list_offset(&self, screen: ScreenId, element: usize) -> f64 {
        self.scrolls
            .get(&(screen, element))
            .map_or(0.0, |scroll| scroll.row)
    }

    /// Сдвигает список на `rows` строк, но не дальше `max` — последних видимых строк.
    /// Доля строки копится, пока не наберётся целая; у края списка прокрутка за край не копится.
    fn scroll_list(&mut self, screen: ScreenId, element: usize, rows: f64, max: f64) {
        let scroll = self.scrolls.entry((screen, element)).or_default();
        scroll.row = scroll.row.min(max);
        scroll.pending += rows;
        let whole = scroll.pending.trunc();
        if whole != 0.0 {
            scroll.row = (scroll.row + whole).clamp(0.0, max);
            scroll.pending -= whole;
        }
        if (scroll.row <= 0.0 && scroll.pending < 0.0)
            || (scroll.row >= max && scroll.pending > 0.0)
        {
            scroll.pending = 0.0;
        }
    }

    fn reset_scrolls(&mut self) {
        self.scrolls.clear();
    }

    pub fn active(&self) -> ScreenId {
        self.active
    }

    pub fn sound_enabled(&self) -> bool {
        self.sound_enabled
    }

    /// «Экраны и состояние» → «Команды»: сработал ли `exit` — страница уходит к списку игр, редактор
    /// останавливает партию.
    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub fn is_live(&self, config: &ScreensConfig) -> bool {
        config.screens[self.active].world_runs
    }

    fn switch(&mut self, target: ScreenId, remember: bool) {
        self.previous = if remember { Some(self.active) } else { None };
        self.active = target;
        self.opened += 1;
    }
}

/// Moves to `target`, releasing held keys through `game` first when the transition leaves a
/// live screen for one that isn't — «Экраны и состояние» → «Жизнь партии».
fn switch_to(
    state: &mut ScreenState,
    config: &ScreensConfig,
    game: &mut Game,
    target: ScreenId,
    remember: bool,
) {
    let was_live = config.screens[state.active].world_runs;
    state.switch(target, remember);
    if was_live && !config.screens[target].world_runs {
        game.release_held_keys();
    }
}

/// Executes one button command. `config` resolves `show_screen`/`new_game` targets, already
/// checked to exist by the prestart validation, so no error path is needed here.
pub fn apply_command(
    cmd: ButtonCommand,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    apply_command_on(cmd, None, game, config, state);
}

/// То же для кнопки, у которой есть объект — строки списка или элемента с `for`: `run` передаёт его
/// функции кода.
pub fn apply_command_on(
    cmd: ButtonCommand,
    object: Option<u32>,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    match cmd {
        ButtonCommand::ShowScreen(target) => switch_to(state, config, game, target, true),
        ButtonCommand::NewGame(target, values_id) => {
            match values_id.and_then(|id| config.initial_values.get(id)) {
                Some(values) => game.new_game_with_values(values),
                None => game.new_game(),
            }
            state.reset_scrolls();
            switch_to(state, config, game, target, false);
        }
        ButtonCommand::Resume => {
            if let Some(previous) = state.previous {
                switch_to(state, config, game, previous, true);
            }
        }
        ButtonCommand::Quit => {
            game.quit();
            state.reset_scrolls();
            switch_to(state, config, game, config.start_screen, false);
        }
        ButtonCommand::Exit => {
            game.quit();
            state.reset_scrolls();
            switch_to(state, config, game, config.start_screen, false);
            state.exit_requested = true;
        }
        ButtonCommand::Run(run) => {
            if let Some(call) = config.runs.get(run) {
                game.run_screen_function(&call.function, object, &call.origin);
            }
        }
        // «Звук» → «Два вида звука»: touches no world property, no
        // screen, no queued anything — the whole command is this one flip.
        ButtonCommand::ToggleSound => state.sound_enabled = !state.sound_enabled,
    }
}

/// «Редактор», требование 27–28: same transition as `apply_command`'s own `NewGame` branch, for a
/// replay's resolved values — a `(имя объекта, свойство, значение)` list, rather than a
/// pre-registered `InitialValuesId`, since a replay's own values were never registered at load
/// time the way a button's or a screen key's own `new_game` third element is.
pub fn apply_replay_new_game(
    target: ScreenId,
    values: &[(String, PropertyId, Value)],
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    game.new_game_with_values(values);
    state.reset_scrolls();
    switch_to(state, config, game, target, false);
}

/// Stage-9 equivalent: switches to the win/loss screen the first time `game.outcome()` reports
/// one, and does nothing once already there. «Формат игры» / «Исполнение игры»: `end_game`
/// finishes the step, the screen switch happens here, on the step's boundary.
fn handle_outcome(game: &mut Game, config: &ScreensConfig, state: &mut ScreenState) {
    let Some((outcome, _step)) = game.outcome() else {
        return;
    };
    let target = match outcome {
        Outcome::Win => config.win_screen,
        Outcome::Loss => config.loss_screen,
    };
    if let Some(target) = target
        && state.active() != target
    {
        switch_to(state, config, game, target, true);
    }
}

/// One fixed-step tick: steps the world when the active screen is live (or resets the runner's
/// accumulator when it isn't), then applies the win/loss screen switch if a step just ended the
/// game. «Экраны и состояние» → «Жизнь партии».
pub fn tick(
    runner: &mut super::runner::Runner,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    dt_seconds: f64,
) {
    runner.advance_or_reset(game, dt_seconds, state.is_live(config));
    handle_outcome(game, config, state);
}

/// «Звук» → «Один вызов движка», пункт 5: writes the active screen's
/// music (or silence, if it names none) and the sound-enabled flag into the window — called once
/// per call, after both the shared mouse/screen-key queue has drained and this call's own steps
/// have run, so a click that just switched screens writes the screen it landed on, never the one
/// it left. «Экраны и состояние» → «Кому принадлежит состояние звука»: `game`'s own window
/// carries the result, `config`/`state` only supply what's currently true.
pub fn write_sound_frame(game: &mut Game, config: &ScreensConfig, state: &ScreenState) {
    let music = config.screens[state.active()].music;
    game.sound_window_mut()
        .write_header(music, state.sound_enabled());
}

/// Принимает ли мир ввод: экран с `world_runs` и скорость времени не 0 — «Исполнение игры» → «Шаг и
/// кадр». Клавиши и щелчки мира при остановленном времени отбрасываются.
fn world_accepts_input(game: &Game, config: &ScreensConfig, state: &ScreenState) -> bool {
    state.is_live(config) && !game.time_is_stopped()
}

/// Мир живого экрана остановлен скоростью времени 0: зажатые клавиши отпускаются, и отпускания идут в
/// запись, как при паузе.
fn release_keys_if_time_stopped(
    game: &mut Game,
    config: &ScreensConfig,
    state: &ScreenState,
    events: Option<&mut Vec<RecordedEvent>>,
) {
    if !state.is_live(config) || !game.time_is_stopped() {
        return;
    }
    let released = game.release_held_keys();
    if let Some(events) = events {
        events.extend(released.into_iter().map(RecordedEvent::WorldKeyUp));
    }
}

/// «Экраны и состояние» → «Клавиши экрана»: a key named in the active screen's own `keys` table
/// never reaches the world, on press or release — judged here against whichever screen is
/// active *right now*, at drain time, not when the raw event first arrived, so a key queued
/// before a click that switches screens still sees the screen the click lands on. Dropped
/// outright on a screen without `world_runs`, except a key the active screen names. `apply_now`
/// picks `Game::press_key` (a play/replay session, «Редактор», требование 1: applied to the world
/// the instant this call drains the queue, not deferred to the next `Game::step`'s own stage 1 —
/// a frame that ends up doing zero world steps still leaves the press visible, for `pause()`'s
/// own `release_held_keys` to find and record) over `Game::key_down` (the plain page: queued,
/// unchanged).
pub fn handle_screen_key_down(
    game: &mut Game,
    config: &ScreensConfig,
    state: &ScreenState,
    code: &str,
    events: Option<&mut Vec<RecordedEvent>>,
    apply_now: bool,
) {
    if config.screens[state.active()].keys.contains_key(code) {
        return;
    }
    if world_accepts_input(game, config, state) {
        if apply_now {
            game.press_key(code);
        } else {
            game.key_down(code);
        }
        if let Some(events) = events {
            events.push(RecordedEvent::WorldKeyDown(code.to_string()));
        }
    }
}

/// Mirrors `handle_screen_key_down`: an absorbed key's command runs right here, in this same
/// drain pass, instead of through a second queue of its own — drain order already puts it after
/// every mouse event queued ahead of it. `Game::release_key` runs unconditionally first: it
/// releases the key from the world only if the world actually holds it, and either way drops
/// whatever the page still has queued for it — see its own doc comment for exactly which case
/// that first part covers. A release the world never held on a screen that never absorbs it (its
/// press was itself absorbed, by this screen or an earlier one) is silently dropped by
/// `Game::key_up`/`Game::release_key` themselves in the non-absorbed branch below, so this
/// function does not need to track that case separately. `apply_now` — see `handle_screen_key_down`.
/// «Редактор», требование 2: when the world actually held `code` (checked *before* releasing it),
/// records a `WorldKeyUp` ahead of the `Command` — a replay applies both, in that order, so it
/// releases the key too, not just the screen's own command; `Game::release_held_keys` already
/// records this same release the other way a live screen can drop a still-held key (leaving a
/// live screen for good), so this is the matching case for one that stays live.
pub fn handle_screen_key_up(
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    code: &str,
    mut events: Option<&mut Vec<RecordedEvent>>,
    apply_now: bool,
) {
    if let Some(cmd) = config.screens[state.active()].keys.get(code).copied() {
        let was_held = game.is_key_held(code);
        game.release_key(code);
        if was_held && let Some(events) = events.as_deref_mut() {
            events.push(RecordedEvent::WorldKeyUp(code.to_string()));
        }
        apply_command(cmd, game, config, state);
        if let Some(events) = events {
            events.push(RecordedEvent::Command(cmd, None));
        }
        return;
    }
    if world_accepts_input(game, config, state) {
        if apply_now {
            game.release_key(code);
        } else {
            game.key_up(code);
        }
        if let Some(events) = events {
            events.push(RecordedEvent::WorldKeyUp(code.to_string()));
        }
    }
}

/// «Мышь в мире», требование 13: что под точкой — самая верхняя кнопка (её ключ) или, когда кнопки
/// под точкой нет, панель (она забирает нажатие у мира и никому его не отдаёт). Панель поверх кнопки,
/// как значок на табличке, нажатие у кнопки не отнимает. Надписи, шкалы, панели с `show_on_hover` и
/// панели-источники частиц без `name` прозрачны для мыши и сюда не попадают.
enum InterfaceHit<'a> {
    Button(Placed<'a>),
    /// Верхняя из панелей под точкой.
    Panel(Placed<'a>),
}

fn topmost_interface_hit<'a>(parts: &[Part<'a>], point: [f32; 2]) -> Option<InterfaceHit<'a>> {
    let mut topmost_panel = None;
    for part in parts.iter().rev() {
        let Part::Element(placed) = part else {
            continue;
        };
        if !placed.contains(point) {
            continue;
        }
        match placed.element {
            Element::Button { .. } => return Some(InterfaceHit::Button(*placed)),
            // «Наведение»: панель с `show_on_hover` прозрачна для мыши, видна она сейчас или нет.
            // «На экране»: и панель-источник частиц без `name`.
            Element::Panel {
                show_on_hover: None,
                ..
            } if !placed.element.passes_mouse() => {
                topmost_panel.get_or_insert(*placed);
            }
            _ => {}
        }
    }
    topmost_panel.map(InterfaceHit::Panel)
}

/// «Интерфейс игры» → «Наведение»: номер элемента экрана — места наведения, над которым курсор. Место —
/// то, что получило бы нажатие: кнопка под точкой, а без кнопки верхняя панель; если у неё нет `name`,
/// курсор ни над каким местом. `point` — в точках `screens.json`; `None` — курсор за краем окна.
pub fn hover_place(parts: &[Part<'_>], point: Option<[f32; 2]>) -> Option<usize> {
    let placed = match topmost_interface_hit(parts, point?)? {
        InterfaceHit::Button(placed) | InterfaceHit::Panel(placed) => placed,
    };
    placed.element.place_name()?;
    Some(placed.key.element)
}

/// «Интерфейс игры» → «Мышь»: стоит ли точка окна над кнопкой, панелью или списком со строками
/// активного экрана. Колесо и правая и средняя кнопки над ними камеру игрока не трогают. `window` и
/// `point` — в точках окна, как их видит страница; масштаб интерфейса учтён здесь.
pub fn over_interface(
    config: &ScreensConfig,
    state: &ScreenState,
    world: &World,
    window: [f32; 2],
    point: [f32; 2],
) -> bool {
    let screen = &config.screens[state.active()];
    let frame = config.ui_frame(window);
    let point = frame.point(point);
    interface::list_under(screen, world, frame.viewport, point).is_some()
        || topmost_interface_hit(
            &interface::layout(screen, state.active(), world, state, frame.viewport),
            point,
        )
        .is_some()
}

/// «Интерфейс игры» → «Список»: колесо на `delta_y` точек над списком сдвигает его строки — на одну за
/// каждые сто точек. `true`, если под точкой список со строками: камера колесо тогда не получает, даже
/// когда все строки видны.
pub fn scroll_wheel(
    config: &ScreensConfig,
    state: &mut ScreenState,
    world: &World,
    window: [f32; 2],
    point: [f32; 2],
    delta_y: f64,
) -> bool {
    let screen_id = state.active();
    let screen = &config.screens[screen_id];
    let frame = config.ui_frame(window);
    let Some(index) = interface::list_under(screen, world, frame.viewport, frame.point(point))
    else {
        return false;
    };
    if let Element::List {
        placement,
        for_,
        row_height,
        gap,
        sort_by,
        descending,
        ..
    } = &screen.elements[index]
        && delta_y.is_finite()
    {
        let rows = interface::list_rows(for_, *sort_by, *descending, world).len();
        let visible = interface::visible_rows(placement.size[1], *row_height, *gap);
        let max = rows.saturating_sub(visible) as f64;
        state.scroll_list(screen_id, index, delta_y / SCROLL_POINTS_PER_ROW, max);
    }
    true
}

/// What handling one queued mouse event resolved to — «Интерфейс игры» → «Мышь», «Мышь в мире»,
/// требования 13–20: a button's own command, or a press/release that reached the world as
/// `MouseLeft` (its own `"cursor"`/`on_click` handling lives in `core::step::apply_input`, this
/// layer only decides whether the event reaches the world at all).
enum MouseOutcome {
    None,
    Command(ButtonCommand, Option<u32>),
    /// Нажатие дошло до мира; объект — тот, чья картинка в мире стояла под курсором.
    WorldPress(Option<u32>),
    WorldRelease,
}

/// Нажатие левой кнопки: его забирает кнопка или панель под курсором, иначе на живом экране — мир.
fn press_outcome(
    mouse: &mut MouseState,
    parts: &[Part<'_>],
    world_accepts: bool,
    image_object: Option<u32>,
) -> MouseOutcome {
    match topmost_interface_hit(parts, mouse.position) {
        Some(InterfaceHit::Button(placed)) => {
            mouse.captured = Some(placed.key);
            MouseOutcome::None
        }
        Some(InterfaceHit::Panel(_)) => MouseOutcome::None,
        None if world_accepts => {
            mouse.world_captured = true;
            MouseOutcome::WorldPress(image_object)
        }
        None => MouseOutcome::None,
    }
}

/// Applies one queued mouse event to `mouse`'s hover/capture state against `screen`'s elements —
/// «Интерфейс игры» → «Мышь», «Мышь в мире», требования 13–16. A press over a button or panel
/// never reaches the world; elsewhere, on a live screen, it does, as `MouseLeft` — and the
/// matching release goes wherever that press went, regardless of where the cursor sits by then.
fn handle_mouse_event(
    mouse: &mut MouseState,
    parts: &[Part<'_>],
    world_accepts: bool,
    event: MouseEvent,
) -> MouseOutcome {
    match event {
        MouseEvent::Move(pos) => {
            mouse.position = pos;
            if mouse.captured.is_none() {
                mouse.hover = match topmost_interface_hit(parts, pos) {
                    Some(InterfaceHit::Button(placed)) => Some(placed.key),
                    _ => None,
                };
            }
            MouseOutcome::None
        }
        MouseEvent::Down(image_object) => {
            mouse.presses = mouse.presses.wrapping_add(1);
            press_outcome(mouse, parts, world_accepts, image_object)
        }
        MouseEvent::Up => {
            mouse.releases = mouse.releases.wrapping_add(1);
            if mouse.world_captured {
                mouse.world_captured = false;
                return MouseOutcome::WorldRelease;
            }
            let Some(captured) = mouse.captured.take() else {
                return MouseOutcome::None;
            };
            // Команда срабатывает, только если под указателем кнопка, на которой нажали, — тот же
            // элемент и тот же объект: строки списка могли сдвинуться.
            let released_on = parts.iter().find_map(|part| match part {
                Part::Element(placed)
                    if placed.key == captured && placed.contains(mouse.position) =>
                {
                    match placed.element {
                        Element::Button { on_click, .. } => Some((*on_click, placed.object)),
                        _ => None,
                    }
                }
                _ => None,
            });
            match released_on {
                Some((command, object)) => MouseOutcome::Command(command, object),
                None => MouseOutcome::None,
            }
        }
    }
}

/// «Редактор», требования 27, 39: one thing a play/replay session's own recording keeps —
/// world-bound key presses/releases (screen-absorbed ones never reach the world, so never appear
/// here) and a screen command once it actually fires (from a button click or a screen key alike),
/// already resolved rather than the raw click that caused it. `process_ui_queue`/`engine_call`
/// (the plain page's own paths) pass `None` for it — «Редактор», нефункциональное требование:
/// recording costs the page nothing, not even a throwaway `Vec` copied into on every key.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordedEvent {
    WorldKeyDown(String),
    WorldKeyUp(String),
    /// «Надписи, полоски и картинки в мире», требование 7: нажатие левой кнопки по картинке в мире —
    /// номер объекта, чья она; повтор щёлкает по тому же объекту.
    WorldClick(u32),
    /// Команда и объект кнопки — строки списка или элемента с `for`, если он был.
    Command(ButtonCommand, Option<u32>),
}

/// Drains the shared mouse/screen-key queue, judging every event — mouse or keyboard — against
/// whichever screen is active at the moment it is *this* event's turn, not when it arrived. A
/// click that switches the screen mid-drain leaves later events in the same batch, key or mouse,
/// to land on the new one, same as the browser delivering them one at a time would — «Экраны и
/// состояние» → «Клавиши экрана».
pub fn process_ui_queue(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
) {
    process_ui_queue_recording(queue, mouse, game, config, state, viewport, None, false);
}

/// Same as `process_ui_queue`, additionally appending every world key and fired command to
/// `events` (when given one), in drain order — «Редактор», требование 27. `apply_now` — see
/// `handle_screen_key_down`: `true` for a play/replay session's own paths, `false` (unchanged)
/// for the plain page's `process_ui_queue`.
#[allow(clippy::too_many_arguments)]
pub fn process_ui_queue_recording(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
    mut events: Option<&mut Vec<RecordedEvent>>,
    apply_now: bool,
) {
    for event in queue.drain() {
        match event {
            UiEvent::Mouse(mouse_event) => {
                let screen_id = state.active();
                let frame = config.ui_frame(viewport);
                let parts = interface::layout(
                    &config.screens[screen_id],
                    screen_id,
                    &game.world,
                    state,
                    frame.viewport,
                );
                let mouse_event = match mouse_event {
                    MouseEvent::Move(pos) => MouseEvent::Move(frame.point(pos)),
                    other => other,
                };
                let accepts = world_accepts_input(game, config, state);
                match handle_mouse_event(mouse, &parts, accepts, mouse_event) {
                    MouseOutcome::Command(cmd, object) => {
                        apply_command_on(cmd, object, game, config, state);
                        if let Some(events) = events.as_deref_mut() {
                            events.push(RecordedEvent::Command(cmd, object));
                        }
                    }
                    // «Мышь в мире», требование 21: тем же путём, что клавиша экрана без
                    // команды — этап 2 шага (очередь) или сразу (`apply_now`, партия/повтор в
                    // редакторе), а не своей отдельной очередью.
                    MouseOutcome::WorldPress(image_object) => {
                        match (image_object, apply_now) {
                            (Some(object), true) => game.press_key_on("MouseLeft", object),
                            (Some(object), false) => game.key_down_on("MouseLeft", object),
                            (None, true) => game.press_key("MouseLeft"),
                            (None, false) => game.key_down("MouseLeft"),
                        }
                        if let Some(events) = events.as_deref_mut() {
                            events.push(match image_object {
                                Some(object) => RecordedEvent::WorldClick(object),
                                None => RecordedEvent::WorldKeyDown("MouseLeft".to_string()),
                            });
                        }
                    }
                    MouseOutcome::WorldRelease => {
                        if apply_now {
                            game.release_key("MouseLeft");
                        } else {
                            game.key_up("MouseLeft");
                        }
                        if let Some(events) = events.as_deref_mut() {
                            events.push(RecordedEvent::WorldKeyUp("MouseLeft".to_string()));
                        }
                    }
                    MouseOutcome::None => {}
                }
            }
            UiEvent::KeyDown(code) => {
                handle_screen_key_down(game, config, state, &code, events.as_deref_mut(), apply_now)
            }
            UiEvent::KeyUp(code) => {
                handle_screen_key_up(game, config, state, &code, events.as_deref_mut(), apply_now)
            }
        }
    }
}

/// The one call every `tick` makes, once per `requestAnimationFrame` — «Исполнение игры» →
/// «Шаг и кадр», «Звук» → «Один вызов движка»: drain the shared mouse/screen-key queue first (a
/// click's own screen switch, and a key the new screen doesn't claim, are both visible to this
/// same call's steps below, not only a later call's), advance the fixed-step simulation, then
/// write what the sound window should say — after both, so a click that just changed screen
/// writes the screen it landed on. `wasm::Engine::tick` calls this and then draws a frame; native
/// code (tests included) calls it directly instead of repeating the three calls by hand, so a
/// change to this order is exercised the same way in both.
#[allow(clippy::too_many_arguments)]
pub fn engine_call(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    runner: &mut super::runner::Runner,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
    dt_seconds: f64,
) {
    engine_call_recording(
        queue, mouse, runner, game, config, state, viewport, dt_seconds, None, false,
    );
}

/// Same as `engine_call`, for a play session that also records — «Редактор», требование 27, 39:
/// the page never calls this, so the recording machinery costs it nothing beyond `engine_call`'s
/// own `None`. `apply_now` — see `process_ui_queue_recording`.
#[allow(clippy::too_many_arguments)]
pub fn engine_call_recording(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    runner: &mut super::runner::Runner,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
    dt_seconds: f64,
    mut events: Option<&mut Vec<RecordedEvent>>,
    apply_now: bool,
) {
    process_ui_queue_recording(
        queue,
        mouse,
        game,
        config,
        state,
        viewport,
        events.as_deref_mut(),
        apply_now,
    );
    release_keys_if_time_stopped(game, config, state, events);
    tick(runner, game, config, state, dt_seconds);
    write_sound_frame(game, config, state);
}

/// «Редактор», требования 40, 47: exactly one world step, bypassing `Runner`'s real-time
/// accumulator entirely — the editor's own "Шаг", and a replay's seek/step-forward, both drive the
/// world at their own pace, not the browser's frame clock. Drains the shared queue first, same as
/// `engine_call`, so a button click or a screen key queued right before still lands on whichever
/// screen is active when this call runs. Does nothing to the world on a screen without
/// `world_runs`, or once the partiya already ended — «Экраны и состояние» → «Жизнь партии».
/// Returns whether a step actually ran. Does not clear the sound window's marks itself — «Редактор»,
/// требование 6: a replay's own real-time burst (`PlaySession::tick_replay`) calls this several
/// times per animation frame and has to let marks accumulate across all of them, the way the page's
/// own `Runner::advance_or_reset` already clears once before its own multi-step burst rather than
/// per step; each caller of this function clears marks itself, once, at the granularity it needs.
#[allow(clippy::too_many_arguments)]
pub fn advance_one_step(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
    events: Option<&mut Vec<RecordedEvent>>,
    apply_now: bool,
) -> bool {
    process_ui_queue_recording(
        queue, mouse, game, config, state, viewport, events, apply_now,
    );
    let stepped = world_accepts_input(game, config, state) && game.is_running();
    if stepped {
        let input = game.take_input_snapshot();
        game.step(input);
    }
    handle_outcome(game, config, state);
    write_sound_frame(game, config, state);
    stepped
}

/// Which of the three fills a button currently shows — pressed wins over hover, and no button
/// hovers while another one holds the capture. «Интерфейс игры» → «Мышь», «Картинки» → «Где
/// появляется картинка».
pub fn button_fill<'a>(
    fill: &'a Fill,
    fill_hover: &'a Fill,
    fill_pressed: &'a Fill,
    key: ButtonKey,
    mouse: &MouseState,
) -> &'a Fill {
    if mouse.captured == Some(key) {
        fill_pressed
    } else if mouse.captured.is_none() && mouse.hover == Some(key) {
        fill_hover
    } else {
        fill
    }
}

#[cfg(test)]
mod opened_tests {
    use super::*;

    #[test]
    fn a_fresh_state_never_shares_an_opening_with_another_one() {
        let mut old = ScreenState::new(0);
        let fresh = ScreenState::new(0);
        assert_ne!(old.opened(), fresh.opened());
        old.switch(0, false);
        assert_ne!(old.opened(), fresh.opened());
        let before = fresh.opened();
        let mut fresh = fresh;
        fresh.switch(0, false);
        assert!(
            fresh.opened() > before,
            "каждая смена экрана — новое открытие"
        );
    }
}

#[cfg(test)]
mod list_scroll_tests {
    use super::*;

    #[test]
    fn a_row_changes_after_a_whole_row_of_the_wheel_in_both_directions() {
        let mut state = ScreenState::new(0);
        state.scroll_list(0, 3, 1.0, 5.0);
        assert_eq!(state.list_offset(0, 3), 1.0);
        state.scroll_list(0, 3, -0.01, 5.0);
        assert_eq!(
            state.list_offset(0, 3),
            1.0,
            "вверх на одну точку — строка та же"
        );
        state.scroll_list(0, 3, -0.99, 5.0);
        assert_eq!(
            state.list_offset(0, 3),
            0.0,
            "вверх на целую строку — на строку выше"
        );
        state.scroll_list(0, 3, 0.6, 5.0);
        assert_eq!(state.list_offset(0, 3), 0.0);
        state.scroll_list(0, 3, 0.4, 5.0);
        assert_eq!(state.list_offset(0, 3), 1.0);
    }

    #[test]
    fn scrolling_past_an_edge_is_not_banked() {
        let mut state = ScreenState::new(0);
        state.scroll_list(0, 0, -3.5, 4.0);
        assert_eq!(state.list_offset(0, 0), 0.0);
        state.scroll_list(0, 0, 0.6, 4.0);
        assert_eq!(
            state.list_offset(0, 0),
            0.0,
            "прокрутка за верх не ушла в долг"
        );
        state.scroll_list(0, 0, 9.0, 4.0);
        assert_eq!(state.list_offset(0, 0), 4.0);
        state.scroll_list(0, 0, -0.5, 4.0);
        assert_eq!(state.list_offset(0, 0), 4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::input::MouseState;

    #[test]
    fn button_fill_defaults_hover_and_pressed_to_the_base_image_fill() {
        // «Картинки»: «Нет своей картинки на вид — во всех трёх видах показывается image».
        let base = Fill::Image {
            image: 3,
            opacity: 0.8,
        };
        let mouse = MouseState {
            hover: Some(ButtonKey::plain(0)),
            captured: None,
            ..Default::default()
        };
        let shown = button_fill(&base, &base, &base, ButtonKey::plain(0), &mouse);
        match shown {
            Fill::Image { image, opacity } => {
                assert_eq!(*image, 3);
                assert_eq!(*opacity, 0.8);
            }
            Fill::Color(_) => panic!("должна остаться картинка"),
        }
    }

    #[test]
    fn button_fill_prefers_its_own_image_hover_over_the_base_fill() {
        let base = Fill::Image {
            image: 1,
            opacity: 1.0,
        };
        let hover = Fill::Image {
            image: 2,
            opacity: 1.0,
        };
        let pressed = base;
        let mouse = MouseState {
            hover: Some(ButtonKey::plain(0)),
            captured: None,
            ..Default::default()
        };
        let shown = button_fill(&base, &hover, &pressed, ButtonKey::plain(0), &mouse);
        assert!(matches!(shown, Fill::Image { image: 2, .. }));
    }

    #[test]
    fn button_fill_pressed_wins_over_hover() {
        let base = Fill::Color([0.0, 0.0, 0.0, 1.0]);
        let hover = Fill::Color([0.5, 0.5, 0.5, 1.0]);
        let pressed = Fill::Color([1.0, 1.0, 1.0, 1.0]);
        let mouse = MouseState {
            hover: Some(ButtonKey::plain(0)),
            captured: Some(ButtonKey::plain(0)),
            ..Default::default()
        };
        let shown = button_fill(&base, &hover, &pressed, ButtonKey::plain(0), &mouse);
        assert!(matches!(shown, Fill::Color([1.0, 1.0, 1.0, 1.0])));
    }

    #[test]
    fn button_fill_of_a_different_button_ignores_this_ones_capture() {
        let base = Fill::Color([0.0, 0.0, 0.0, 1.0]);
        let hover = Fill::Color([0.5, 0.5, 0.5, 1.0]);
        let pressed = Fill::Color([1.0, 1.0, 1.0, 1.0]);
        // Another button (index 1) holds the capture — button 0 shows neither hover nor pressed.
        let mouse = MouseState {
            hover: Some(ButtonKey::plain(0)),
            captured: Some(ButtonKey::plain(1)),
            ..Default::default()
        };
        let shown = button_fill(&base, &hover, &pressed, ButtonKey::plain(0), &mouse);
        assert!(matches!(shown, Fill::Color([0.0, 0.0, 0.0, 1.0])));
    }

    fn placement(anchor: Anchor, offset: [f32; 2], size: [f32; 2]) -> Placement {
        Placement {
            anchor,
            offset,
            size,
        }
    }

    #[test]
    fn top_left_anchor_ignores_window_size() {
        let p = placement(Anchor::TopLeft, [10.0, 20.0], [50.0, 30.0]);
        assert_eq!(p.top_left([800.0, 600.0]), [10.0, 20.0]);
        assert_eq!(p.top_left([320.0, 200.0]), [10.0, 20.0]);
    }

    #[test]
    fn bottom_right_anchor_hugs_the_far_corner_inward() {
        let p = placement(Anchor::BottomRight, [16.0, 16.0], [100.0, 40.0]);
        // right edge sits 16px from the window's right edge, bottom 16px from the bottom.
        let [x, y] = p.top_left([800.0, 600.0]);
        assert_eq!(x, 800.0 - 100.0 - 16.0);
        assert_eq!(y, 600.0 - 40.0 - 16.0);
    }

    #[test]
    fn center_anchor_offsets_from_the_middle_at_three_window_sizes() {
        let p = placement(Anchor::Center, [0.0, -60.0], [400.0, 48.0]);
        for &(w, h) in &[(800.0, 600.0), (1920.0, 1080.0), (360.0, 640.0)] {
            let [x, y] = p.top_left([w, h]);
            assert_eq!(x, (w - 400.0) / 2.0);
            assert_eq!(y, (h - 48.0) / 2.0 - 60.0);
        }
    }

    #[test]
    fn edge_midpoint_anchors_center_along_the_tangential_axis() {
        let top = placement(Anchor::Top, [0.0, 10.0], [200.0, 20.0]);
        let [x, y] = top.top_left([800.0, 600.0]);
        assert_eq!(x, (800.0 - 200.0) / 2.0);
        assert_eq!(y, 10.0);

        let left = placement(Anchor::Left, [5.0, 0.0], [30.0, 200.0]);
        let [x, y] = left.top_left([800.0, 600.0]);
        assert_eq!(x, 5.0);
        assert_eq!(y, (600.0 - 200.0) / 2.0);
    }

    #[test]
    fn contains_matches_the_resolved_rectangle() {
        let p = placement(Anchor::TopLeft, [0.0, 0.0], [100.0, 50.0]);
        let viewport = [800.0, 600.0];
        assert!(p.contains(viewport, [50.0, 25.0]));
        assert!(!p.contains(viewport, [150.0, 25.0]));
        assert!(!p.contains(viewport, [50.0, 60.0]));
    }

    /// All nine anchors, at three window sizes — «Тесты»: «Раскладка: девять якорей при трёх
    /// размерах окна».
    #[test]
    fn all_nine_anchors_resolve_at_three_window_sizes() {
        let anchors = [
            Anchor::TopLeft,
            Anchor::Top,
            Anchor::TopRight,
            Anchor::Left,
            Anchor::Center,
            Anchor::Right,
            Anchor::BottomLeft,
            Anchor::Bottom,
            Anchor::BottomRight,
        ];
        for &(w, h) in &[(320.0, 240.0), (800.0, 600.0), (1920.0, 1080.0)] {
            for &anchor in &anchors {
                let p = placement(anchor, [4.0, 6.0], [40.0, 20.0]);
                let [x, y] = p.top_left([w, h]);
                // Every resolved rectangle must fit inside the window — the anchor names are
                // there precisely so an element never has to be placed off-window on purpose.
                assert!(x >= -1e-6 && x + 40.0 <= w + 1e-6, "{anchor:?} x={x} w={w}");
                assert!(y >= -1e-6 && y + 20.0 <= h + 1e-6, "{anchor:?} y={y} h={h}");
            }
        }
    }

    fn table_with(names_and_kinds: &[(&str, PropKind)]) -> PropertyTable {
        let mut table = PropertyTable::new();
        for &(name, kind) in names_and_kinds {
            table.declare_author(name, kind).unwrap();
        }
        table
    }

    #[test]
    fn format_text_substitutes_number_time_flag_and_literal() {
        let properties = table_with(&[
            ("score", PropKind::Number),
            ("elapsed", PropKind::Time),
            ("on", PropKind::Flag),
        ]);
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_text(id, property::NAME, "head".to_string());
        world.set_number(id, properties.resolve("score").unwrap(), 12.0);
        world.set_time(id, properties.resolve("elapsed").unwrap(), 90); // 1.5s at 60 steps/s
        world.set_flag(id, properties.resolve("on").unwrap(), true);

        let parts = vec![
            TextPart::Literal("Счёт: ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("score").unwrap(),
            },
            TextPart::Literal(" за ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("elapsed").unwrap(),
            },
            TextPart::Literal("с, вкл: ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("on").unwrap(),
            },
        ];
        assert_eq!(
            format_text(&parts, &world, &properties),
            "Счёт: 12 за 1.5с, вкл: да"
        );
    }

    #[test]
    fn format_text_substitutes_empty_when_the_named_object_is_gone() {
        let properties = table_with(&[("score", PropKind::Number)]);
        let world = World::new(&properties);
        let parts = vec![
            TextPart::Literal("Счёт: ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("score").unwrap(),
            },
        ];
        assert_eq!(format_text(&parts, &world, &properties), "Счёт: ");
    }
}

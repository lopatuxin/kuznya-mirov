//! «Интерфейс игры» → «Элемент при объекте», «Список»: раскладка элементов активного экрана по миру —
//! какие элементы есть сейчас, где они стоят в окне и какому объекту принадлежат. Рисование, попадание
//! мыши и колесо считают по одной раскладке.

use std::cmp::Ordering;

use super::input::{ButtonKey, MouseState};
use super::property::{PropertyId, PropertyTable};
use super::rules::Selector;
use super::screens::{Element, Screen, ScreenId, ScreenState, TooltipStyle, format_text_for};
use super::world::World;
use super::world_elements::bar_fraction;

/// Сколько точек колеса сдвигают список на одну строку.
pub(crate) const SCROLL_POINTS_PER_ROW: f64 = 100.0;
/// Ширина полосы прокрутки у правого края списка, в точках.
pub(crate) const SCROLL_BAR_WIDTH: f32 = 4.0;

/// Панель, надпись или кнопка на своём месте в окне.
#[derive(Debug, Clone, Copy)]
pub struct Placed<'a> {
    pub element: &'a Element,
    /// `[x, y, ширина, высота]` в точках окна.
    pub rect: [f32; 4],
    /// Объект элемента с `for` или строки списка, которой принадлежит элемент.
    pub object: Option<u32>,
    pub key: ButtonKey,
    /// «Выбор»: кнопка заперта — в её группе `needs_choice` ничего не выбрано.
    pub locked: bool,
}

impl Placed<'_> {
    pub fn contains(&self, point: [f32; 2]) -> bool {
        contains(self.rect, point)
    }
}

/// Что рисуется и принимает мышь на экране, в порядке рисования.
#[derive(Debug, Clone, Copy)]
pub enum Part<'a> {
    Element(Placed<'a>),
    ScrollBar { rect: [f32; 4], color: [f32; 4] },
}

fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}

/// Подходящий отбору живой объект с меньшим номером.
fn lowest_match(selector: &Selector, world: &World) -> Option<u32> {
    world
        .ids()
        .find(|&id| world.has_all(id, &selector.has) && world.has_none(id, &selector.without))
}

/// Объекты строк списка по порядку: по возрастанию `sort_by` (по убыванию с `descending`), без свойства —
/// в конце, при равных и без `sort_by` — по номеру.
pub fn list_rows(
    selector: &Selector,
    sort_by: Option<PropertyId>,
    descending: bool,
    world: &World,
) -> Vec<u32> {
    let mut rows: Vec<(u32, Option<f64>)> = world
        .ids()
        .filter(|&id| world.has_all(id, &selector.has) && world.has_none(id, &selector.without))
        .map(|id| (id, sort_by.and_then(|prop| world.number_like(id, prop))))
        .collect();
    if sort_by.is_some() {
        rows.sort_by(|a, b| match (a.1, b.1) {
            (Some(x), Some(y)) => {
                let order = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                let order = if descending { order.reverse() } else { order };
                order.then(a.0.cmp(&b.0))
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.0.cmp(&b.0),
        });
    }
    rows.into_iter().map(|(id, _)| id).collect()
}

/// Сколько целых строк помещается в список высотой `height`.
pub fn visible_rows(height: f32, row_height: f32, gap: f32) -> usize {
    let rows = (f64::from(height) + f64::from(gap)) / (f64::from(row_height) + f64::from(gap));
    (rows + 1e-6).floor().max(0.0) as usize
}

/// Номер первой видимой строки при сдвиге `offset` строк: целая часть сдвига, от первой строки до
/// последних `visible`.
pub fn first_row(offset: f64, rows: usize, visible: usize) -> usize {
    (offset.trunc().max(0.0) as usize).min(rows.saturating_sub(visible))
}

/// Список под точкой окна: самый верхний по порядку элементов экрана из тех, у кого есть строки. Список
/// без строк ничего не рисует и мышь не ловит, как элемент с `for` без своего объекта.
pub fn list_under(
    screen: &Screen,
    world: &World,
    viewport: [f32; 2],
    point: [f32; 2],
) -> Option<usize> {
    screen
        .elements
        .iter()
        .enumerate()
        .rev()
        .find(|(_, element)| {
            matches!(element, Element::List { placement, for_, .. }
                if placement.contains(viewport, point) && lowest_match(for_, world).is_some())
        })
        .map(|(index, _)| index)
}

/// Раскладка экрана `screen_id`: элементы с `for` — только пока есть объект отбора, списки — видимыми
/// строками со своей полосой прокрутки.
pub fn layout<'a>(
    screen: &'a Screen,
    screen_id: ScreenId,
    world: &World,
    state: &ScreenState,
    viewport: [f32; 2],
) -> Vec<Part<'a>> {
    let mut parts = Vec::with_capacity(screen.elements.len());
    for (index, element) in screen.elements.iter().enumerate() {
        let placement = element.placement();
        let [x, y] = placement.top_left(viewport);
        let [width, height] = placement.size;
        let Element::List {
            for_,
            row_height,
            gap,
            sort_by,
            descending,
            row,
            scroll_color,
            ..
        } = element
        else {
            let object = match element.selector() {
                Some(selector) => match lowest_match(selector, world) {
                    Some(id) => Some(id),
                    None => continue,
                },
                None => None,
            };
            parts.push(Part::Element(Placed {
                element,
                rect: [x, y, width, height],
                object,
                key: ButtonKey {
                    element: index,
                    object,
                    inner: 0,
                },
                locked: state.is_locked(element),
            }));
            continue;
        };
        let rows = list_rows(for_, *sort_by, *descending, world);
        let visible = visible_rows(height, *row_height, *gap);
        let first = first_row(state.list_offset(screen_id, index), rows.len(), visible);
        for (slot, &object) in rows.iter().skip(first).take(visible).enumerate() {
            let top = y + slot as f32 * (row_height + gap);
            for (inner, part) in row.iter().enumerate() {
                let own = part.placement();
                parts.push(Part::Element(Placed {
                    element: part,
                    rect: [
                        x + own.offset[0],
                        top + own.offset[1],
                        own.size[0],
                        own.size[1],
                    ],
                    object: Some(object),
                    key: ButtonKey {
                        element: index,
                        object: Some(object),
                        inner,
                    },
                    locked: state.is_locked(part),
                }));
            }
        }
        if visible > 0 && rows.len() > visible {
            let total = rows.len() as f32;
            parts.push(Part::ScrollBar {
                rect: [
                    x + width - SCROLL_BAR_WIDTH,
                    y + height * first as f32 / total,
                    SCROLL_BAR_WIDTH,
                    height * visible as f32 / total,
                ],
                color: *scroll_color,
            });
        }
    }
    parts
}

/// «Шкала»: доля заливки шкалы `element` у объекта `object`; `None` — не шкала, нет объекта или у него нет
/// свойства `value` или `max`: шкала не рисуется.
pub fn bar_share(element: &Element, object: Option<u32>, world: &World) -> Option<f32> {
    let Element::Bar { value, max, .. } = element else {
        return None;
    };
    bar_fraction(world, object?, *value, max)
}

/// «Шкала»: прямоугольник заливки — от левого края на долю `share`; при нуле заливки нет.
pub fn bar_fill_rect(rect: [f32; 4], share: f32) -> Option<[f32; 4]> {
    (share > 0.0).then(|| [rect[0], rect[1], rect[2] * share, rect[3]])
}

/// «Подсказка»: что показать — текст и прямоугольник элемента, над которым он всплывает.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipShow {
    pub text: String,
    pub element_rect: [f32; 4],
}

/// «Подсказка»: над каким элементом курсор и с какого времени окна; нажатие прячет подсказку, пока
/// курсор не уйдёт с элемента. Не входит в запись партии — это не ввод.
#[derive(Debug, Clone, Copy, Default)]
pub struct TooltipHover {
    target: Option<(ScreenId, ButtonKey)>,
    /// Место элемента под курсором: кнопка, которую щелчок подменил другой на том же месте (синяя
    /// табличка — кремовой), остаётся тем же элементом, и курсор с неё не уходил. Другой элемент на том же
    /// месте без щелчка — строки списка прокрутили, табличку сменила клавиша — снова ждёт задержку.
    rect: Option<[f32; 4]>,
    since: f64,
    presses: u32,
    releases: u32,
    hidden: bool,
}

impl TooltipHover {
    /// Подсказка этого кадра. `pointer` — курсор в точках `screens.json` (`None`, пока мышь не
    /// двигалась), `mouse` — счёт нажатий и отпусканий, `now` — секунды времени окна, которое идёт и на
    /// паузе. Команда кнопки выполняется при отпускании в том же кадре, поэтому кнопка, которую она сменила,
    /// встаёт на место в кадре, где счёт отпусканий изменился.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        parts: &[Part<'_>],
        screen: ScreenId,
        pointer: Option<[f32; 2]>,
        mouse: &MouseState,
        now: f64,
        style: &TooltipStyle,
        world: &World,
        properties: &PropertyTable,
    ) -> Option<TooltipShow> {
        let placed = pointer.and_then(|point| tooltip_under(parts, point, world));
        let target = placed.map(|placed| (screen, placed.key));
        let rect = placed.map(|placed| placed.rect);
        let pressed = mouse.presses != self.presses;
        let clicked = pressed || mouse.releases != self.releases;
        let swapped_by_click = clicked && rect.is_some() && rect == self.rect;
        if target != self.target && !swapped_by_click {
            *self = TooltipHover {
                target,
                rect,
                since: now,
                presses: mouse.presses,
                releases: mouse.releases,
                hidden: false,
            };
        } else {
            self.target = target;
            self.rect = rect;
            self.releases = mouse.releases;
            if pressed {
                self.presses = mouse.presses;
                self.hidden = true;
            }
        }
        let placed = placed?;
        if self.hidden || now - self.since < style.delay {
            return None;
        }
        let text = format_text_for(placed.element.tooltip()?, world, properties, placed.object);
        (!text.is_empty()).then_some(TooltipShow {
            text,
            element_rect: placed.rect,
        })
    }
}

/// Элемент с подсказкой под точкой: верхний по порядку рисования из тех, у кого она есть, — значок без
/// подсказки поверх кнопки её не заслоняет. Шкала, которую не рисуют (у объекта нет `value` или `max`),
/// подсказки не даёт.
fn tooltip_under<'a>(parts: &[Part<'a>], point: [f32; 2], world: &World) -> Option<Placed<'a>> {
    parts.iter().rev().find_map(|part| match part {
        Part::Element(placed)
            if placed.element.tooltip().is_some()
                && placed.contains(point)
                && (!matches!(placed.element, Element::Bar { .. })
                    || bar_share(placed.element, placed.object, world).is_some()) =>
        {
            Some(*placed)
        }
        _ => None,
    })
}

/// «Подсказка»: плашка под текст шириной `text_width` и строкой высотой `line_height` — посередине над
/// элементом, низом на `gap` выше его верха; сверху не помещается в окно `viewport` — под элементом на
/// том же `gap`; по ширине прижата внутрь окна.
pub fn tooltip_rect(
    element: [f32; 4],
    text_width: f32,
    line_height: f32,
    style: &TooltipStyle,
    viewport: [f32; 2],
) -> [f32; 4] {
    let width = text_width + 2.0 * style.padding[0];
    let height = line_height + 2.0 * style.padding[1];
    let centered = element[0] + (element[2] - width) / 2.0;
    let x = centered.min(viewport[0] - width).max(0.0);
    let above = element[1] - style.gap - height;
    let y = if above >= 0.0 {
        above
    } else {
        element[1] + element[3] + style.gap
    };
    [x, y, width, height]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fit_whole_and_the_gap_counts_between_them() {
        assert_eq!(visible_rows(300.0, 64.0, 4.0), 4);
        assert_eq!(visible_rows(68.0, 64.0, 4.0), 1);
        assert_eq!(visible_rows(63.0, 64.0, 4.0), 0);
        assert_eq!(visible_rows(136.0, 64.0, 8.0), 2);
    }

    #[test]
    fn the_first_row_stays_between_the_start_and_the_last_screenful() {
        assert_eq!(first_row(0.0, 7, 4), 0);
        assert_eq!(first_row(2.0, 7, 4), 2);
        assert_eq!(first_row(9.0, 7, 4), 3);
        assert_eq!(first_row(1.0, 3, 4), 0);
        assert_eq!(first_row(-2.0, 7, 4), 0);
    }

    #[test]
    fn a_row_moves_only_after_a_whole_hundred_points_of_the_wheel() {
        let offset = |points: f64| points / SCROLL_POINTS_PER_ROW;
        assert_eq!(first_row(offset(60.0), 7, 4), 0);
        assert_eq!(first_row(offset(99.0), 7, 4), 0);
        assert_eq!(first_row(offset(100.0), 7, 4), 1);
        assert_eq!(first_row(offset(160.0), 7, 4), 1);
    }
}

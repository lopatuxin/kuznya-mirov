//! «Рельеф» → «Высота объектов»: на какую поверхность встаёт объект трёхмерной сцены и какой столб
//! он занимает. Поверхность — рельеф или верх настила (`deck`); высота основания `z` есть у каждого
//! объекта с `position`, и всё, что её ставит, — сборка мира, сдвиг, создание, вызовы редактора —
//! считает её здесь.

use super::footprint::Footprint;
use super::property;
use super::scene::ground_footprint;
use super::value::Vec2;
use super::world::World;

/// Ступенька, которую объект переходит, и допуск на неё: перепад до 0,4 клетки включительно.
pub const STEP: f64 = 0.4;

const TOLERANCE: f64 = 1e-9;

/// Глубже этого прямоугольник объекта должен зайти на настил, чтобы встать на него: идущий, что лежит
/// телом вдоль края настила ровно впритык, не прыгает на настил и обратно от погрешности дробных чисел.
pub const SEAT_DEPTH: f64 = 1e-5;

/// Чем объект трёхмерной сцены является для поверхности под ним.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Настил: мост, помост, ступень.
    Deck,
    /// Объём: фигура и объект без `shape`, `image` и `color`.
    Volume,
    /// Плоский объект: `image` или `color` без `shape`.
    Flat,
}

pub fn kind(world: &World, id: u32) -> Kind {
    if world.flag(id, property::DECK) {
        Kind::Deck
    } else if world.has(id, property::SHAPE) {
        Kind::Volume
    } else if world.has(id, property::IMAGE) || world.has(id, property::COLOR) {
        Kind::Flat
    } else {
        Kind::Volume
    }
}

/// `height` объекта: у фигуры и объекта без `shape`, `image` и `color` по умолчанию одна клетка; тем же
/// ростом меряется просвет под настилом для идущего плоского объекта.
pub fn body_height(world: &World, id: u32) -> f64 {
    world.number_like(id, property::HEIGHT).unwrap_or(1.0)
}

/// Верх настила: `z + height`; у настила без фигуры и без `height` — сам `z`.
pub fn deck_top(world: &World, id: u32) -> f64 {
    let base = world.base_z(id);
    let height = if world.has(id, property::SHAPE) {
        Some(body_height(world, id))
    } else {
        world.number_like(id, property::HEIGHT)
    };
    base + height.unwrap_or(0.0)
}

/// Настилы мира: номер, место на земле и верх.
pub fn decks(world: &World) -> impl Iterator<Item = (u32, Footprint, f64)> + '_ {
    world
        .ids()
        .filter(|&id| world.flag(id, property::DECK))
        .filter_map(|id| Some((id, ground_footprint(world, id)?, deck_top(world, id))))
}

/// Есть ли в мире хоть один настил.
pub fn has_decks(world: &World) -> bool {
    decks(world).next().is_some()
}

/// Ходят ли по этому миру по поверхностям: есть рельеф с холмами, ямами или водой либо настил. Без
/// них земля ровная, и ходьба — прежняя, по плоскости.
pub fn walks_on_surfaces(world: &World) -> bool {
    world.three_d() && (!world.terrain().is_trivial() || has_decks(world))
}

/// Основание, на которое встал бы объект (не настил) с местом `footprint`: с `from` — сдвинувшись с
/// основания `from` на верх самого высокого настила под собой, чей верх не выше `from` плюс ступенька;
/// без `from` — на верх самого высокого настила под собой. Без такого настила — на самую нижнюю
/// точку рельефа под местом. `id` как настил под собой не считается.
pub fn rest_height(
    world: &World,
    id: Option<u32>,
    footprint: &Footprint,
    from: Option<f64>,
) -> f64 {
    let mut best: Option<f64> = None;
    for (deck_id, place, top) in decks(world) {
        if Some(deck_id) == id {
            continue;
        }
        if from.is_some_and(|from| top > from + STEP + TOLERANCE) {
            continue;
        }
        if place.overlaps_deeper_than(footprint, SEAT_DEPTH) {
            best = Some(best.map_or(top, |b| b.max(top)));
        }
    }
    best.unwrap_or_else(|| world.terrain().min_under(footprint))
}

/// Основание настила без `z` в данных: низом на самой высокой точке рельефа под ним.
pub fn deck_rest_height(world: &World, footprint: &Footprint) -> f64 {
    world.terrain().max_under(footprint)
}

/// Основание объекта `id` на новом месте `position` (с его нынешними `size` и `rotation`): с `from`
/// — как после сдвига с этого основания, настил — на нынешнем; без `from` — как объект без `z` в
/// данных. `None` в плоской сцене и без `position` и `size`.
pub fn rest_height_at(world: &World, id: u32, position: Vec2, from: Option<f64>) -> Option<f64> {
    if !world.three_d() || !world.is_alive(id) {
        return None;
    }
    let size = world.vec2(id, property::SIZE)?;
    world.vec2(id, property::POSITION)?;
    let footprint = Footprint::rotated(position, size, world.rotation(id, property::ROTATION));
    Some(match (kind(world, id), from) {
        (Kind::Deck, Some(_)) => world.base_z(id),
        (Kind::Deck, None) => deck_rest_height(world, &footprint),
        (_, from) => rest_height(world, Some(id), &footprint, from),
    })
}

/// Место, на которое сажается объект: его прямоугольник, а у объекта без `size` — точка `position`.
fn seating_place(world: &World, id: u32) -> Option<Footprint> {
    let position = world.vec2(id, property::POSITION)?;
    let size = world.vec2(id, property::SIZE).unwrap_or([0.0, 0.0]);
    Some(Footprint::rotated(
        position,
        size,
        world.rotation(id, property::ROTATION),
    ))
}

/// Сдвиг: объект, кроме настила, у которого сменились `x`, `y` или `rotation`, встаёт заново от
/// своего прежнего основания.
pub fn seat_after_shift(world: &mut World, id: u32) {
    if !world.three_d() || kind(world, id) == Kind::Deck {
        return;
    }
    let Some(place) = seating_place(world, id) else {
        return;
    };
    let z = rest_height(world, Some(id), &place, Some(world.base_z(id)));
    world.set_base_z(id, z);
}

/// Сборка мира и создание без `z`: объект встаёт как в данных без третьего числа.
pub fn seat_fresh(world: &mut World, id: u32) {
    if !world.three_d() {
        return;
    }
    let Some(place) = seating_place(world, id) else {
        return;
    };
    let z = if kind(world, id) == Kind::Deck {
        deck_rest_height(world, &place)
    } else {
        rest_height(world, Some(id), &place, None)
    };
    world.set_base_z(id, z);
}

/// `at_parent`: созданный объект встаёт так, будто сдвинулся с основания `from` родителя.
pub fn seat_from(world: &mut World, id: u32, from: f64) {
    if !world.three_d() || kind(world, id) == Kind::Deck {
        return;
    }
    let Some(place) = seating_place(world, id) else {
        return;
    };
    let z = rest_height(world, Some(id), &place, Some(from));
    world.set_base_z(id, z);
}

/// Поверхность, на которой лежит плоский объект.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lies {
    /// По рельефу: от самой низкой до самой высокой точки под местом.
    Terrain { low: f64, high: f64 },
    /// На верху настила.
    Deck { top: f64 },
}

/// Поверхность, по которой рисуется плоский объект и на которую он ловит щелчок: рельеф или верх
/// настила под его местом — та, чья высота ближе к его `z`. `None` без `position` и `size`.
pub fn lies_on(world: &World, id: u32) -> Option<Lies> {
    let place = ground_footprint(world, id)?;
    let z = world.base_z(id);
    let (low, high) = world.terrain().range_under(&place);
    let terrain_distance = (low - z).max(z - high).max(0.0);
    let deck = decks(world)
        .filter(|&(deck_id, ref deck_place, _)| deck_id != id && deck_place.overlaps(&place))
        .map(|(_, _, top)| top)
        .min_by(|a, b| (a - z).abs().total_cmp(&(b - z).abs()));
    Some(match deck {
        Some(top) if (top - z).abs() <= terrain_distance + TOLERANCE => Lies::Deck { top },
        _ => Lies::Terrain { low, high },
    })
}

/// Столб объекта по высоте: от `low` включительно до `high`; у объёма верх не входит, у плоского
/// объекта и настила без высоты — оба конца.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pillar {
    pub low: f64,
    pub high: f64,
    pub closed: bool,
}

impl Pillar {
    /// Столб плоской сцены: пересекается со всяким.
    pub const ANY: Pillar = Pillar {
        low: f64::NEG_INFINITY,
        high: f64::INFINITY,
        closed: false,
    };

    fn contains(&self, height: f64) -> bool {
        height >= self.low && (height < self.high || (self.closed && height == self.high))
    }

    /// Столбы, которые только касаются (`высота₁ + столб₁ = высота₂`), не пересекаются.
    pub fn overlaps(&self, other: &Pillar) -> bool {
        let low = self.low.max(other.low);
        let high = self.high.min(other.high);
        if low < high {
            return true;
        }
        low == high && self.contains(low) && other.contains(low)
    }
}

/// Столб объекта `id`; в плоской сцене — `Pillar::ANY`.
pub fn pillar(world: &World, id: u32) -> Pillar {
    if !world.three_d() {
        return Pillar::ANY;
    }
    let base = world.base_z(id);
    match kind(world, id) {
        Kind::Volume => Pillar {
            low: base,
            high: base + body_height(world, id),
            closed: false,
        },
        Kind::Deck => {
            let top = deck_top(world, id);
            Pillar {
                low: base,
                high: top,
                closed: top <= base,
            }
        }
        Kind::Flat => match lies_on(world, id) {
            Some(Lies::Terrain { low, high }) => Pillar {
                low,
                high,
                closed: true,
            },
            Some(Lies::Deck { top }) => Pillar {
                low: top,
                high: top,
                closed: true,
            },
            None => Pillar {
                low: base,
                high: base,
                closed: true,
            },
        },
    }
}

/// Высота верха объёма объекта над землёй, для камеры и рамок: `base + height` у объёма, `base` у
/// плоского.
pub fn volume_top(world: &World, id: u32) -> f64 {
    let base = world.base_z(id);
    match kind(world, id) {
        Kind::Volume => base + body_height(world, id),
        Kind::Deck => deck_top(world, id).max(base),
        Kind::Flat => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pillar_of(low: f64, high: f64, closed: bool) -> Pillar {
        Pillar { low, high, closed }
    }

    #[test]
    fn pillars_that_only_touch_do_not_meet_but_a_flat_object_on_a_top_meets_a_body_above() {
        let body = pillar_of(0.0, 1.0, false);
        let above = pillar_of(1.0, 2.0, false);
        assert!(!body.overlaps(&above));
        assert!(pillar_of(1.0, 1.0, true).overlaps(&above));
        assert!(!pillar_of(1.0, 1.0, true).overlaps(&body));
        assert!(pillar_of(0.5, 1.5, false).overlaps(&body));
    }

    #[test]
    fn a_flat_scene_pillar_meets_everything() {
        assert!(Pillar::ANY.overlaps(&pillar_of(5.0, 6.0, false)));
        assert!(Pillar::ANY.overlaps(&Pillar::ANY));
    }
}

//! «Огонь»: три свойства объекта — сила огня, цвет пламени и яркость ореола — с умолчаниями.

use super::particles::Rgb;
use super::property;
use super::world::World;

/// Оранжевый `#ff8c1a`.
const DEFAULT_COLOR: Rgb = [1.0, 140.0 / 255.0, 26.0 / 255.0];
const DEFAULT_GLOW: f64 = 0.5;

/// Огонь объекта, прочитанный из его свойств. Значения вне отрезка от 0 до 1 подтягиваются к нему:
/// рисование данные не отвергает.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub fire: f64,
    pub color: Rgb,
    pub glow: f64,
}

impl Settings {
    pub fn read(world: &World, id: u32) -> Settings {
        let number = |prop| world.number_like(id, prop).filter(|n| !n.is_nan());
        Settings {
            fire: number(property::FIRE).map_or(0.0, |n| n.clamp(0.0, 1.0)),
            color: world
                .color(id, property::FIRE_COLOR)
                .map_or(DEFAULT_COLOR, |[r, g, b, _]| [r, g, b]),
            glow: number(property::FIRE_GLOW).map_or(DEFAULT_GLOW, |n| n.clamp(0.0, 1.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::PropertyTable;

    fn world_with(set: impl FnOnce(&mut World)) -> World {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        assert_eq!(world.create(), 0);
        set(&mut world);
        world
    }

    #[test]
    fn an_object_without_the_properties_has_no_fire_an_orange_colour_and_half_a_glow() {
        let settings = Settings::read(&world_with(|_| {}), 0);
        assert_eq!(settings.fire, 0.0);
        assert_eq!(settings.color, DEFAULT_COLOR);
        assert_eq!(settings.glow, 0.5);
    }

    #[test]
    fn the_properties_are_read_as_written() {
        let world = world_with(|world| {
            world.set_number(0, property::FIRE, 0.6);
            world.set_color(0, property::FIRE_COLOR, [0.0, 0.0, 1.0, 1.0]);
            world.set_number(0, property::FIRE_GLOW, 0.25);
        });
        let settings = Settings::read(&world, 0);
        assert_eq!(settings.fire, 0.6);
        assert_eq!(settings.color, [0.0, 0.0, 1.0]);
        assert_eq!(settings.glow, 0.25);
    }

    #[test]
    fn values_outside_the_range_are_pulled_in_and_nonsense_gives_the_default() {
        let read = |fire: f64, glow: f64| {
            let world = world_with(|world| {
                world.set_number(0, property::FIRE, fire);
                world.set_number(0, property::FIRE_GLOW, glow);
            });
            Settings::read(&world, 0)
        };
        assert_eq!((read(7.0, -1.0).fire, read(7.0, -1.0).glow), (1.0, 0.0));
        assert_eq!(read(-3.0, 9.0).fire, 0.0);
        assert_eq!(read(-3.0, 9.0).glow, 1.0);
        assert_eq!(
            (read(f64::NAN, f64::NAN).fire, read(f64::NAN, f64::NAN).glow),
            (0.0, 0.5)
        );
    }
}

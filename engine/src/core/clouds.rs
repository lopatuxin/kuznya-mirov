//! «Ветер и частицы» → «Облака»: два свойства неба — сколько облаков и из каких картинок — и то, чем
//! картинка может не годиться облакам.

use super::property;
use super::value::ImageId;
use super::world::World;

/// Чем картинка не годится облакам (проверка перед запуском, требование 27).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudMisfit {
    Video,
    Frames,
    Placement,
    NoSize,
}

impl CloudMisfit {
    pub fn reason(self) -> &'static str {
        match self {
            CloudMisfit::Video => "видео",
            CloudMisfit::Frames => "картинка с кадрами",
            CloudMisfit::Placement => "картинка с anchor или offset",
            CloudMisfit::NoSize => "картинка без size в игре без cell_pixels",
        }
    }
}

/// Облака объекта, прочитанные из его свойств. `clouds` вне отрезка от 0 до 1 подтягивается к нему:
/// рисование данные не отвергает.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings<'a> {
    pub clouds: f64,
    pub images: &'a [ImageId],
}

impl<'a> Settings<'a> {
    pub fn read(world: &'a World, id: u32) -> Settings<'a> {
        Settings {
            clouds: world
                .number_like(id, property::CLOUDS)
                .filter(|n| !n.is_nan())
                .map_or(0.0, |n| n.clamp(0.0, 1.0)),
            images: world.image_list(id, property::CLOUD_IMAGES).unwrap_or(&[]),
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
    fn an_object_without_the_properties_has_no_clouds_and_no_pictures() {
        let bare = world_with(|_| {});
        let settings = Settings::read(&bare, 0);
        assert_eq!(settings.clouds, 0.0);
        assert!(settings.images.is_empty());
    }

    #[test]
    fn the_count_and_the_pictures_are_read_as_written_in_the_list_order() {
        let world = world_with(|world| {
            world.set_number(0, property::CLOUDS, 0.3);
            world.set_image_list(0, property::CLOUD_IMAGES, vec![1, 0]);
        });
        let settings = Settings::read(&world, 0);
        assert_eq!(settings.clouds, 0.3);
        assert_eq!(settings.images, &[1, 0]);
    }

    #[test]
    fn a_count_outside_the_range_is_pulled_in_and_nonsense_gives_none() {
        let read = |value: f64| {
            let world = world_with(|world| world.set_number(0, property::CLOUDS, value));
            Settings::read(&world, 0).clouds
        };
        assert_eq!(read(7.0), 1.0);
        assert_eq!(read(-1.0), 0.0);
        assert_eq!(read(f64::NAN), 0.0);
    }
}

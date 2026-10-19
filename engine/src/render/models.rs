//! «Трёхмерная сцена» → «Модели и движения», «Отрисовка»: где в кадре стоит каждый объект с `model`. Ни
//! видеокарты, ни браузера: `wasm::mod` переводит это в данные для `render::gpu_models`.

use crate::core::game::Game;
use crate::core::model::Image;
use crate::core::property;
use crate::core::value::Rotation;

use super::materials::{linear_to_srgb, srgb_to_linear};

/// Один объект модели в кадре: середина основания на земле с косинусом и синусом поворота и высота
/// основания с увеличением модели. Вершины модели уже лежат серединой основания в начале.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelInstance {
    /// `[x, y, cos, sin]`.
    pub placement: [f32; 4],
    /// `[z, scale, 0, 0]`.
    pub base: [f32; 4],
}

/// Объекты каждой модели кадра по номеру модели и самая высокая точка среди них — по ней охватывается
/// карта теней.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelDraws {
    pub instances: Vec<Vec<ModelInstance>>,
    pub top: f64,
}

/// Объекты мира с `model`, `position` и `size`, разложенные по моделям. Модель без частей не рисуется.
pub fn model_draws(game: &Game) -> ModelDraws {
    let world = &game.world;
    let models = world.models();
    let mut draws = ModelDraws {
        instances: vec![Vec::new(); models.len()],
        top: f64::NEG_INFINITY,
    };
    if models.is_empty() {
        return draws;
    }
    for id in world.ids() {
        let Some(name) = world.text(id, property::MODEL) else {
            continue;
        };
        let Some(index) = models.index_of(name) else {
            continue;
        };
        let (Some(model), Some(position), Some(size)) = (
            models.get(index),
            world.vec2(id, property::POSITION),
            world.vec2(id, property::SIZE),
        ) else {
            continue;
        };
        if model.parts.is_empty() {
            continue;
        }
        let scale = model.scale(world.number_like(id, property::HEIGHT));
        let (sin, cos) = world
            .rotation(id, property::ROTATION)
            .map_or((0.0, 1.0), Rotation::sin_cos);
        let base = world.base_z(id);
        draws.top = draws.top.max(base + model.size[2] * scale);
        draws.instances[index].push(ModelInstance {
            placement: [
                (position[0] + size[0] / 2.0) as f32,
                (position[1] + size[1] / 2.0) as f32,
                cos as f32,
                sin as f32,
            ],
            base: [base as f32, scale as f32, 0.0, 0.0],
        });
    }
    draws
}

/// Одна ступень уменьшения картинки модели: четыре байта на точку.
#[derive(Debug, Clone, PartialEq)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Ступени уменьшения картинки до одной точки, с первой, которая влезает в `max_side`. Цвет `srgb`
/// усредняется в линейной яркости, остальные карты — как есть.
pub fn mip_chain(image: &Image, srgb: bool, max_side: u32) -> Vec<MipLevel> {
    let mut level = MipLevel {
        width: image.width,
        height: image.height,
        pixels: image.pixels.clone(),
    };
    let mut chain = Vec::new();
    loop {
        let last = level.width <= 1 && level.height <= 1;
        let next = (!last).then(|| shrink(&level, srgb));
        if level.width <= max_side && level.height <= max_side {
            chain.push(level);
        }
        match next {
            Some(next) => level = next,
            None => return chain,
        }
    }
}

/// Вдвое меньше по каждой стороне (не меньше точки): каждая новая точка — среднее до четырёх старых.
fn shrink(level: &MipLevel, srgb: bool) -> MipLevel {
    let (width, height) = ((level.width / 2).max(1), (level.height / 2).max(1));
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    let at = |x: u32, y: u32| (y as usize * level.width as usize + x as usize) * 4;
    for y in 0..height {
        for x in 0..width {
            let sources = [
                (2 * x, 2 * y),
                ((2 * x + 1).min(level.width - 1), 2 * y),
                (2 * x, (2 * y + 1).min(level.height - 1)),
                (
                    (2 * x + 1).min(level.width - 1),
                    (2 * y + 1).min(level.height - 1),
                ),
            ];
            for channel in 0..4 {
                let sum: f32 = sources
                    .iter()
                    .map(|&(sx, sy)| {
                        let byte = level.pixels[at(sx, sy) + channel];
                        if srgb && channel < 3 {
                            srgb_to_linear(byte)
                        } else {
                            f32::from(byte) / 255.0
                        }
                    })
                    .sum();
                let mean = sum / 4.0;
                pixels.push(if srgb && channel < 3 {
                    linear_to_srgb(mean)
                } else {
                    (mean * 255.0).round() as u8
                });
            }
        }
    }
    MipLevel {
        width,
        height,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, pixel: [u8; 4]) -> Image {
        Image {
            width,
            height,
            pixels: pixel.repeat((width * height) as usize),
        }
    }

    #[test]
    fn the_chain_goes_down_to_one_point_and_halves_every_side() {
        let chain = mip_chain(&image(8, 4, [10, 20, 30, 255]), false, 64);
        let sides: Vec<(u32, u32)> = chain.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sides, [(8, 4), (4, 2), (2, 1), (1, 1)]);
        assert!(
            chain
                .iter()
                .all(|l| l.pixels == [10, 20, 30, 255].repeat((l.width * l.height) as usize))
        );
    }

    #[test]
    fn levels_that_do_not_fit_the_limit_are_dropped_from_the_top() {
        let chain = mip_chain(&image(16, 16, [1; 4]), false, 4);
        assert_eq!((chain[0].width, chain[0].height), (4, 4));
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn a_colour_is_averaged_in_linear_light_and_a_data_map_as_it_is() {
        let mut pixels = [0, 0, 0, 255].repeat(2);
        pixels.extend([255, 255, 255, 255].repeat(2));
        let source = Image {
            width: 2,
            height: 2,
            pixels,
        };
        let colour = &mip_chain(&source, true, 8)[1];
        // Среднее чёрного и белого в линейной яркости — 0,5, а это около 188 в sRGB.
        assert!(
            (i32::from(colour.pixels[0]) - 188).abs() <= 1,
            "{:?}",
            colour.pixels
        );
        let data = &mip_chain(&source, false, 8)[1];
        assert_eq!(data.pixels[0], 128);
    }

    #[test]
    fn odd_sides_do_not_read_past_the_edge() {
        let chain = mip_chain(&image(3, 5, [200; 4]), false, 64);
        assert_eq!(chain.last().map(|l| (l.width, l.height)), Some((1, 1)));
    }
}

//! «Трёхмерная сцена» → «Модели и движения», «Отрисовка»: где в кадре стоит каждый объект с `model`. Ни
//! видеокарты, ни браузера: `wasm::mod` переводит это в данные для `render::gpu_models`.

use crate::core::game::Game;
use crate::core::model::Image;
use crate::core::property;
use crate::core::value::Rotation;

use super::materials::{linear_to_srgb, srgb_to_linear};

/// Один объект модели в кадре: середина основания на земле с косинусом и синусом поворота, высота
/// основания, увеличение модели и растяжение вдоль её локальной оси +Y — у кусков дороги. Вершины модели уже
/// лежат серединой основания в начале.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelInstance {
    /// `[x, y, cos, sin]`.
    pub placement: [f32; 4],
    /// `[z, scale, stretch, 0]`; `stretch` — единица у всего, кроме кусков дороги.
    pub base: [f32; 4],
}

/// Объект с частью `BLEND`: номер модели и номер объекта в её списке кадра.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendedDraw {
    pub model: u32,
    pub index: u32,
}

/// Объекты каждой модели кадра по номеру модели, самая высокая точка среди них — по ней охватывается карта
/// теней, — и середины объектов, чьи модели имеют часть `BLEND`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelDraws {
    pub instances: Vec<Vec<ModelInstance>>,
    pub top: f64,
    blended: Vec<(BlendedDraw, [f64; 3])>,
}

impl ModelDraws {
    /// «Свет и материалы», прозрачность: объекты с частью `BLEND` от дальнего к ближнему по середине объекта,
    /// как их рисовать; равноудалённые — в порядке моделей и объектов.
    pub fn blended_far_to_near(&self, eye: [f64; 3]) -> Vec<BlendedDraw> {
        let reach = |middle: &[f64; 3]| {
            (0..3)
                .map(|axis| (middle[axis] - eye[axis]).powi(2))
                .sum::<f64>()
        };
        let mut order: Vec<&(BlendedDraw, [f64; 3])> = self.blended.iter().collect();
        order.sort_by(|a, b| reach(&b.1).total_cmp(&reach(&a.1)));
        order.into_iter().map(|(draw, _)| *draw).collect()
    }
}

/// Куски и узлы видов дорог кадра по номеру модели: сначала рисуются все куски, потом все узлы.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadDraws {
    pub pieces: Vec<Vec<ModelInstance>>,
    pub nodes: Vec<Vec<ModelInstance>>,
}

/// «Вид дороги»: куски и узлы раскладки `game` по моделям.
pub fn road_draws(game: &Game) -> RoadDraws {
    let count = game.world.models().len();
    let mut draws = RoadDraws {
        pieces: vec![Vec::new(); count],
        nodes: vec![Vec::new(); count],
    };
    let layout = game.road_kind_layout();
    let instance =
        |center: [f64; 2], z: f64, cos: f64, sin: f64, scale: f64, stretch: f64| ModelInstance {
            placement: [center[0] as f32, center[1] as f32, cos as f32, sin as f32],
            base: [z as f32, scale as f32, stretch as f32, 0.0],
        };
    for piece in layout.pieces() {
        draws.pieces[piece.model].push(instance(
            piece.center,
            piece.z,
            piece.cos,
            piece.sin,
            piece.scale,
            piece.stretch,
        ));
    }
    for node in layout.nodes() {
        draws.nodes[node.model].push(instance(
            node.center,
            node.z,
            node.cos,
            node.sin,
            node.scale,
            1.0,
        ));
    }
    draws
}

/// Объекты мира с `model`, `position` и `size`, разложенные по моделям. Модель без частей не рисуется.
pub fn model_draws(game: &Game) -> ModelDraws {
    let world = &game.world;
    let models = world.models();
    let mut draws = ModelDraws {
        instances: vec![Vec::new(); models.len()],
        top: f64::NEG_INFINITY,
        blended: Vec::new(),
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
        let middle = [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0];
        draws.top = draws.top.max(base + model.size[2] * scale);
        if model.has_blend() {
            draws.blended.push((
                BlendedDraw {
                    model: index as u32,
                    index: draws.instances[index].len() as u32,
                },
                [middle[0], middle[1], base + model.size[2] * scale / 2.0],
            ));
        }
        draws.instances[index].push(ModelInstance {
            placement: [middle[0] as f32, middle[1] as f32, cos as f32, sin as f32],
            base: [base as f32, scale as f32, 1.0, 0.0],
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

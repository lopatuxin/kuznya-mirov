//! «Трёхмерная сцена» → «Модели и движения»: модель `.glb`, разобранная в данные для рисования и для хода
//! игры. Вершины лежат в осях сцены (+x вправо, +y к игроку, +z вверх), середина основания модели — в
//! начале, низ — на нуле: так объекту остаётся сдвинуть, повернуть и масштабировать модель одним
//! числом на объект. Ни видеокарты, ни браузера: файл разбирает `data::glb`.

use std::sync::Arc;

use super::skeleton::Skeleton;
use super::solid::SolidGrid;

/// Вершина части модели.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Точка карты, как в glTF: начало слева сверху.
    pub uv: [f32; 2],
    /// Направление вдоль `u` карты; `w` — знак, с которым `cross(normal, tangent.xyz)` даёт направление
    /// вдоль «вверх» карты мелкого рельефа.
    pub tangent: [f32; 4],
    /// Цвет вершины `COLOR_0`; белый, если его нет.
    pub color: [f32; 4],
}

/// Как карта повторяется за своим краем.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    Mirror,
    Clamp,
}

/// `sampler` текстуры в файле: повтор по каждой оси и сглаживание вблизи (`mag_linear`) и вдали
/// (`min_linear`; вдали карта берётся ещё и по ступеням уменьшения).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sampler {
    pub wrap: [Wrap; 2],
    pub mag_linear: bool,
    pub min_linear: bool,
}

impl Default for Sampler {
    fn default() -> Self {
        Sampler {
            wrap: [Wrap::Repeat; 2],
            mag_linear: true,
            min_linear: true,
        }
    }
}

/// Карта материала: номер картинки в `Model::images` и как её брать.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Texture {
    pub image: usize,
    pub sampler: Sampler,
}

/// «Свет и материалы», «Прозрачность материала модели»: `alphaMode` материала по стандарту glTF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    /// Прозрачность не учитывается.
    Opaque,
    /// Точки с прозрачностью меньше порога не рисуются, остальные сплошные.
    Mask,
    /// Часть смешивается с тем, что за ней.
    Blend,
}

/// Порог `MASK`, если в файле нет `alphaCutoff`.
pub const DEFAULT_ALPHA_CUTOFF: f32 = 0.5;

/// Материал по стандарту glTF: цвет, мелкий рельеф, шероховатость и металл, затенение, прозрачность.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub base_color: [f32; 4],
    pub base_texture: Option<Texture>,
    pub metallic: f32,
    pub roughness: f32,
    /// Шероховатость в зелёном канале, металл — в синем.
    pub metal_rough_texture: Option<Texture>,
    /// Карта мелкого рельефа и её `scale`.
    pub normal_texture: Option<(Texture, f32)>,
    /// Карта затенения (красный канал) и её `strength`.
    pub occlusion_texture: Option<(Texture, f32)>,
    pub alpha_mode: AlphaMode,
    /// Порог `MASK`; у других режимов не читается.
    pub alpha_cutoff: f32,
}

impl Material {
    /// Часть без материала — белая матовая сплошная.
    pub fn plain() -> Material {
        Material {
            base_color: [1.0; 4],
            base_texture: None,
            metallic: 0.0,
            roughness: 1.0,
            metal_rough_texture: None,
            normal_texture: None,
            occlusion_texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: DEFAULT_ALPHA_CUTOFF,
        }
    }
}

/// Часть модели: все её треугольники с одним материалом.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub material: usize,
}

/// Какие суставы скелета двигают вершину и с каким весом: до четырёх, веса в сумме единица; нулевые веса —
/// вершина стоит на месте.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkinVertex {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

/// Картинка из файла модели, какой её разжала страница: четыре байта на точку.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Как лежат точки сжатой картинки: блоки BC по 16 байт на 4 × 4 точки или четыре байта на точку.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackedFormat {
    Bc7Srgb,
    Bc7,
    Bc5,
    Rgba8Srgb,
    Rgba8,
}

impl PackedFormat {
    pub fn is_block(self) -> bool {
        matches!(
            self,
            PackedFormat::Bc7Srgb | PackedFormat::Bc7 | PackedFormat::Bc5
        )
    }

    pub fn is_srgb(self) -> bool {
        matches!(self, PackedFormat::Bc7Srgb | PackedFormat::Rgba8Srgb)
    }

    /// Сколько байт занимает ступень `width` × `height`: блоки целиком у BC, у остальных — четыре байта на точку.
    pub fn level_bytes(self, width: u32, height: u32) -> usize {
        let (width, height) = (width as usize, height as usize);
        if self.is_block() {
            width.div_ceil(4) * height.div_ceil(4) * 16
        } else {
            width * height * 4
        }
    }
}

/// Картинка модели из файла KTX2 службы сжатия: все ступени уменьшения от полного размера до одной точки.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedImage {
    pub format: PackedFormat,
    pub width: u32,
    pub height: u32,
    /// Ступень `i` — `max(1, width >> i)` × `max(1, height >> i)`, как её читает видеокарта.
    pub levels: Vec<Vec<u8>>,
}

/// Картинка файла модели: точки, что разжала страница, или файл службы сжатия. Один сжатый файл — один `Arc` на
/// все модели игры.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelImage {
    Pixels(Image),
    Packed(Arc<PackedImage>),
}

/// «Свет и материалы», «Рисованный свет и картины»: как освещена модель. `Picture` показывает цвет своей картинки
/// как есть, без солнца и неба, не отбрасывает теней, но принимает чужие.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Look {
    #[default]
    Lit,
    Picture,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub parts: Vec<Part>,
    pub materials: Vec<Material>,
    pub images: Vec<ModelImage>,
    /// Коробка вокруг модели при масштабе 1: ширина по x, глубина по y и высота.
    pub size: [f64; 3],
    /// Где в осях модели лежит начало координат файла: загрузка ставит в начало середину коробки, а перекрёсток
    /// лежит в файле серединой в нуле.
    pub origin: [f32; 2],
    /// Скелет и движения файла; `None` у модели без скелета.
    pub skeleton: Option<Skeleton>,
    /// Веса вершин каждой части по порядку частей; пусто у модели без скелета.
    pub skin: Vec<Vec<SkinVertex>>,
    pub look: Look,
    pub building: Building,
}

/// Допуск на нижнюю границу коробки `inside`: стоящий на полу — в коробке, что начинается с пола.
const INSIDE_SLACK: f64 = 1e-6;

/// «Трёхмерная сцена» → «Здания»: коробка внутренности здания — узел файла с именем на `inside`. Коробка лежит в осях
/// своего узла (с обменом осей файла на оси сцены) и поворачивается вместе с ним: автор строит её прямой в осях узла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsideBox {
    /// Низ и верх коробки в осях узла.
    pub low: [f32; 3],
    pub high: [f32; 3],
    /// Точка осей модели → та же точка в осях узла; матрица по столбцам, как в glTF.
    pub to_node: [[f32; 4]; 4],
}

impl InsideBox {
    /// Лежит ли точка осей модели в коробке: низ включительно, верх нет.
    pub fn contains(&self, point: [f64; 3]) -> bool {
        (0..3).all(|axis| {
            let local = (0..3)
                .map(|k| f64::from(self.to_node[k][axis]) * point[k])
                .sum::<f64>()
                + f64::from(self.to_node[3][axis]);
            local >= f64::from(self.low[axis]) - INSIDE_SLACK && local < f64::from(self.high[axis])
        })
    }
}

/// «Трёхмерная сцена» → «Здания»: что делает модель зданием.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Building {
    /// Точка файла (в осях сцены), что легла в начало осей модели: середина коробки по плану и её низ. Модель среза
    /// встаёт на модель здания по этой точке, а не по своим коробкам.
    pub shift: [f32; 3],
    /// Сетка столбов твёрдой модели (`solid: true`); `None` у остальных.
    pub solid: Option<Arc<SolidGrid>>,
    /// Номер модели среза в таблице (`cutaway`).
    pub cutaway: Option<usize>,
    /// Коробки внутренности; их узлы не рисуются и не твёрдые.
    pub inside: Vec<InsideBox>,
}

impl Model {
    /// Модель, чьего файла нет под рукой: рисовать нечего, коробка — клетка.
    pub fn placeholder(name: &str) -> Model {
        Model {
            name: name.to_string(),
            parts: Vec::new(),
            materials: Vec::new(),
            images: Vec::new(),
            size: [1.0; 3],
            origin: [0.0; 2],
            skeleton: None,
            skin: Vec::new(),
            look: Look::Lit,
            building: Default::default(),
        }
    }

    /// Бросает ли модель тень: модель `picture` — нет, её тени нарисованы на картине.
    pub fn casts_shadow(&self) -> bool {
        self.look == Look::Lit
    }

    /// Есть ли у модели часть `BLEND`: её рисуют отдельно от сплошных частей, по порядку от дальнего к ближнему.
    pub fn has_blend(&self) -> bool {
        self.parts
            .iter()
            .any(|part| self.materials[part.material].alpha_mode == AlphaMode::Blend)
    }

    /// Во сколько раз модель увеличена объектом с `height`: без него одна единица файла — одна клетка.
    pub fn scale(&self, height: Option<f64>) -> f64 {
        match height {
            Some(height) if height > 0.0 && self.size[2] > 0.0 => height / self.size[2],
            _ => 1.0,
        }
    }

    /// Коробка вокруг модели при увеличении `scale`.
    pub fn dims(&self, scale: f64) -> [f64; 3] {
        self.size.map(|side| side * scale)
    }
}

/// Модели игры в порядке объявления в `files.models`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ModelTable {
    models: Vec<Model>,
    any_solid: bool,
    any_cutaway: bool,
}

impl ModelTable {
    pub fn new(models: Vec<Model>) -> ModelTable {
        ModelTable {
            any_solid: models.iter().any(|model| model.building.solid.is_some()),
            any_cutaway: models.iter().any(|model| model.building.cutaway.is_some()),
            models,
        }
    }

    /// Есть ли в игре твёрдая модель: без неё ход игры зданий не ищет.
    pub fn any_solid(&self) -> bool {
        self.any_solid
    }

    /// Есть ли в игре модель со срезом.
    pub fn any_cutaway(&self) -> bool {
        self.any_cutaway
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Model> {
        self.models.get(index)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.models.iter().position(|model| model.name == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Model> {
        self.models.iter()
    }

    /// Объявленные имена в порядке объявления.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.models.iter().map(|model| model.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(size: [f64; 3]) -> Model {
        Model {
            size,
            ..Model::placeholder("house")
        }
    }

    #[test]
    fn a_model_without_height_keeps_one_unit_per_cell_and_height_scales_it_whole() {
        let house = model([8.0, 6.0, 7.0]);
        assert_eq!(house.scale(None), 1.0);
        let scale = house.scale(Some(0.7));
        assert!((scale - 0.1).abs() < 1e-12);
        let dims = house.dims(scale);
        assert!((dims[0] - 0.8).abs() < 1e-12 && (dims[1] - 0.6).abs() < 1e-12);
        assert!((dims[2] - 0.7).abs() < 1e-12);
    }

    #[test]
    fn a_flat_or_unmeasured_model_ignores_height() {
        assert_eq!(model([1.0, 1.0, 0.0]).scale(Some(2.0)), 1.0);
        assert_eq!(model([1.0, 1.0, 3.0]).scale(Some(0.0)), 1.0);
    }

    #[test]
    fn the_table_finds_models_by_name_in_declaration_order() {
        let table = ModelTable::new(vec![Model::placeholder("a"), Model::placeholder("b")]);
        assert_eq!(table.index_of("b"), Some(1));
        assert_eq!(table.index_of("c"), None);
        assert_eq!(table.names().collect::<Vec<_>>(), ["a", "b"]);
    }
}

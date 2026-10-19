//! «Трёхмерная сцена» → «Модели и движения»: модель `.glb`, разобранная в данные для рисования и для хода
//! игры. Вершины лежат в осях сцены (+x вправо, +y к игроку, +z вверх), середина основания модели — в
//! начале, низ — на нуле: так объекту остаётся сдвинуть, повернуть и масштабировать модель одним
//! числом на объект. Ни видеокарты, ни браузера: файл разбирает `data::glb`.

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

/// Материал по стандарту glTF: цвет, мелкий рельеф, шероховатость и металл, затенение.
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
}

impl Material {
    /// Часть без материала — белая матовая.
    pub fn plain() -> Material {
        Material {
            base_color: [1.0; 4],
            base_texture: None,
            metallic: 0.0,
            roughness: 1.0,
            metal_rough_texture: None,
            normal_texture: None,
            occlusion_texture: None,
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

/// Картинка из файла модели, какой её разжала страница: четыре байта на точку.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub parts: Vec<Part>,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    /// Коробка вокруг модели при масштабе 1: ширина по x, глубина по y и высота.
    pub size: [f64; 3],
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
        }
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
}

impl ModelTable {
    pub fn new(models: Vec<Model>) -> ModelTable {
        ModelTable { models }
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

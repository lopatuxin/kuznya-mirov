use std::collections::HashMap;
use std::sync::Arc;

use super::clouds::CloudMisfit;
use super::imprints::StampTable;
use super::terrain::Terrain;
use super::value::{ImageId, PropKind};

pub type PropertyId = u16;

pub const POSITION: PropertyId = 0;
pub const SIZE: PropertyId = 1;
pub const VELOCITY: PropertyId = 2;
pub const GRID: PropertyId = 3;
pub const COLLIDES: PropertyId = 4;
pub const COLOR: PropertyId = 5;
pub const LAYER: PropertyId = 6;
pub const KEYS: PropertyId = 7;
pub const LIFETIME: PropertyId = 8;
pub const NAME: PropertyId = 9;
pub const IMAGE: PropertyId = 10;
pub const OPACITY: PropertyId = 11;
pub const ROTATION: PropertyId = 12;
pub const FOLLOW_MOUSE: PropertyId = 13;
/// «Камера и мышь в мире», требование 33: новые свойства движка добавлены в конец `BUILTINS` —
/// номера прежних свойств не сдвигаются.
pub const CAMERA_FOLLOWS: PropertyId = 14;
pub const WALK_TO: PropertyId = 15;
pub const WALK_SPEED: PropertyId = 16;
pub const ON_CLICK: PropertyId = 17;
pub const FLIP_X: PropertyId = 18;
pub const SHAPE: PropertyId = 19;
pub const HEIGHT: PropertyId = 20;
/// «Рельеф»: настил — мост, помост, ступень лестницы.
pub const DECK: PropertyId = 21;
/// «Мир на экране» → «Слои глубины»: доля сдвига камеры, на которую объект уходит по экрану.
pub const PARALLAX: PropertyId = 22;
/// «Слои глубины»: повтор рисунка объекта влево и вправо до краёв окна.
pub const REPEAT_X: PropertyId = 23;
/// «Ветер и частицы» → «Качание»: гибкость объекта в клетках — на сколько уходит вбок верх рисунка
/// при ровном ветре в одну клетку в секунду.
pub const SWAY: PropertyId = 24;
/// «Ветер и частицы» → «Частицы»: девять свойств дыма, искр и листопада.
pub const SMOKE: PropertyId = 25;
pub const SMOKE_HEIGHT: PropertyId = 26;
pub const SMOKE_COLOR: PropertyId = 27;
pub const SPARKS: PropertyId = 28;
pub const SPARKS_REACH: PropertyId = 29;
pub const SPARKS_DIRECTION: PropertyId = 30;
pub const SPARKS_SPREAD: PropertyId = 31;
pub const LEAF_FALL: PropertyId = 32;
pub const LEAF_COLOR: PropertyId = 33;
/// «Ветер и частицы» → «Облака»: сколько облаков на небе и из каких картинок они.
pub const CLOUDS: PropertyId = 34;
pub const CLOUD_IMAGES: PropertyId = 35;
/// «Огонь»: сила пламени, его цвет и яркость ореола.
pub const FIRE: PropertyId = 36;
pub const FIRE_COLOR: PropertyId = 37;
pub const FIRE_GLOW: PropertyId = 38;

/// Свойства частиц: только плоская сцена, только объекту с `position` и `size`, без `repeat_x`.
pub const PARTICLE_PROPERTIES: [PropertyId; 9] = [
    SMOKE,
    SMOKE_HEIGHT,
    SMOKE_COLOR,
    SPARKS,
    SPARKS_REACH,
    SPARKS_DIRECTION,
    SPARKS_SPREAD,
    LEAF_FALL,
    LEAF_COLOR,
];

/// Свойства облаков: только плоская сцена, только объекту с `position`, `size` и `repeat_x`.
pub const CLOUD_PROPERTIES: [PropertyId; 2] = [CLOUDS, CLOUD_IMAGES];

/// Свойства огня: только плоская сцена, только объекту с `position` и `size`, без `repeat_x`.
pub const FIRE_PROPERTIES: [PropertyId; 3] = [FIRE, FIRE_COLOR, FIRE_GLOW];

const BUILTINS: &[(&str, PropKind)] = &[
    ("position", PropKind::Vec2),
    ("size", PropKind::Vec2),
    ("velocity", PropKind::Vec2),
    ("grid", PropKind::Grid),
    ("collides", PropKind::Flag),
    ("color", PropKind::Color),
    ("layer", PropKind::Layer),
    ("keys", PropKind::Keys),
    ("lifetime", PropKind::Time),
    ("name", PropKind::Text),
    ("image", PropKind::Image),
    ("opacity", PropKind::Number),
    ("rotation", PropKind::Rotation),
    ("follow_mouse", PropKind::FollowMouse),
    ("camera_follows", PropKind::Flag),
    ("walk_to", PropKind::Vec2),
    ("walk_speed", PropKind::Number),
    ("on_click", PropKind::OnClick),
    ("flip_x", PropKind::Flag),
    ("shape", PropKind::Shape),
    ("height", PropKind::Number),
    ("deck", PropKind::Flag),
    ("parallax", PropKind::Number),
    ("repeat_x", PropKind::Flag),
    ("sway", PropKind::Number),
    ("smoke", PropKind::Number),
    ("smoke_height", PropKind::Number),
    ("smoke_color", PropKind::Color),
    ("sparks", PropKind::Number),
    ("sparks_reach", PropKind::Number),
    ("sparks_direction", PropKind::Number),
    ("sparks_spread", PropKind::Number),
    ("leaf_fall", PropKind::Number),
    ("leaf_color", PropKind::Color),
    ("clouds", PropKind::Number),
    ("cloud_images", PropKind::ImageList),
    ("fire", PropKind::Number),
    ("fire_color", PropKind::Color),
    ("fire_glow", PropKind::Number),
];

#[derive(Debug, Clone)]
pub struct PropertyDef {
    pub name: String,
    pub kind: PropKind,
    pub builtin: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PropertyTable {
    defs: Vec<PropertyDef>,
    by_name: HashMap<String, PropertyId>,
    /// «Трёхмерная сцена»: сцена игры трёхмерная (`scene.camera`). От этого зависит, что значит
    /// `rotation` и можно ли `shape`/`height`, — таблицу свойств видят все места, что читают и
    /// пишут значения (файлы, правка на ходу, код), поэтому знать это ей.
    three_d: bool,
    /// «Рельеф»: земля трёхмерной сцены — та же таблица, что знает про `three_d`, отдаёт её каждому
    /// миру, который строится по этим свойствам.
    terrain: Option<Arc<Terrain>>,
    /// «Лепка рельефа»: штампы `files.stamps` — по ним ставятся отпечатки файла рельефа и правки редактора.
    stamps: StampTable,
    /// «Ветер и частицы» → «Облака»: по номеру картинки — чем она не годится облакам; пусто, пока игра не
    /// загружена, и тогда годится любая. Таблицу свойств видят все места, что пишут `cloud_images`.
    cloud_misfits: Vec<Option<CloudMisfit>>,
}

impl PropertyTable {
    pub fn new() -> Self {
        let mut table = PropertyTable {
            defs: Vec::with_capacity(BUILTINS.len()),
            by_name: HashMap::with_capacity(BUILTINS.len()),
            three_d: false,
            terrain: None,
            stamps: StampTable::default(),
            cloud_misfits: Vec::new(),
        };
        for (name, kind) in BUILTINS {
            let id = table.defs.len() as PropertyId;
            table.defs.push(PropertyDef {
                name: (*name).to_string(),
                kind: *kind,
                builtin: true,
            });
            table.by_name.insert((*name).to_string(), id);
        }
        table
    }

    /// Registers an author-declared property. Returns `Err` with the already-taken id
    /// when the name collides with a builtin or an already-declared property.
    pub fn declare_author(&mut self, name: &str, kind: PropKind) -> Result<PropertyId, PropertyId> {
        if let Some(&existing) = self.by_name.get(name) {
            return Err(existing);
        }
        let id = self.defs.len() as PropertyId;
        self.defs.push(PropertyDef {
            name: name.to_string(),
            kind,
            builtin: false,
        });
        self.by_name.insert(name.to_string(), id);
        Ok(id)
    }

    pub fn set_three_d(&mut self, three_d: bool) {
        self.three_d = three_d;
    }

    pub fn three_d(&self) -> bool {
        self.three_d
    }

    pub fn set_terrain(&mut self, terrain: Terrain) {
        self.terrain = Some(Arc::new(terrain));
    }

    pub fn terrain(&self) -> Option<&Arc<Terrain>> {
        self.terrain.as_ref()
    }

    pub fn set_stamps(&mut self, stamps: StampTable) {
        self.stamps = stamps;
    }

    pub fn stamps(&self) -> &StampTable {
        &self.stamps
    }

    pub fn set_cloud_misfits(&mut self, misfits: Vec<Option<CloudMisfit>>) {
        self.cloud_misfits = misfits;
    }

    pub fn cloud_misfit(&self, image: ImageId) -> Option<CloudMisfit> {
        self.cloud_misfits.get(image).copied().flatten()
    }

    pub fn resolve(&self, name: &str) -> Option<PropertyId> {
        self.by_name.get(name).copied()
    }

    pub fn def(&self, id: PropertyId) -> &PropertyDef {
        &self.defs[id as usize]
    }

    pub fn name(&self, id: PropertyId) -> &str {
        &self.defs[id as usize].name
    }

    pub fn kind(&self, id: PropertyId) -> PropKind {
        self.defs[id as usize].kind
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (PropertyId, &PropertyDef)> {
        self.defs
            .iter()
            .enumerate()
            .map(|(i, def)| (i as PropertyId, def))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_get_fixed_ids() {
        let table = PropertyTable::new();
        assert_eq!(table.resolve("position"), Some(POSITION));
        assert_eq!(table.resolve("velocity"), Some(VELOCITY));
        assert_eq!(table.kind(COLLIDES), PropKind::Flag);
        assert_eq!(table.resolve("image"), Some(IMAGE));
        assert_eq!(table.kind(IMAGE), PropKind::Image);
        assert_eq!(table.resolve("opacity"), Some(OPACITY));
        assert_eq!(table.kind(OPACITY), PropKind::Number);
        assert_eq!(table.resolve("flip_x"), Some(FLIP_X));
        assert_eq!(table.kind(FLIP_X), PropKind::Flag);
        assert_eq!(table.resolve("shape"), Some(SHAPE));
        assert_eq!(table.kind(SHAPE), PropKind::Shape);
        assert_eq!(table.resolve("height"), Some(HEIGHT));
        assert_eq!(table.kind(HEIGHT), PropKind::Number);
        assert_eq!(table.resolve("deck"), Some(DECK));
        assert_eq!(table.kind(DECK), PropKind::Flag);
        assert_eq!(table.resolve("parallax"), Some(PARALLAX));
        assert_eq!(table.kind(PARALLAX), PropKind::Number);
        assert_eq!(table.resolve("repeat_x"), Some(REPEAT_X));
        assert_eq!(table.kind(REPEAT_X), PropKind::Flag);
        assert_eq!(table.resolve("sway"), Some(SWAY));
        assert_eq!(table.kind(SWAY), PropKind::Number);
        for (name, id, kind) in [
            ("smoke", SMOKE, PropKind::Number),
            ("smoke_height", SMOKE_HEIGHT, PropKind::Number),
            ("smoke_color", SMOKE_COLOR, PropKind::Color),
            ("sparks", SPARKS, PropKind::Number),
            ("sparks_reach", SPARKS_REACH, PropKind::Number),
            ("sparks_direction", SPARKS_DIRECTION, PropKind::Number),
            ("sparks_spread", SPARKS_SPREAD, PropKind::Number),
            ("leaf_fall", LEAF_FALL, PropKind::Number),
            ("leaf_color", LEAF_COLOR, PropKind::Color),
        ] {
            assert_eq!(table.resolve(name), Some(id), "{name}");
            assert_eq!(table.kind(id), kind, "{name}");
            assert!(PARTICLE_PROPERTIES.contains(&id), "{name}");
        }
        for (name, id, kind) in [
            ("clouds", CLOUDS, PropKind::Number),
            ("cloud_images", CLOUD_IMAGES, PropKind::ImageList),
        ] {
            assert_eq!(table.resolve(name), Some(id), "{name}");
            assert_eq!(table.kind(id), kind, "{name}");
            assert!(CLOUD_PROPERTIES.contains(&id), "{name}");
            assert!(!PARTICLE_PROPERTIES.contains(&id), "{name}");
        }
        for (name, id, kind) in [
            ("fire", FIRE, PropKind::Number),
            ("fire_color", FIRE_COLOR, PropKind::Color),
            ("fire_glow", FIRE_GLOW, PropKind::Number),
        ] {
            assert_eq!(table.resolve(name), Some(id), "{name}");
            assert_eq!(table.kind(id), kind, "{name}");
            assert!(FIRE_PROPERTIES.contains(&id), "{name}");
            assert!(!PARTICLE_PROPERTIES.contains(&id), "{name}");
            assert!(!CLOUD_PROPERTIES.contains(&id), "{name}");
        }
        assert_eq!(table.resolve("particles"), None);
    }

    #[test]
    fn a_table_is_flat_until_the_scene_says_otherwise() {
        let mut table = PropertyTable::new();
        assert!(!table.three_d());
        table.set_three_d(true);
        assert!(table.three_d());
    }

    #[test]
    fn author_property_cannot_shadow_image_or_opacity() {
        let mut table = PropertyTable::new();
        assert_eq!(table.declare_author("image", PropKind::Flag), Err(IMAGE));
        assert_eq!(
            table.declare_author("opacity", PropKind::Flag),
            Err(OPACITY)
        );
    }

    #[test]
    fn author_property_cannot_shadow_builtin() {
        let mut table = PropertyTable::new();
        let result = table.declare_author("position", PropKind::Number);
        assert_eq!(result, Err(POSITION));
    }

    #[test]
    fn author_property_gets_new_id() {
        let mut table = PropertyTable::new();
        let id = table.declare_author("score", PropKind::Number).unwrap();
        assert_eq!(table.resolve("score"), Some(id));
        assert_eq!(table.kind(id), PropKind::Number);
    }
}

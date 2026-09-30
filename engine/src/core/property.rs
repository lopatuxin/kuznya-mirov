use std::collections::HashMap;
use std::sync::Arc;

use super::terrain::Terrain;
use super::value::PropKind;

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
}

impl PropertyTable {
    pub fn new() -> Self {
        let mut table = PropertyTable {
            defs: Vec::with_capacity(BUILTINS.len()),
            by_name: HashMap::with_capacity(BUILTINS.len()),
            three_d: false,
            terrain: None,
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

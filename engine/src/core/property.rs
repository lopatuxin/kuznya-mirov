use std::collections::HashMap;

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
}

impl PropertyTable {
    pub fn new() -> Self {
        let mut table = PropertyTable {
            defs: Vec::with_capacity(BUILTINS.len()),
            by_name: HashMap::with_capacity(BUILTINS.len()),
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

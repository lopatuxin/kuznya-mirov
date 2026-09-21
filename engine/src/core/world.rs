use super::keys::KeyTable;
use super::property::{PropertyId, PropertyTable};
use super::value::{GridSpec, ImageId, PropKind, Value, Vec2};

#[derive(Debug, Clone)]
enum Column {
    Flag(Vec<bool>),
    Number(Vec<Option<f64>>),
    Time(Vec<Option<i64>>),
    Vec2(Vec<Option<Vec2>>),
    Color(Vec<Option<[f32; 4]>>),
    Layer(Vec<Option<i32>>),
    Text(Vec<Option<String>>),
    Grid(Vec<Option<GridSpec>>),
    Keys(Vec<Option<KeyTable>>),
    Image(Vec<Option<ImageId>>),
}

impl Column {
    fn new(kind: PropKind) -> Self {
        match kind {
            PropKind::Flag => Column::Flag(Vec::new()),
            PropKind::Number => Column::Number(Vec::new()),
            PropKind::Time => Column::Time(Vec::new()),
            PropKind::Vec2 => Column::Vec2(Vec::new()),
            PropKind::Color => Column::Color(Vec::new()),
            PropKind::Layer => Column::Layer(Vec::new()),
            PropKind::Text => Column::Text(Vec::new()),
            PropKind::Grid => Column::Grid(Vec::new()),
            PropKind::Keys => Column::Keys(Vec::new()),
            PropKind::Image => Column::Image(Vec::new()),
        }
    }

    fn push_empty(&mut self) {
        match self {
            Column::Flag(v) => v.push(false),
            Column::Number(v) => v.push(None),
            Column::Time(v) => v.push(None),
            Column::Vec2(v) => v.push(None),
            Column::Color(v) => v.push(None),
            Column::Layer(v) => v.push(None),
            Column::Text(v) => v.push(None),
            Column::Grid(v) => v.push(None),
            Column::Keys(v) => v.push(None),
            Column::Image(v) => v.push(None),
        }
    }

    fn clear(&mut self, id: usize) {
        match self {
            Column::Flag(v) => v[id] = false,
            Column::Number(v) => v[id] = None,
            Column::Time(v) => v[id] = None,
            Column::Vec2(v) => v[id] = None,
            Column::Color(v) => v[id] = None,
            Column::Layer(v) => v[id] = None,
            Column::Text(v) => v[id] = None,
            Column::Grid(v) => v[id] = None,
            Column::Keys(v) => v[id] = None,
            Column::Image(v) => v[id] = None,
        }
    }

    fn has(&self, id: usize) -> bool {
        match self {
            Column::Flag(v) => v[id],
            Column::Number(v) => v[id].is_some(),
            Column::Time(v) => v[id].is_some(),
            Column::Vec2(v) => v[id].is_some(),
            Column::Color(v) => v[id].is_some(),
            Column::Layer(v) => v[id].is_some(),
            Column::Text(v) => v[id].is_some(),
            Column::Grid(v) => v[id].is_some(),
            Column::Keys(v) => v[id].is_some(),
            Column::Image(v) => v[id].is_some(),
        }
    }
}

#[derive(Debug)]
pub struct World {
    alive: Vec<bool>,
    free_list: Vec<u32>,
    columns: Vec<Column>,
    grid_counter: Vec<i64>,
    /// «Код игры»: bumped on every `create()` (fresh slot or reused one) so a code handle taken
    /// while an object was alive can tell it apart from a later, unrelated object that reused the
    /// same freed slot — see `generation`.
    generation: Vec<u32>,
}

impl World {
    pub fn new(properties: &PropertyTable) -> Self {
        let columns = properties
            .iter()
            .map(|(_, def)| Column::new(def.kind))
            .collect();
        World {
            alive: Vec::new(),
            free_list: Vec::new(),
            columns,
            grid_counter: Vec::new(),
            generation: Vec::new(),
        }
    }

    pub fn create(&mut self) -> u32 {
        if let Some(id) = self.free_list.pop() {
            self.alive[id as usize] = true;
            self.generation[id as usize] += 1;
            return id;
        }
        let id = self.alive.len() as u32;
        self.alive.push(true);
        self.grid_counter.push(0);
        self.generation.push(0);
        for column in &mut self.columns {
            column.push_empty();
        }
        id
    }

    /// «Код игры»: the object currently occupying slot `id` was created by this many `create()`
    /// calls into that slot — a code handle is stale (points at a deleted object even though its
    /// slot may already hold a new one) when this no longer matches the generation it was taken at.
    pub fn generation(&self, id: u32) -> u32 {
        self.generation[id as usize]
    }

    /// «Код игры»: clears one property on a live object without deleting the object itself — the
    /// runtime write side for `obj.prop = nil`. A no-op past the slot count, same as every other
    /// per-property accessor here.
    pub fn clear_property(&mut self, id: u32, prop: PropertyId) {
        self.columns[prop as usize].clear(id as usize);
    }

    pub fn delete(&mut self, id: u32) {
        let idx = id as usize;
        if !self.alive[idx] {
            return;
        }
        self.alive[idx] = false;
        for column in &mut self.columns {
            column.clear(idx);
        }
        self.free_list.push(id);
    }

    pub fn is_alive(&self, id: u32) -> bool {
        self.alive.get(id as usize).copied().unwrap_or(false)
    }

    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }

    pub fn slot_count(&self) -> usize {
        self.alive.len()
    }

    /// Ascending object ids currently alive.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.alive
            .iter()
            .enumerate()
            .filter(|&(_, &a)| a)
            .map(|(i, _)| i as u32)
    }

    pub fn has(&self, id: u32, prop: PropertyId) -> bool {
        self.alive[id as usize] && self.columns[prop as usize].has(id as usize)
    }

    pub fn has_all(&self, id: u32, props: &[PropertyId]) -> bool {
        props.iter().all(|&p| self.has(id, p))
    }

    pub fn has_none(&self, id: u32, props: &[PropertyId]) -> bool {
        props.iter().all(|&p| !self.has(id, p))
    }

    pub fn vec2(&self, id: u32, prop: PropertyId) -> Option<Vec2> {
        match &self.columns[prop as usize] {
            Column::Vec2(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_vec2(&mut self, id: u32, prop: PropertyId, value: Vec2) {
        if let Column::Vec2(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn number_like(&self, id: u32, prop: PropertyId) -> Option<f64> {
        match &self.columns[prop as usize] {
            Column::Number(v) => v[id as usize],
            Column::Time(v) => v[id as usize].map(|t| t as f64),
            _ => None,
        }
    }

    pub fn time(&self, id: u32, prop: PropertyId) -> Option<i64> {
        match &self.columns[prop as usize] {
            Column::Time(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_number(&mut self, id: u32, prop: PropertyId, value: f64) {
        if let Column::Number(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn set_time(&mut self, id: u32, prop: PropertyId, value: i64) {
        if let Column::Time(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn add_number_like(&mut self, id: u32, prop: PropertyId, delta: f64) {
        match &mut self.columns[prop as usize] {
            Column::Number(v) => {
                if let Some(cur) = v[id as usize].as_mut() {
                    *cur += delta;
                }
            }
            Column::Time(v) => {
                if let Some(cur) = v[id as usize].as_mut() {
                    *cur += delta.round() as i64;
                }
            }
            _ => {}
        }
    }

    pub fn flag(&self, id: u32, prop: PropertyId) -> bool {
        matches!(&self.columns[prop as usize], Column::Flag(v) if v[id as usize])
    }

    pub fn set_flag(&mut self, id: u32, prop: PropertyId, value: bool) {
        if let Column::Flag(v) = &mut self.columns[prop as usize] {
            v[id as usize] = value;
        }
    }

    pub fn color(&self, id: u32, prop: PropertyId) -> Option<[f32; 4]> {
        match &self.columns[prop as usize] {
            Column::Color(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_color(&mut self, id: u32, prop: PropertyId, value: [f32; 4]) {
        if let Column::Color(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn layer(&self, id: u32, prop: PropertyId) -> Option<i32> {
        match &self.columns[prop as usize] {
            Column::Layer(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_layer(&mut self, id: u32, prop: PropertyId, value: i32) {
        if let Column::Layer(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn text(&self, id: u32, prop: PropertyId) -> Option<&str> {
        match &self.columns[prop as usize] {
            Column::Text(v) => v[id as usize].as_deref(),
            _ => None,
        }
    }

    pub fn set_text(&mut self, id: u32, prop: PropertyId, value: String) {
        if let Column::Text(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn grid(&self, id: u32, prop: PropertyId) -> Option<GridSpec> {
        match &self.columns[prop as usize] {
            Column::Grid(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_grid(&mut self, id: u32, prop: PropertyId, value: GridSpec) {
        if let Column::Grid(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn keys(&self, id: u32, prop: PropertyId) -> Option<&KeyTable> {
        match &self.columns[prop as usize] {
            Column::Keys(v) => v[id as usize].as_ref(),
            _ => None,
        }
    }

    pub fn set_keys(&mut self, id: u32, prop: PropertyId, value: KeyTable) {
        if let Column::Keys(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn image(&self, id: u32, prop: PropertyId) -> Option<ImageId> {
        match &self.columns[prop as usize] {
            Column::Image(v) => v[id as usize],
            _ => None,
        }
    }

    pub fn set_image(&mut self, id: u32, prop: PropertyId, value: ImageId) {
        if let Column::Image(v) = &mut self.columns[prop as usize] {
            v[id as usize] = Some(value);
        }
    }

    pub fn grid_counter(&self, id: u32) -> i64 {
        self.grid_counter[id as usize]
    }

    pub fn set_grid_counter(&mut self, id: u32, value: i64) {
        self.grid_counter[id as usize] = value;
    }

    /// Sets a property to a generically-typed constant. `value`'s variant must match the
    /// column's kind; validation at load time guarantees this.
    pub fn set_value(&mut self, id: u32, prop: PropertyId, value: &Value) {
        match value {
            Value::Flag(b) => self.set_flag(id, prop, *b),
            Value::Number(n) => self.set_number(id, prop, *n),
            Value::Time(t) => self.set_time(id, prop, *t),
            Value::Vec2(v) => self.set_vec2(id, prop, *v),
            Value::Color(c) => self.set_color(id, prop, *c),
            Value::Layer(l) => self.set_layer(id, prop, *l),
            Value::Text(s) => self.set_text(id, prop, s.clone()),
            Value::Image(i) => self.set_image(id, prop, *i),
        }
    }

    pub fn get_value(&self, id: u32, prop: PropertyId, kind: PropKind) -> Option<Value> {
        match kind {
            PropKind::Flag => Some(Value::Flag(self.flag(id, prop))),
            PropKind::Number => self.number_like(id, prop).map(Value::Number),
            PropKind::Time => self.time(id, prop).map(Value::Time),
            PropKind::Vec2 => self.vec2(id, prop).map(Value::Vec2),
            PropKind::Color => self.color(id, prop).map(Value::Color),
            PropKind::Layer => self.layer(id, prop).map(Value::Layer),
            PropKind::Text => self.text(id, prop).map(|s| Value::Text(s.to_string())),
            PropKind::Image => self.image(id, prop).map(Value::Image),
            PropKind::Grid | PropKind::Keys => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::{self, PropertyTable};

    #[test]
    fn create_then_delete_frees_slot_for_reuse() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let a = world.create();
        world.set_vec2(a, property::POSITION, [1.0, 2.0]);
        world.delete(a);
        assert!(!world.is_alive(a));
        let b = world.create();
        assert_eq!(a, b, "freed slot is reused");
        assert_eq!(
            world.vec2(b, property::POSITION),
            None,
            "stale value is gone"
        );
    }

    #[test]
    fn presence_is_independent_per_property() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let a = world.create();
        world.set_vec2(a, property::POSITION, [0.0, 0.0]);
        assert!(world.has(a, property::POSITION));
        assert!(!world.has(a, property::VELOCITY));
    }
}

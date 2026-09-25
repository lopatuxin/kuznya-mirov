//! «Редактор» → «Правка на ходу», требования 41–43: live edits over an already-assembled `World`,
//! validated by the exact same functions `data::load` uses for `scene.json` itself, and the JSON
//! glue that turns a `Value`/`GridSpec` back into the form a file would show it in — the shape
//! `object_properties` reports and `add_object`/`set_property` accept.

use serde_json::Value as Json;

use crate::core::property::{self, PropertyId, PropertyTable};
use crate::core::value::{GridSpec, PropKind, Value};
use crate::core::world::World;

use super::load::{ImageDecl, parse_grid, parse_scalar_value};
use crate::data::error::ErrorSink;

/// «Редактор», требование 14: «не больше трёх знаков после запятой и без хвоста машинного
/// округления».
pub fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn json_number(n: f64) -> Json {
    serde_json::Number::from_f64(round3(n))
        .map(Json::Number)
        .unwrap_or(Json::Null)
}

fn format_hex_color(c: [f32; 4]) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))
}

/// A `Value` in the same units and shape `scene.json` itself writes it in — «Редактор», требование
/// 14. `images` resolves an `Image` value back to its declared name.
pub fn value_to_json(value: &Value, images: &[ImageDecl]) -> Json {
    match value {
        Value::Flag(b) => Json::Bool(*b),
        Value::Number(n) => json_number(*n),
        Value::Time(steps) => json_number(*steps as f64 / 60.0),
        Value::Timer(steps) => json_number(*steps as f64 / 60.0),
        Value::Vec2(v) => Json::Array(vec![json_number(v[0]), json_number(v[1])]),
        Value::Color(c) => Json::String(format_hex_color(*c)),
        Value::Layer(l) => Json::Number((*l).into()),
        Value::Text(s) => Json::String(s.clone()),
        Value::Image(id) => images
            .get(*id)
            .map(|decl| Json::String(decl.name.clone()))
            .unwrap_or(Json::Null),
        Value::Rotation(r) => Json::Number(r.degrees().into()),
        Value::FollowMouse(a) => Json::String(a.as_str().to_string()),
    }
}

/// `grid`'s own JSON shape — «Редактор», требование 14: `interval` in seconds, not steps.
pub fn grid_to_json(spec: GridSpec) -> Json {
    let mut map = serde_json::Map::with_capacity(1);
    map.insert(
        "interval".to_string(),
        json_number(spec.interval_steps as f64 / 60.0),
    );
    Json::Object(map)
}

/// Parses one property's value against `prop`'s declared kind, exactly as `scene.json` would —
/// «Редактор», требование 43: same checks, same seconds-to-steps conversion, a Russian message on
/// failure instead of a collected `GameError`.
pub fn parse_edit_value(
    json: &Json,
    prop: PropertyId,
    properties: &PropertyTable,
    images: &[ImageDecl],
) -> Result<Value, String> {
    let mut errors = ErrorSink::new();
    match parse_scalar_value(json, prop, properties, images, "", "", &mut errors) {
        Some(value) => Ok(value),
        None => {
            let (errs, _) = errors.into_parts();
            Err(errs
                .into_iter()
                .next()
                .map(|e| e.message)
                .unwrap_or_else(|| "значение не подходит".to_string()))
        }
    }
}

/// `grid`'s own value, parsed the same way `scene.json`'s `grid` field is.
pub fn parse_edit_grid(json: &Json) -> Result<GridSpec, String> {
    let mut errors = ErrorSink::new();
    match parse_grid(json, "", "", &mut errors) {
        Some(spec) => Ok(spec),
        None => {
            let (errs, _) = errors.into_parts();
            Err(errs
                .into_iter()
                .next()
                .map(|e| e.message)
                .unwrap_or_else(|| "grid не подходит".to_string()))
        }
    }
}

/// «Редактор», требование 42: object `id`'s properties in file form — `None` when it isn't alive.
/// `keys` carries no simple value form (same as `scene.json`'s own `values`, which never lists it
/// either) and is left out.
pub fn object_properties_json(
    world: &World,
    properties: &PropertyTable,
    images: &[ImageDecl],
    id: u32,
) -> Option<serde_json::Map<String, Json>> {
    if !world.is_alive(id) {
        return None;
    }
    let mut map = serde_json::Map::new();
    for (prop, def) in properties.iter() {
        if !world.has(id, prop) {
            continue;
        }
        let json = match def.kind {
            PropKind::Grid => world.grid(id, prop).map(grid_to_json),
            PropKind::Keys => None,
            kind => world
                .get_value(id, prop, kind)
                .map(|v| value_to_json(&v, images)),
        };
        if let Some(json) = json {
            map.insert(def.name.clone(), json);
        }
    }
    Some(map)
}

/// «Редактор», требование 43: sets or adds `prop_name` on a live object, validated like a
/// `scene.json` value — an unresolvable name is the "+ свойство" case, требование 19: the property
/// is not declared anywhere, so it cannot be declared mid-partiya either.
pub fn set_property(
    world: &mut World,
    properties: &PropertyTable,
    images: &[ImageDecl],
    id: u32,
    prop_name: &str,
    value_json: &Json,
) -> Result<(), String> {
    if !world.is_alive(id) {
        return Err("объекта уже нет в мире".to_string());
    }
    let prop = properties
        .resolve(prop_name)
        .ok_or_else(|| "Новое свойство объявляется вне партии".to_string())?;
    if prop_name == "keys" {
        return Err("keys не редактируется на ходу".to_string());
    }
    if properties.kind(prop) == PropKind::Grid {
        let had_grid = world.has(id, prop);
        let spec = parse_edit_grid(value_json)?;
        world.set_grid(id, prop, spec);
        if !had_grid {
            world.set_grid_counter(id, spec.interval_steps);
        }
        return Ok(());
    }
    let value = parse_edit_value(value_json, prop, properties, images)?;
    world.set_value(id, prop, &value);
    Ok(())
}

/// «Редактор», требование 43: removes `prop_name` from a live object — a no-op past the object's
/// own life or an unknown name is reported the same way `set_property` reports one.
pub fn remove_property(
    world: &mut World,
    properties: &PropertyTable,
    id: u32,
    prop_name: &str,
) -> Result<(), String> {
    if !world.is_alive(id) {
        return Err("объекта уже нет в мире".to_string());
    }
    let prop = properties
        .resolve(prop_name)
        .ok_or_else(|| format!("неизвестное свойство \"{prop_name}\""))?;
    world.clear_property(id, prop);
    Ok(())
}

/// «Редактор», требования 18, 43: a new object under the first free number (`World::create`, same
/// as a rule-spawned one), its properties parsed from `props_json` the same way `scene.json`'s own
/// `values` object is. `max_objects` reached — требование 18's exact wording, mir unchanged.
pub fn add_object(
    world: &mut World,
    properties: &PropertyTable,
    images: &[ImageDecl],
    max_objects: usize,
    props_json: &Json,
) -> Result<u32, String> {
    if world.alive_count() >= max_objects {
        return Err("Достигнут предел объектов".to_string());
    }
    let obj = props_json
        .as_object()
        .ok_or_else(|| "свойства объекта должны быть объектом".to_string())?;
    let mut values: Vec<(PropertyId, Value)> = Vec::with_capacity(obj.len());
    let mut grid: Option<GridSpec> = None;
    for (name, value_json) in obj {
        let prop = properties
            .resolve(name)
            .ok_or_else(|| "Новое свойство объявляется вне партии".to_string())?;
        if name == "keys" {
            return Err("keys не редактируется на ходу".to_string());
        }
        if properties.kind(prop) == PropKind::Grid {
            grid = Some(parse_edit_grid(value_json)?);
            continue;
        }
        values.push((
            prop,
            parse_edit_value(value_json, prop, properties, images)?,
        ));
    }
    let id = world.create();
    for (prop, value) in &values {
        world.set_value(id, *prop, value);
    }
    if let Some(spec) = grid {
        world.set_grid(id, property::GRID, spec);
        world.set_grid_counter(id, spec.interval_steps);
    }
    Ok(id)
}

/// «Редактор», требование 43: removes a live object outright, freeing its number.
pub fn delete_object(world: &mut World, id: u32) {
    world.delete(id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::{self as prop, PropertyTable};
    use crate::core::value::Rotation;
    use crate::core::world::World;

    fn table() -> PropertyTable {
        let mut t = PropertyTable::new();
        t.declare_author("score", PropKind::Number).unwrap();
        t
    }

    #[test]
    fn set_property_parses_seconds_into_steps_like_the_loader() {
        let properties = table();
        let mut world = World::new(&properties);
        let id = world.create();
        set_property(
            &mut world,
            &properties,
            &[],
            id,
            "lifetime",
            &serde_json::json!(1.5),
        )
        .unwrap();
        assert_eq!(world.time(id, prop::LIFETIME), Some(90));
    }

    #[test]
    fn set_property_of_the_wrong_shape_errors_and_leaves_the_world_unchanged() {
        let properties = table();
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_number(id, properties.resolve("score").unwrap(), 3.0);
        let err = set_property(
            &mut world,
            &properties,
            &[],
            id,
            "score",
            &serde_json::json!("not a number"),
        )
        .unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(
            world.number_like(id, properties.resolve("score").unwrap()),
            Some(3.0)
        );
    }

    #[test]
    fn set_property_of_an_undeclared_name_reports_the_editor_specific_message() {
        let properties = table();
        let mut world = World::new(&properties);
        let id = world.create();
        let err = set_property(
            &mut world,
            &properties,
            &[],
            id,
            "not_declared",
            &serde_json::json!(1),
        )
        .unwrap_err();
        assert_eq!(err, "Новое свойство объявляется вне партии");
    }

    #[test]
    fn add_object_takes_the_first_free_number_and_max_objects_is_reported_in_russian() {
        let properties = table();
        let mut world = World::new(&properties);
        let first = add_object(
            &mut world,
            &properties,
            &[],
            2,
            &serde_json::json!({"position": [1.0, 2.0]}),
        )
        .unwrap();
        world.delete(first);
        let second = add_object(
            &mut world,
            &properties,
            &[],
            2,
            &serde_json::json!({"position": [3.0, 4.0]}),
        )
        .unwrap();
        assert_eq!(
            first, second,
            "freed number is reused, same as a rule spawn"
        );

        let _third = add_object(&mut world, &properties, &[], 2, &serde_json::json!({})).unwrap();
        let err = add_object(&mut world, &properties, &[], 2, &serde_json::json!({})).unwrap_err();
        assert_eq!(err, "Достигнут предел объектов");
    }

    #[test]
    fn object_properties_json_round_trips_through_add_object() {
        let properties = table();
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_vec2(id, prop::POSITION, [1.0, 2.0]);
        world.set_color(id, prop::COLOR, [1.0, 0.0, 0.0, 1.0]);
        world.set_rotation(
            id,
            prop::ROTATION,
            Rotation::from_degrees_exact(90.0).unwrap(),
        );
        let json = object_properties_json(&world, &properties, &[], id).unwrap();
        assert_eq!(json["position"], serde_json::json!([1.0, 2.0]));
        assert_eq!(json["color"], serde_json::json!("#ff0000"));
        assert_eq!(json["rotation"], serde_json::json!(90));

        let copy_id = add_object(&mut world, &properties, &[], 100, &Json::Object(json)).unwrap();
        assert_eq!(world.vec2(copy_id, prop::POSITION), Some([1.0, 2.0]));
        assert_eq!(
            world.color(copy_id, prop::COLOR),
            Some([1.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn remove_property_clears_it_and_delete_object_frees_the_number() {
        let properties = table();
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_number(id, properties.resolve("score").unwrap(), 5.0);
        remove_property(&mut world, &properties, id, "score").unwrap();
        assert!(!world.has(id, properties.resolve("score").unwrap()));

        delete_object(&mut world, id);
        assert!(!world.is_alive(id));
    }
}

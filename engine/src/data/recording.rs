//! «Редактор» → «Запись и повтор», «Технические детали»: the replay file format — events by step
//! number, one per line so the file reads by eye. Property/screen names travel as plain strings,
//! resolved against whichever files are loaded at replay time (не при разборе), so a replay against
//! edited files can skip what no longer applies — «Исполнение игры» → «Запись партии в редакторе»,
//! требование 32.

use serde_json::Value as Json;

pub const FORMAT: u64 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayCommand {
    ShowScreen(String),
    /// The target screen's name, plus raw `"<объект>.<свойство>"` → JSON value pairs — resolved at
    /// replay time the same way `data::load::parse_initial_values` resolves a `new_game` button's
    /// own third element.
    NewGame(String, Vec<(String, Json)>),
    Resume,
    Quit,
    ToggleSound,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayEdit {
    Set(u32, String, Json),
    Remove(u32, String),
    Add(Json),
    Delete(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayEventKind {
    KeyDown(String),
    KeyUp(String),
    Cursor([f64; 2]),
    Command(ReplayCommand),
    Edit(ReplayEdit),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayEvent {
    pub step: u64,
    pub kind: ReplayEventKind,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    pub steps: u64,
    pub events: Vec<ReplayEvent>,
}

fn want_u64(json: &Json, what: &str) -> Result<u64, String> {
    json.as_u64()
        .ok_or_else(|| format!("{what} должен быть целым неотрицательным числом"))
}

fn want_u32(json: &Json, what: &str) -> Result<u32, String> {
    json.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("{what} должен быть номером объекта"))
}

fn want_str(json: &Json, what: &str) -> Result<String, String> {
    json.as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{what} должен быть строкой"))
}

fn parse_command(arr: &[Json]) -> Result<ReplayCommand, String> {
    let head = arr
        .first()
        .and_then(Json::as_str)
        .ok_or_else(|| "команда без имени".to_string())?;
    match head {
        "show_screen" => {
            let name = want_str(
                arr.get(1).ok_or("show_screen без экрана")?,
                "show_screen[1]",
            )?;
            Ok(ReplayCommand::ShowScreen(name))
        }
        "new_game" => {
            let name = want_str(arr.get(1).ok_or("new_game без экрана")?, "new_game[1]")?;
            let values = match arr.get(2) {
                Some(v) => v
                    .as_object()
                    .ok_or_else(|| "new_game[2] должен быть объектом".to_string())?
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                None => Vec::new(),
            };
            Ok(ReplayCommand::NewGame(name, values))
        }
        "resume" => Ok(ReplayCommand::Resume),
        "quit" => Ok(ReplayCommand::Quit),
        "toggle_sound" => Ok(ReplayCommand::ToggleSound),
        other => Err(format!("неизвестная команда \"{other}\"")),
    }
}

fn command_to_json(cmd: &ReplayCommand) -> Json {
    match cmd {
        ReplayCommand::ShowScreen(name) => Json::Array(vec![
            Json::String("show_screen".to_string()),
            Json::String(name.clone()),
        ]),
        ReplayCommand::NewGame(name, values) => {
            let mut arr = vec![
                Json::String("new_game".to_string()),
                Json::String(name.clone()),
            ];
            if !values.is_empty() {
                let map: serde_json::Map<String, Json> = values.iter().cloned().collect();
                arr.push(Json::Object(map));
            }
            Json::Array(arr)
        }
        ReplayCommand::Resume => Json::Array(vec![Json::String("resume".to_string())]),
        ReplayCommand::Quit => Json::Array(vec![Json::String("quit".to_string())]),
        ReplayCommand::ToggleSound => Json::Array(vec![Json::String("toggle_sound".to_string())]),
    }
}

/// Parses one `events[]` entry — exactly one of `command`/`key_down`/`key_up`/`cursor`/`set`/
/// `remove`/`add`/`delete` alongside `step`, unknown or missing being «Технические детали»'s
/// "неизвестное событие".
fn parse_event(json: &Json, index: usize) -> Result<ReplayEvent, String> {
    let obj = json
        .as_object()
        .ok_or_else(|| format!("events[{index}] должен быть объектом"))?;
    let step = want_u64(
        obj.get("step")
            .ok_or_else(|| format!("events[{index}] без \"step\""))?,
        "step",
    )?;
    let kind = if let Some(v) = obj.get("command") {
        let arr = v
            .as_array()
            .ok_or_else(|| format!("events[{index}].command должен быть списком"))?;
        ReplayEventKind::Command(parse_command(arr).map_err(|e| format!("events[{index}]: {e}"))?)
    } else if let Some(v) = obj.get("key_down") {
        ReplayEventKind::KeyDown(want_str(v, &format!("events[{index}].key_down"))?)
    } else if let Some(v) = obj.get("key_up") {
        ReplayEventKind::KeyUp(want_str(v, &format!("events[{index}].key_up"))?)
    } else if let Some(v) = obj.get("cursor") {
        let arr = v
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or_else(|| format!("events[{index}].cursor должен быть парой чисел"))?;
        let x = arr[0]
            .as_f64()
            .ok_or_else(|| format!("events[{index}].cursor[0] должен быть числом"))?;
        let y = arr[1]
            .as_f64()
            .ok_or_else(|| format!("events[{index}].cursor[1] должен быть числом"))?;
        ReplayEventKind::Cursor([x, y])
    } else if let Some(v) = obj.get("set") {
        let arr = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| {
            format!("events[{index}].set должен быть [номер, свойство, значение]")
        })?;
        let id = want_u32(&arr[0], &format!("events[{index}].set[0]"))?;
        let name = want_str(&arr[1], &format!("events[{index}].set[1]"))?;
        ReplayEventKind::Edit(ReplayEdit::Set(id, name, arr[2].clone()))
    } else if let Some(v) = obj.get("remove") {
        let arr = v
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or_else(|| format!("events[{index}].remove должен быть [номер, свойство]"))?;
        let id = want_u32(&arr[0], &format!("events[{index}].remove[0]"))?;
        let name = want_str(&arr[1], &format!("events[{index}].remove[1]"))?;
        ReplayEventKind::Edit(ReplayEdit::Remove(id, name))
    } else if let Some(v) = obj.get("add") {
        ReplayEventKind::Edit(ReplayEdit::Add(v.clone()))
    } else if let Some(v) = obj.get("delete") {
        let id = want_u32(v, &format!("events[{index}].delete"))?;
        ReplayEventKind::Edit(ReplayEdit::Delete(id))
    } else {
        return Err(format!(
            "events[{index}]: неизвестное событие — нет ни command, ни key_down/key_up/cursor, ни set/remove/add/delete"
        ));
    };
    Ok(ReplayEvent { step, kind })
}

fn event_to_json(event: &ReplayEvent) -> Json {
    let mut map = serde_json::Map::with_capacity(2);
    map.insert("step".to_string(), Json::from(event.step));
    match &event.kind {
        ReplayEventKind::KeyDown(code) => {
            map.insert("key_down".to_string(), Json::String(code.clone()));
        }
        ReplayEventKind::KeyUp(code) => {
            map.insert("key_up".to_string(), Json::String(code.clone()));
        }
        ReplayEventKind::Cursor([x, y]) => {
            map.insert("cursor".to_string(), serde_json::json!([x, y]));
        }
        ReplayEventKind::Command(cmd) => {
            map.insert("command".to_string(), command_to_json(cmd));
        }
        ReplayEventKind::Edit(ReplayEdit::Set(id, name, value)) => {
            map.insert(
                "set".to_string(),
                Json::Array(vec![
                    Json::from(*id),
                    Json::String(name.clone()),
                    value.clone(),
                ]),
            );
        }
        ReplayEventKind::Edit(ReplayEdit::Remove(id, name)) => {
            map.insert(
                "remove".to_string(),
                Json::Array(vec![Json::from(*id), Json::String(name.clone())]),
            );
        }
        ReplayEventKind::Edit(ReplayEdit::Add(props)) => {
            map.insert("add".to_string(), props.clone());
        }
        ReplayEventKind::Edit(ReplayEdit::Delete(id)) => {
            map.insert("delete".to_string(), Json::from(*id));
        }
    }
    Json::Object(map)
}

/// Parses a replay file's text — «Редактор», требование 35: any failure here is reported as "Это
/// не запись партии: <причина>" by the caller, which alone knows to add that prefix; this function
/// just names the reason.
pub fn parse(text: &str) -> Result<Recording, String> {
    let root: Json =
        serde_json::from_str(text).map_err(|e| format!("не разобралась как JSON: {e}"))?;
    let obj = root
        .as_object()
        .ok_or_else(|| "не объект верхнего уровня".to_string())?;
    let format = obj
        .get("format")
        .and_then(Json::as_u64)
        .ok_or_else(|| "нет числового поля \"format\"".to_string())?;
    if format != FORMAT {
        return Err(format!(
            "формат {format} не поддерживается, ожидался {FORMAT}"
        ));
    }
    let steps = want_u64(
        obj.get("steps")
            .ok_or_else(|| "нет поля \"steps\"".to_string())?,
        "steps",
    )?;
    let events = match obj.get("events") {
        Some(v) => v
            .as_array()
            .ok_or_else(|| "\"events\" должен быть списком".to_string())?
            .iter()
            .enumerate()
            .map(|(i, e)| parse_event(e, i))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    Ok(Recording { steps, events })
}

/// Writes a replay back to text — «Технические детали»: one event per line, so the file reads by
/// eye (and by an outside model). Not `serde_json::to_string_pretty`: that wraps every object
/// across several lines, one event per several lines instead of one.
pub fn serialize(recording: &Recording) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"format\": {FORMAT},\n"));
    out.push_str(&format!("  \"steps\": {},\n", recording.steps));
    out.push_str("  \"events\": [\n");
    for (i, event) in recording.events.iter().enumerate() {
        let line = serde_json::to_string(&event_to_json(event)).unwrap_or_default();
        let comma = if i + 1 == recording.events.len() {
            ""
        } else {
            ","
        };
        out.push_str(&format!("    {line}{comma}\n"));
    }
    out.push_str("  ]\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_survives_the_text_round_trip() {
        let recording = Recording {
            steps: 302,
            events: vec![
                ReplayEvent {
                    step: 0,
                    kind: ReplayEventKind::Command(ReplayCommand::NewGame(
                        "game".to_string(),
                        vec![("game.level".to_string(), serde_json::json!(5))],
                    )),
                },
                ReplayEvent {
                    step: 12,
                    kind: ReplayEventKind::KeyDown("ArrowLeft".to_string()),
                },
                ReplayEvent {
                    step: 15,
                    kind: ReplayEventKind::KeyUp("ArrowLeft".to_string()),
                },
                ReplayEvent {
                    step: 20,
                    kind: ReplayEventKind::Cursor([15.0, 22.5]),
                },
                ReplayEvent {
                    step: 300,
                    kind: ReplayEventKind::Edit(ReplayEdit::Set(
                        4,
                        "velocity".to_string(),
                        serde_json::json!([0, 20]),
                    )),
                },
                ReplayEvent {
                    step: 300,
                    kind: ReplayEventKind::Edit(ReplayEdit::Remove(4, "collides".to_string())),
                },
                ReplayEvent {
                    step: 301,
                    kind: ReplayEventKind::Edit(ReplayEdit::Add(
                        serde_json::json!({"position": [3, 4]}),
                    )),
                },
                ReplayEvent {
                    step: 302,
                    kind: ReplayEventKind::Edit(ReplayEdit::Delete(17)),
                },
            ],
        };
        let text = serialize(&recording);
        let parsed = parse(&text).expect("round trip parses");
        assert_eq!(parsed, recording);
    }

    #[test]
    fn a_plain_json_object_with_no_format_field_is_not_a_recording() {
        let err = parse("{}").unwrap_err();
        assert!(err.contains("format"), "{err}");
    }

    #[test]
    fn wrong_format_number_is_reported() {
        let err = parse(r#"{"format": 2, "steps": 0}"#).unwrap_err();
        assert!(err.contains("формат"), "{err}");
    }

    #[test]
    fn garbage_text_is_not_json() {
        let err = parse("не json").unwrap_err();
        assert!(err.contains("JSON"), "{err}");
    }

    #[test]
    fn an_event_naming_no_known_kind_is_rejected() {
        let err = parse(r#"{"format": 1, "steps": 1, "events": [{"step": 0}]}"#).unwrap_err();
        assert!(err.contains("неизвестное событие"), "{err}");
    }
}

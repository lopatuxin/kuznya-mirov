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
    /// «Экраны и состояние» → «Команды», `run`: имя функции кода и номер объекта кнопки — строки списка или
    /// элемента с `for`, — если он был. Повтор вызывает функцию в то же место записи.
    Run(String, Option<u32>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayEdit {
    Set(u32, String, Json),
    Remove(u32, String),
    Add(Json),
    Delete(u32),
    /// «Ветер и частицы» → «Время и случайность»: ровный ветер, поставленный в партии, — код игры его
    /// читает, и повтор без него разошёлся бы с партией.
    Wind([f64; 2]),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayEventKind {
    KeyDown(String),
    KeyUp(String),
    /// «Надписи, полоски и картинки в мире», требование 7: нажатие левой кнопки по картинке в мире —
    /// номер объекта, чья она. Повтор щёлкает по тому же объекту, где бы ни стоял курсор.
    Click(u32),
    /// Точка под курсором с высотой и, в трёхмерной игре, место камеры `eye` (клетки сцены и
    /// высота), от которого щелчок идёт лучом через эту точку. Запись с двумя числами точки читается
    /// с высотой 0.
    Cursor([f64; 3], Option<[f64; 3]>),
    /// «Камера игрока», требование 20: вид, в который игрок привёл камеру, — точка, на которую она
    /// смотрит (в трёхмерной игре с высотой), `view_height` и следит ли она за объектом.
    Camera {
        center: [f64; 2],
        z: Option<f64>,
        view_height: f64,
        follow: bool,
    },
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
        "run" => {
            let function = want_str(arr.get(1).ok_or("run без функции")?, "run[1]")?;
            let object = arr
                .get(2)
                .map(|object| want_u32(object, "run[2]"))
                .transpose()?;
            Ok(ReplayCommand::Run(function, object))
        }
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
        ReplayCommand::Run(function, object) => {
            let mut arr = vec![
                Json::String("run".to_string()),
                Json::String(function.clone()),
            ];
            arr.extend(object.map(Json::from));
            Json::Array(arr)
        }
    }
}

fn want_f64(json: &Json, what: &str) -> Result<f64, String> {
    json.as_f64()
        .ok_or_else(|| format!("{what} должен быть числом"))
}

/// `camera`: `{center: [x, y] или [x, y, z], view_height, follow}`; `follow` необязателен.
fn parse_camera_event(json: &Json, index: usize) -> Result<ReplayEventKind, String> {
    let obj = json
        .as_object()
        .ok_or_else(|| format!("events[{index}].camera должен быть объектом"))?;
    let center = obj
        .get("center")
        .and_then(Json::as_array)
        .filter(|a| matches!(a.len(), 2 | 3))
        .ok_or_else(|| {
            format!("events[{index}].camera.center должен быть парой или тройкой чисел")
        })?;
    let what = |axis: usize| format!("events[{index}].camera.center[{axis}]");
    let z = match center.get(2) {
        Some(z) => Some(want_f64(z, &what(2))?),
        None => None,
    };
    let view_height = want_f64(
        obj.get("view_height")
            .ok_or_else(|| format!("events[{index}].camera без \"view_height\""))?,
        &format!("events[{index}].camera.view_height"),
    )?;
    if !view_height.is_finite() || view_height <= 0.0 {
        return Err(format!(
            "events[{index}].camera.view_height должен быть больше нуля"
        ));
    }
    let follow = match obj.get("follow") {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| format!("events[{index}].camera.follow должен быть true или false"))?,
    };
    Ok(ReplayEventKind::Camera {
        center: [
            want_f64(&center[0], &what(0))?,
            want_f64(&center[1], &what(1))?,
        ],
        z,
        view_height,
        follow,
    })
}

/// Parses one `events[]` entry — exactly one of `command`/`key_down`/`key_up`/`click`/`cursor`/`camera`/`set`/
/// `remove`/`add`/`delete`/`wind` alongside `step`, unknown or missing being «Технические детали»'s
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
    } else if let Some(v) = obj.get("click") {
        ReplayEventKind::Click(want_u32(v, &format!("events[{index}].click"))?)
    } else if let Some(v) = obj.get("cursor") {
        let arr = v
            .as_array()
            .filter(|a| matches!(a.len(), 2 | 3))
            .ok_or_else(|| format!("events[{index}].cursor должен быть парой или тройкой чисел"))?;
        let x = arr[0]
            .as_f64()
            .ok_or_else(|| format!("events[{index}].cursor[0] должен быть числом"))?;
        let y = arr[1]
            .as_f64()
            .ok_or_else(|| format!("events[{index}].cursor[1] должен быть числом"))?;
        let z = match arr.get(2) {
            None => 0.0,
            Some(z) => z
                .as_f64()
                .ok_or_else(|| format!("events[{index}].cursor[2] должен быть числом"))?,
        };
        let eye = match obj.get("eye") {
            None => None,
            Some(v) => {
                let arr = v
                    .as_array()
                    .filter(|a| a.len() == 3)
                    .ok_or_else(|| format!("events[{index}].eye должен быть тройкой чисел"))?;
                let mut eye = [0.0; 3];
                for (axis, slot) in eye.iter_mut().enumerate() {
                    *slot = arr[axis]
                        .as_f64()
                        .ok_or_else(|| format!("events[{index}].eye[{axis}] должен быть числом"))?;
                }
                Some(eye)
            }
        };
        ReplayEventKind::Cursor([x, y, z], eye)
    } else if let Some(v) = obj.get("camera") {
        parse_camera_event(v, index)?
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
    } else if let Some(v) = obj.get("wind") {
        let arr = v
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or_else(|| format!("events[{index}].wind должен быть парой чисел"))?;
        let x = arr[0]
            .as_f64()
            .ok_or_else(|| format!("events[{index}].wind[0] должен быть числом"))?;
        let y = arr[1]
            .as_f64()
            .ok_or_else(|| format!("events[{index}].wind[1] должен быть числом"))?;
        ReplayEventKind::Edit(ReplayEdit::Wind([x, y]))
    } else {
        return Err(format!(
            "events[{index}]: неизвестное событие — нет ни command, ни key_down/key_up/click/cursor/camera, ни set/remove/add/delete/wind"
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
        ReplayEventKind::Click(object) => {
            map.insert("click".to_string(), Json::from(*object));
        }
        ReplayEventKind::Cursor([x, y, z], eye) => {
            let point = match eye {
                Some(_) => serde_json::json!([x, y, z]),
                None => serde_json::json!([x, y]),
            };
            map.insert("cursor".to_string(), point);
            if let Some(eye) = eye {
                map.insert("eye".to_string(), serde_json::json!(eye));
            }
        }
        ReplayEventKind::Camera {
            center,
            z,
            view_height,
            follow,
        } => {
            let point = match z {
                Some(z) => serde_json::json!([center[0], center[1], z]),
                None => serde_json::json!([center[0], center[1]]),
            };
            map.insert(
                "camera".to_string(),
                serde_json::json!({"center": point, "view_height": view_height, "follow": follow}),
            );
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
        ReplayEventKind::Edit(ReplayEdit::Wind(wind)) => {
            map.insert("wind".to_string(), serde_json::json!(wind));
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
                    step: 16,
                    kind: ReplayEventKind::Click(7),
                },
                ReplayEvent {
                    step: 20,
                    kind: ReplayEventKind::Cursor([15.0, 22.5, 0.0], None),
                },
                ReplayEvent {
                    step: 20,
                    kind: ReplayEventKind::Cursor([15.0, 22.5, 3.0], Some([16.0, 26.9, 12.8])),
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
                ReplayEvent {
                    step: 303,
                    kind: ReplayEventKind::Edit(ReplayEdit::Wind([3.0, -0.5])),
                },
            ],
        };
        let text = serialize(&recording);
        let parsed = parse(&text).expect("round trip parses");
        assert_eq!(parsed, recording);
    }

    #[test]
    fn a_run_command_keeps_its_function_and_the_object_of_the_button() {
        let recording = Recording {
            steps: 4,
            events: vec![
                ReplayEvent {
                    step: 1,
                    kind: ReplayEventKind::Command(ReplayCommand::Run("take".to_string(), Some(7))),
                },
                ReplayEvent {
                    step: 2,
                    kind: ReplayEventKind::Command(ReplayCommand::Run("count".to_string(), None)),
                },
            ],
        };
        let text = serialize(&recording);
        assert!(text.contains(r#"["run","take",7]"#), "{text}");
        assert!(text.contains(r#"["run","count"]"#), "{text}");
        assert_eq!(parse(&text).expect("читается"), recording);
        let bad = r#"{"format": 1, "steps": 1, "events": [{"step": 0, "command": ["run", "take", "x"]}]}"#;
        assert!(parse(bad).unwrap_err().contains("run[2]"));
    }

    #[test]
    fn the_camera_position_rides_next_to_the_cursor_point_in_the_same_event() {
        let event = ReplayEvent {
            step: 20,
            kind: ReplayEventKind::Cursor([15.0, 22.5, 3.0], Some([16.0, 26.9, 12.8])),
        };
        let line = serde_json::to_string(&event_to_json(&event)).expect("serializes");
        assert_eq!(
            line,
            r#"{"cursor":[15.0,22.5,3.0],"eye":[16.0,26.9,12.8],"step":20}"#
        );
        let bad = r#"{"format": 1, "steps": 1, "events": [{"step": 0, "cursor": [1, 2], "eye": [1, 2]}]}"#;
        assert!(parse(bad).unwrap_err().contains("eye"));
    }

    #[test]
    fn a_camera_event_survives_the_round_trip_with_and_without_a_height() {
        let recording = Recording {
            steps: 5,
            events: vec![
                ReplayEvent {
                    step: 2,
                    kind: ReplayEventKind::Camera {
                        center: [12.5, 30.0],
                        z: None,
                        view_height: 46.0,
                        follow: false,
                    },
                },
                ReplayEvent {
                    step: 3,
                    kind: ReplayEventKind::Camera {
                        center: [12.5, 30.0],
                        z: Some(1.2),
                        view_height: 40.0,
                        follow: true,
                    },
                },
            ],
        };
        let text = serialize(&recording);
        assert_eq!(parse(&text).expect("читается"), recording);
        let line = serde_json::to_string(&event_to_json(&recording.events[0])).expect("пишется");
        assert_eq!(
            line,
            r#"{"camera":{"center":[12.5,30.0],"follow":false,"view_height":46.0},"step":2}"#
        );
    }

    #[test]
    fn a_camera_event_without_a_view_or_with_a_bad_one_is_rejected() {
        for bad in [
            r#"{"step": 0, "camera": {"center": [1, 2]}}"#,
            r#"{"step": 0, "camera": {"center": [1], "view_height": 5}}"#,
            r#"{"step": 0, "camera": {"center": [1, 2], "view_height": 0}}"#,
            r#"{"step": 0, "camera": {"center": [1, 2], "view_height": 5, "follow": 1}}"#,
            r#"{"step": 0, "camera": 5}"#,
        ] {
            let text = format!(r#"{{"format": 1, "steps": 1, "events": [{bad}]}}"#);
            assert!(parse(&text).unwrap_err().contains("camera"), "{bad}");
        }
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

    /// Фаза 44, требование 7: щелчок по картинке в мире записан номером объекта.
    #[test]
    fn a_click_on_a_world_image_is_written_as_the_object_number() {
        let text = r#"{"format": 1, "steps": 3, "events": [{"step": 1, "click": 7}]}"#;
        let recording = parse(text).expect("запись должна разобраться");
        assert_eq!(recording.events[0].kind, ReplayEventKind::Click(7));
        assert!(serialize(&recording).contains(r#"{"click":7,"step":1}"#));
        let err = parse(r#"{"format": 1, "steps": 1, "events": [{"step": 0, "click": "a"}]}"#)
            .unwrap_err();
        assert!(err.contains("click"), "{err}");
    }

    #[test]
    fn an_event_naming_no_known_kind_is_rejected() {
        let err = parse(r#"{"format": 1, "steps": 1, "events": [{"step": 0}]}"#).unwrap_err();
        assert!(err.contains("неизвестное событие"), "{err}");
    }
}

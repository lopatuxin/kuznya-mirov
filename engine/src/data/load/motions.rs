//! «Движения» → «Движение с поправками», «Проверка перед запуском»: файл `motions.json` — движения моделей со
//! скелетом с поправками. Каждое движение ложится на скелет своей модели ещё одним `Clip` с поправками, поэтому его
//! имя годится везде, где годится имя движения файла модели.

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::core::skeleton::{BoneTurn, Clip, Retouch, Skeleton, Turns};

use super::models::ModelDecl;
use super::{
    ErrorSink, expect_number, expect_object, expect_string, join, kind_name, parse_json_or_error,
    reject_unknown_keys, require_field,
};

/// Наибольшее расхождение конца и начала повторяющегося движения, при котором кость ещё сходится сама, в градусах.
pub const END_GAP_LIMIT: f32 = 5.0;

/// Поправка кости, как она записана: тройка или отметки по возрастанию доли.
#[derive(Debug, Clone, PartialEq)]
enum TurnsDecl {
    Constant([f32; 3]),
    Keyed(Vec<(f32, [f32; 3])>),
}

/// Одно движение `motions.json`, чьи ключи уже разобраны; кости и движения ещё не сверены со скелетом.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionDecl {
    pub name: String,
    pub model: String,
    clip: String,
    speed: f32,
    looped: Option<bool>,
    upper: Option<String>,
    bounce: f32,
    lean: f32,
    arms_out: f32,
    bones: Vec<(String, TurnsDecl)>,
}

/// Весь `motions.json`: путь файла и движения без ошибок разбора, в порядке имён.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MotionSet {
    pub file: String,
    pub decls: Vec<MotionDecl>,
}

impl MotionSet {
    pub fn upper_names(&self) -> impl Iterator<Item = &str> {
        self.decls.iter().filter_map(|decl| decl.upper.as_deref())
    }
}

/// Разбирает текст `motions.json`: таблицу «имя → описание». Описание с ошибкой пропускается, его ошибки записаны.
pub(super) fn parse_motions(
    text: &str,
    file: &str,
    models: &[ModelDecl],
    errors: &mut ErrorSink,
) -> MotionSet {
    let mut set = MotionSet {
        file: file.to_string(),
        decls: Vec::new(),
    };
    let Some(json) = parse_json_or_error(file, text, errors) else {
        return set;
    };
    let Some(table) = expect_object(&json, file, "", errors) else {
        return set;
    };
    for (name, value) in table {
        if name.is_empty() {
            errors.push(
                file,
                "",
                "пустое имя: у движения должно быть имя, которым его назовёт idle, walk или animation",
            );
            continue;
        }
        if let Some(decl) = parse_decl(name, value, models, file, errors) {
            set.decls.push(decl);
        }
    }
    set
}

fn parse_decl(
    name: &str,
    value: &Json,
    models: &[ModelDecl],
    file: &str,
    errors: &mut ErrorSink,
) -> Option<MotionDecl> {
    let obj = expect_object(value, file, name, errors)?;
    let known = [
        "model",
        "clip",
        "speed",
        "loop",
        "upper",
        "bounce",
        "lean",
        "arms_out",
        "bones",
        "reference",
    ];
    reject_unknown_keys(obj, &known, file, name, errors);
    if !obj.keys().all(|key| known.contains(&key.as_str())) {
        return None;
    }
    let string = |key: &str, errors: &mut ErrorSink| {
        let value = require_field(obj, key, file, name, errors)?;
        expect_string(value, file, &join(name, key), errors)
    };
    let (model, clip) = (string("model", errors), string("clip", errors));
    let number = |key: &str, default: f32, errors: &mut ErrorSink| -> Option<f32> {
        match obj.get(key) {
            None => Some(default),
            Some(value) => expect_number(value, file, &join(name, key), errors).map(|n| n as f32),
        }
    };
    let speed = number("speed", 1.0, errors).and_then(|speed| {
        if speed > 0.0 {
            return Some(speed);
        }
        errors.push(
            file,
            &join(name, "speed"),
            format!("speed — число больше нуля, получено {speed}"),
        );
        None
    });
    let looped = match obj.get("loop") {
        None => Some(None),
        Some(Json::Bool(flag)) => Some(Some(*flag)),
        Some(other) => {
            errors.push(
                file,
                &join(name, "loop"),
                format!("loop — true или false, получено {}", kind_name(other)),
            );
            None
        }
    };
    let upper = match obj.get("upper") {
        None => Some(None),
        Some(value) => expect_string(value, file, &join(name, "upper"), errors).map(Some),
    };
    let (bounce, lean, arms_out) = (
        number("bounce", 1.0, errors),
        number("lean", 0.0, errors),
        number("arms_out", 0.0, errors),
    );
    let bones = match obj.get("bones") {
        None => Some(Vec::new()),
        Some(value) => parse_bones(value, file, &join(name, "bones"), errors),
    };
    let model = model?;
    if !models.iter().any(|decl| decl.name == model) {
        let known: Vec<String> = models
            .iter()
            .map(|decl| format!("\"{}\"", decl.name))
            .collect();
        errors.push(
            file,
            &join(name, "model"),
            format!(
                "модель \"{model}\" не объявлена в game.json → files → models; объявлены {}",
                if known.is_empty() {
                    "никакие".to_string()
                } else {
                    known.join(", ")
                }
            ),
        );
        return None;
    }
    Some(MotionDecl {
        name: name.to_string(),
        model,
        clip: clip?,
        speed: speed?,
        looped: looped?,
        upper: upper?,
        bounce: bounce?,
        lean: lean?,
        arms_out: arms_out?,
        bones: bones?,
    })
}

fn parse_bones(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<(String, TurnsDecl)>> {
    let table = expect_object(value, file, path, errors)?;
    let mut bones = Vec::with_capacity(table.len());
    let mut broken = false;
    for (bone, turns) in table {
        match parse_turns(turns, file, &join(path, bone), errors) {
            Some(turns) => bones.push((bone.clone(), turns)),
            None => broken = true,
        }
    }
    (!broken).then_some(bones)
}

fn parse_turns(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<TurnsDecl> {
    let Some(items) = value.as_array() else {
        errors.push(
            file,
            path,
            format!(
                "поправка кости — три числа [x, y, z] в градусах или список отметок {{ \"at\", \"turn\" }}, получено {}",
                kind_name(value)
            ),
        );
        return None;
    };
    if items.iter().all(Json::is_number) {
        return triple(value, file, path, errors).map(TurnsDecl::Constant);
    }
    if items.iter().all(Json::is_object) {
        return parse_marks(items, file, path, errors).map(TurnsDecl::Keyed);
    }
    errors.push(
        file,
        path,
        "поправка кости — либо три числа, либо список отметок { \"at\", \"turn\" }; вперемешку нельзя",
    );
    None
}

fn triple(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<[f32; 3]> {
    let numbers: Option<Vec<f32>> =
        value
            .as_array()
            .filter(|items| items.len() == 3)
            .and_then(|items| {
                items
                    .iter()
                    .map(|item| item.as_f64().map(|n| n as f32))
                    .collect()
            });
    let numbers = numbers.and_then(|numbers| <[f32; 3]>::try_from(numbers).ok());
    if numbers.is_none() {
        errors.push(
            file,
            path,
            format!(
                "ожидалась тройка чисел [x, y, z] в градусах, получено {}",
                kind_name(value)
            ),
        );
    }
    numbers
}

fn parse_marks(
    items: &[Json],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<(f32, [f32; 3])>> {
    if items.is_empty() {
        errors.push(file, path, "список отметок пуст: нужна хотя бы одна");
        return None;
    }
    let mut marks = Vec::with_capacity(items.len());
    let mut broken = false;
    for (index, item) in items.iter().enumerate() {
        let mark_path = join(path, &format!("[{index}]"));
        match parse_mark(item, file, &mark_path, errors) {
            Some(mark) => marks.push(mark),
            None => broken = true,
        }
    }
    if broken {
        return None;
    }
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));
    if marks.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        errors.push(file, path, "две отметки в одной доле движения");
        return None;
    }
    Some(marks)
}

fn parse_mark(
    item: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<(f32, [f32; 3])> {
    let obj = expect_object(item, file, path, errors)?;
    reject_unknown_keys(obj, &["at", "turn"], file, path, errors);
    let at = require_field(obj, "at", file, path, errors)
        .and_then(|value| expect_number(value, file, &join(path, "at"), errors));
    let turn = require_field(obj, "turn", file, path, errors)
        .and_then(|value| triple(value, file, &join(path, "turn"), errors));
    let at = at?;
    if !(0.0..=1.0).contains(&at) {
        errors.push(
            file,
            &join(path, "at"),
            format!("at — доля движения от 0 до 1, получено {at}"),
        );
        return None;
    }
    Some((at as f32, turn?))
}

/// Кладёт на скелет модели `model` её движения из `set`, ключи `spine` и `hands` уже на скелете. Названия, которых
/// нет в скелете, и поправки, которым не хватает ключа модели, — ошибки; движение с ошибкой на скелет не ложится.
pub(super) fn attach(
    skeleton: &mut Skeleton,
    model: &ModelDecl,
    set: &MotionSet,
    errors: &mut ErrorSink,
) {
    let file_clips = skeleton.clips.len();
    let mine: Vec<&MotionDecl> = set
        .decls
        .iter()
        .filter(|decl| decl.model == model.name)
        .collect();
    let mut placed: Vec<&MotionDecl> = Vec::new();
    for decl in &mine {
        if let Some(clip) = base_clip(skeleton, file_clips, decl, &set.file, errors) {
            skeleton.clips.push(clip);
            placed.push(decl);
        }
    }
    let mut kept = Vec::with_capacity(placed.len());
    for (index, decl) in placed.into_iter().enumerate() {
        let at = file_clips + index;
        match retouch_of(skeleton, model, decl, &set.file, errors) {
            Some(retouch) => {
                skeleton.clips[at].retouch = Some(retouch);
                kept.push((at, decl));
            }
            None => skeleton.clips[at].length = 0.0,
        }
    }
    check_uppers(skeleton, file_clips, &set.file, errors);
    warn_end_gaps(skeleton, model, &kept, &set.file, errors);
}

/// Пока без поправок: длина движения файла, делённая на `speed`. Номера движений `upper` и поправки встанут, когда
/// все движения модели уже лежат на скелете.
fn base_clip(
    skeleton: &Skeleton,
    file_clips: usize,
    decl: &MotionDecl,
    file: &str,
    errors: &mut ErrorSink,
) -> Option<Clip> {
    if skeleton.clips[..file_clips]
        .iter()
        .any(|clip| clip.name == decl.name)
        || skeleton.clips[file_clips..]
            .iter()
            .any(|clip| clip.name == decl.name)
    {
        errors.push(
            file,
            &decl.name,
            "это имя уже занято движением файла модели или другим движением motions.json",
        );
        return None;
    }
    let base = file_clip(
        skeleton,
        file_clips,
        &decl.clip,
        file,
        &join(&decl.name, "clip"),
        errors,
    )?;
    let length = skeleton.clips[base].length / decl.speed;
    Some(Clip {
        name: decl.name.clone(),
        start: 0.0,
        length: if skeleton.clips[base].is_playable() {
            length
        } else {
            0.0
        },
        tracks: Vec::new(),
        retouch: Some(Retouch {
            base,
            looped: decl.looped,
            upper: None,
            bounce: 1.0,
            pelvis: None,
            lean: 0.0,
            arms_out: 0.0,
            bones: Vec::new(),
        }),
    })
}

fn file_clip(
    skeleton: &Skeleton,
    file_clips: usize,
    name: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<usize> {
    let found = skeleton.clips[..file_clips]
        .iter()
        .position(|clip| clip.name == name);
    if found.is_none() {
        errors.push(file, path, missing_clip(skeleton, file_clips, name));
    }
    found
}

fn missing_clip(skeleton: &Skeleton, file_clips: usize, name: &str) -> String {
    let known: Vec<String> = skeleton.clips[..file_clips]
        .iter()
        .map(|clip| format!("\"{}\"", clip.name))
        .collect();
    format!(
        "в файле модели нет движения \"{name}\"; в нём {}",
        if known.is_empty() {
            "движений нет".to_string()
        } else {
            known.join(", ")
        }
    )
}

fn retouch_of(
    skeleton: &Skeleton,
    model: &ModelDecl,
    decl: &MotionDecl,
    file: &str,
    errors: &mut ErrorSink,
) -> Option<Retouch> {
    let before = errors.error_count();
    let own = skeleton.clip_named(&decl.name)?;
    let base = skeleton.clips[own].retouch.as_ref()?.base;
    let upper = decl.upper.as_deref().and_then(|name| {
        if skeleton.spine.is_none() {
            errors.push(
                file,
                &join(&decl.name, "upper"),
                format!(
                    "у модели {} нет ключа spine: верх тела не от чего отсчитать",
                    model.name
                ),
            );
            return None;
        }
        let found = skeleton.clip_named(name);
        if found.is_none() {
            errors.push(
                file,
                &join(&decl.name, "upper"),
                format!("нет движения \"{name}\" ни в файле модели, ни в motions.json"),
            );
        }
        found
    });
    for (key, needs, value) in [
        ("lean", skeleton.spine, decl.lean),
        (
            "arms_out",
            skeleton.hands.map(|hands| hands[0]),
            decl.arms_out,
        ),
    ] {
        if value != 0.0 && needs.is_none() {
            let needed = if key == "lean" { "spine" } else { "hands" };
            errors.push(
                file,
                &join(&decl.name, key),
                format!("у модели {} нет ключа {needed}", model.name),
            );
        }
    }
    if decl.bounce != 1.0 && skeleton.legs.is_none() {
        errors.push(
            file,
            &join(&decl.name, "bounce"),
            format!("у модели {} нет ключа feet: таз не найти", model.name),
        );
    }
    let mut bones = Vec::with_capacity(decl.bones.len());
    for (name, turns) in &decl.bones {
        let Some(bone) = skeleton.bone_named(name) else {
            errors.push(
                file,
                &join(&join(&decl.name, "bones"), name),
                format!("в скелете модели нет кости \"{name}\""),
            );
            continue;
        };
        bones.push(BoneTurn {
            bone,
            turns: match turns {
                TurnsDecl::Constant(turn) => Turns::Constant(*turn),
                TurnsDecl::Keyed(marks) => Turns::Keyed(marks.clone()),
            },
        });
    }
    if errors.error_count() > before || (decl.upper.is_some() && upper.is_none()) {
        return None;
    }
    Some(Retouch {
        base,
        looped: decl.looped,
        upper,
        bounce: decl.bounce,
        pelvis: skeleton.pelvis_rise(base),
        lean: decl.lean,
        arms_out: decl.arms_out,
        bones,
    })
}

/// Движение `upper` само не берёт верх у третьего: цепочку из двух звеньев движок не строит.
fn check_uppers(skeleton: &mut Skeleton, file_clips: usize, file: &str, errors: &mut ErrorSink) {
    let doubled: Vec<(usize, usize)> = (file_clips..skeleton.clips.len())
        .filter_map(|at| {
            let upper = skeleton.clips[at].retouch.as_ref()?.upper?;
            let inner = skeleton.clips[upper].retouch.as_ref()?;
            inner.upper.is_some().then_some((at, upper))
        })
        .collect();
    for (at, upper) in doubled {
        errors.push(
            file,
            &join(&skeleton.clips[at].name, "upper"),
            format!(
                "движение \"{}\" само берёт верх тела у другого: верх берётся только из движения без своего upper",
                skeleton.clips[upper].name
            ),
        );
        if let Some(retouch) = skeleton.clips[at].retouch.as_mut() {
            retouch.upper = None;
        }
    }
}

/// Предупреждение: у повторяющегося движения конец и начало расходятся больше чем на `END_GAP_LIMIT`° в одной из костей.
fn warn_end_gaps(
    skeleton: &Skeleton,
    model: &ModelDecl,
    decls: &[(usize, &MotionDecl)],
    file: &str,
    errors: &mut ErrorSink,
) {
    for &(at, decl) in decls {
        let repeats = decl.looped.unwrap_or_else(|| {
            model.idle.as_deref() == Some(decl.name.as_str())
                || model.walk.as_deref() == Some(decl.name.as_str())
        });
        if !repeats {
            continue;
        }
        let Some((degrees, bone)) = skeleton.ends_gap(at, true) else {
            continue;
        };
        if degrees > END_GAP_LIMIT {
            errors.push_warning(
                file,
                &decl.name,
                format!(
                    "движение повторяется, а его конец расходится с началом на {degrees:.0}° в кости \"{}\": на стыке круга поза дёрнется",
                    skeleton.bones[bone].name
                ),
            );
        }
    }
}

/// Для стенда анимации: скелет модели `model` без движений прежнего `motions.json`, с движениями из `text` — тот
/// же разбор и те же проверки, что при загрузке игры. `models` — имена моделей игры. Ошибки — по одной строке на
/// каждую.
pub fn rebuild_skeleton(
    skeleton: &Skeleton,
    model: &str,
    models: &[String],
    text: &str,
) -> Result<Skeleton, Vec<String>> {
    let declared: Vec<ModelDecl> = models
        .iter()
        .map(|name| ModelDecl {
            name: name.clone(),
            path: String::new(),
            idle: None,
            walk: None,
            feet: None,
            spine: None,
            hands: None,
        })
        .collect();
    let mut errors = ErrorSink::new();
    let set = parse_motions(text, "motions.json", &declared, &mut errors);
    let mut rebuilt = skeleton.clone();
    rebuilt.clips.retain(|clip| clip.retouch.is_none());
    let kept = rebuilt.clips.len();
    for slot in [&mut rebuilt.idle, &mut rebuilt.walk] {
        *slot = slot.filter(|&clip| clip < kept);
    }
    if let Some(target) = declared.iter().find(|decl| decl.name == model) {
        attach(&mut rebuilt, target, &set, &mut errors);
    }
    let (found, _) = errors.into_parts();
    if found.is_empty() {
        return Ok(rebuilt);
    }
    Err(found
        .iter()
        .map(|error| match error.path.is_empty() {
            true => format!("{}: {}", error.file, error.message),
            false => format!("{} → {}: {}", error.file, error.path, error.message),
        })
        .collect())
}

/// Предупреждение: движение `motions.json` нигде не названо — ни в `idle` и `walk` модели, ни в `animation` объектов
/// сцены, ни как `upper` другого движения, ни строкой в `rules.json`, ни словом в коде.
pub(super) fn warn_unused_motions(
    set: &MotionSet,
    models: &[ModelDecl],
    named: &HashSet<String>,
    rules: Option<&str>,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let uppers: HashSet<&str> = set.upper_names().collect();
    for decl in &set.decls {
        let quoted = format!("\"{}\"", decl.name);
        let by_model = models.iter().any(|model| {
            model.idle.as_deref() == Some(decl.name.as_str())
                || model.walk.as_deref() == Some(decl.name.as_str())
        });
        if by_model
            || named.contains(&decl.name)
            || uppers.contains(decl.name.as_str())
            || rules.is_some_and(|rules| rules.contains(&quoted))
            || code.is_some_and(|code| super::code_mentions_word(code, &decl.name))
        {
            continue;
        }
        errors.push_warning(
            &set.file,
            &decl.name,
            format!(
                "движение \"{}\" нигде не названо ни в idle и walk модели, ни в animation объектов, ни в rules.json, ни в тексте кода",
                decl.name
            ),
        );
    }
}

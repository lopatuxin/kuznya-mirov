//! Проход по одной игре: `games/<игра>/imported/` приводится в соответствие с её моделями `.glb`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ktx2_writer;
use crate::model::{self, TextureJob};
use crate::texture::{self, FORMAT_VERSION};

const IMPORTED: &str = "imported";
const INDEX_NAME: &str = "index.json";
const SOURCE_PREFIX: usize = 8;
/// Сколько файл, на который перестал ссылаться `index.json`, ещё лежит: страница, успевшая прочитать прежний индекс,
/// докачивает его файлы.
const GRACE: Duration = Duration::from_secs(60);

pub type Log<'a> = &'a dyn Fn(&str);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub source: String,
    pub model: String,
    pub images: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub models: BTreeMap<String, Entry>,
}

impl Entry {
    /// Пути от папки игры: облегчённая модель и сжатые файлы её картинок.
    fn files(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.model).chain(self.images.iter().flatten())
    }
}

impl Default for Index {
    fn default() -> Self {
        Index {
            version: FORMAT_VERSION,
            models: BTreeMap::new(),
        }
    }
}

fn parse_index(bytes: &[u8]) -> Option<Index> {
    serde_json::from_slice(bytes)
        .ok()
        .filter(|index: &Index| index.version == FORMAT_VERSION)
}

pub fn imported_dir(game_dir: &Path) -> PathBuf {
    game_dir.join(IMPORTED)
}

pub fn index_path(game_dir: &Path) -> PathBuf {
    imported_dir(game_dir).join(INDEX_NAME)
}

/// Файлы, которые называет `imported/index.json` игры, с путями от папки игры; индекса нет или он не читается —
/// пусто.
pub fn indexed_files(game_dir: &Path) -> Vec<PathBuf> {
    let index = fs::read(index_path(game_dir))
        .ok()
        .and_then(|bytes| parse_index(&bytes));
    index
        .iter()
        .flat_map(|index| index.models.values())
        .flat_map(Entry::files)
        .map(|path| game_dir.join(path))
        .collect()
}

/// Результат сжатия моделей игры, ещё не вошедший в `index.json`.
pub struct Pending {
    game_dir: PathBuf,
    index: Index,
    previous: Option<Vec<u8>>,
}

/// Причина последнего отказа по пути модели: одну и ту же строку «не сжато» журнал не повторяет.
pub type Refused = HashMap<String, String>;

/// Проход по игре; `true` — в `imported/` остались ничейные файлы, которые удалит `clean`, когда пройдёт минута.
pub fn sync(game_dir: &Path, log: Log, refused: &mut Refused) -> Result<bool> {
    let pending = compress_changed(game_dir, log, refused)?;
    commit(pending)
}

/// Сжимает новые и изменённые модели игры: пишет их картинки и облегчённые модели, но не `index.json`.
pub fn compress_changed(game_dir: &Path, log: Log, refused: &mut Refused) -> Result<Pending> {
    let game = game_name(game_dir);
    let previous = fs::read(index_path(game_dir)).ok();
    let old = previous
        .as_deref()
        .and_then(parse_index)
        .unwrap_or_default();

    let mut index = Index::default();
    let models = find_models(game_dir)?;
    let present: HashSet<&String> = models.iter().map(|(key, _)| key).collect();
    refused.retain(|key, _| present.contains(key));
    for (key, path) in &models {
        match refresh(game_dir, key, path, old.models.get(key), &game, log) {
            Ok(entry) => {
                refused.remove(key);
                index.models.insert(key.clone(), entry);
            }
            Err(Failure::Io(error)) => return Err(error),
            Err(Failure::Refused(error)) => {
                let reason = format!("{error:#}");
                if refused.insert(key.clone(), reason.clone()).as_ref() != Some(&reason) {
                    log(&format!("не сжато: {game}/{key} — {reason}"));
                }
            }
        }
    }
    for key in old.models.keys().filter(|key| !present.contains(key)) {
        log(&format!("удалено: {game}/{key}"));
    }
    Ok(Pending {
        game_dir: game_dir.to_path_buf(),
        index,
        previous,
    })
}

/// Пишет `index.json` последним и убирает файлы, на которые он давно не ссылается (`clean`).
pub fn commit(pending: Pending) -> Result<bool> {
    let Pending {
        game_dir,
        index,
        previous,
    } = pending;
    if index.models.is_empty() && previous.is_none() {
        return Ok(false);
    }
    let mut bytes = serde_json::to_vec_pretty(&index)?;
    bytes.push(b'\n');
    if previous.as_deref() != Some(&bytes) {
        write_atomic(&index_path(&game_dir), &bytes)?;
    }
    clean(&game_dir)
}

/// Прошла ли минута с последней записи `index.json`: раньше ничейные файлы не удаляются.
pub fn grace_over(game_dir: &Path) -> bool {
    fs::metadata(index_path(game_dir))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= GRACE)
}

/// Удаляет из `imported/` файлы, которых `index.json` не называет, но не раньше чем через минуту после его
/// последней записи; `true` — такие файлы ещё остались. Нет индекса — ничего не удаляется.
pub fn clean(game_dir: &Path) -> Result<bool> {
    let Some(index) = fs::read(index_path(game_dir))
        .ok()
        .and_then(|bytes| parse_index(&bytes))
    else {
        return Ok(false);
    };
    let mut keep: HashSet<String> = HashSet::from([INDEX_NAME.to_string()]);
    for entry in index.models.values() {
        keep.extend(
            entry
                .files()
                .filter_map(|path| path.strip_prefix("imported/"))
                .map(str::to_string),
        );
    }
    let imported = imported_dir(game_dir);
    let mut stale = Vec::new();
    collect_stale(&imported, "", &keep, &mut stale)?;
    if stale.is_empty() {
        return Ok(false);
    }
    if !grace_over(game_dir) {
        return Ok(true);
    }
    for path in &stale {
        fs::remove_file(path).with_context(|| format!("файл не удаляется: {}", path.display()))?;
    }
    remove_empty_folders(&imported);
    Ok(false)
}

/// Чем кончилась попытка сжать модель: модель не годится (её файл не разбирается) или не удался ввод-вывод.
enum Failure {
    Refused(anyhow::Error),
    Io(anyhow::Error),
}

fn refresh(
    game_dir: &Path,
    key: &str,
    path: &Path,
    old: Option<&Entry>,
    game: &str,
    log: Log,
) -> Result<Entry, Failure> {
    let bytes = fs::read(path)
        .with_context(|| format!("файл не читается: {}", path.display()))
        .map_err(Failure::Io)?;
    let source = texture::hex(&Sha256::digest(&bytes));
    if let Some(entry) = old
        && entry.source == source
        && files_intact(game_dir, entry)
    {
        return Ok(entry.clone());
    }

    let started = Instant::now();
    let light = light_path(key, &source);
    let lightened = model::lighten(&bytes, &light).map_err(Failure::Refused)?;
    let imported = imported_dir(game_dir);
    let light_file = imported.join(&light);
    let light_ready = fs::read(&light_file).is_ok_and(|ready| ready == lightened.glb);
    let fresh = write_textures(&imported.join("textures"), &lightened.textures)?;
    if !light_ready {
        write_atomic(&light_file, &lightened.glb).map_err(Failure::Io)?;
    }
    let images: Vec<Option<String>> = lightened
        .textures
        .iter()
        .map(|job| {
            job.as_ref()
                .map(|job| format!("imported/textures/{}.ktx2", job.stem))
        })
        .collect();
    if !light_ready || fresh > 0 {
        log(&format!(
            "сжато: {game}/{key} — {}, новых {fresh}, {} с",
            pictures(images.iter().flatten().count()),
            format!("{:.1}", started.elapsed().as_secs_f64()).replace('.', ",")
        ));
    }
    Ok(Entry {
        source,
        model: format!("{IMPORTED}/{light}"),
        images,
    })
}

/// Сжимает на всех ядрах картинки, файлов которых ещё нет; возвращает, сколько файлов написано.
fn write_textures(dir: &Path, jobs: &[Option<TextureJob>]) -> Result<usize, Failure> {
    let missing: BTreeMap<&str, &TextureJob> = jobs
        .iter()
        .flatten()
        .filter(|job| !texture_intact(&dir.join(format!("{}.ktx2", job.stem))))
        .map(|job| (job.stem.as_str(), job))
        .collect();
    missing.par_iter().try_for_each(|(stem, job)| {
        let file = texture::encode(&job.source, job.encoding, job.role)
            .with_context(|| format!("картинка {stem}"))
            .map_err(Failure::Refused)?;
        write_atomic(&dir.join(format!("{stem}.ktx2")), &file).map_err(Failure::Io)
    })?;
    Ok(missing.len())
}

/// Облегчённая модель и сжатые файлы записи на месте и целы; иначе модель пишется заново.
fn files_intact(game_dir: &Path, entry: &Entry) -> bool {
    light_intact(&game_dir.join(&entry.model))
        && entry
            .images
            .iter()
            .flatten()
            .all(|path| texture_intact(&game_dir.join(path)))
}

fn light_intact(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let declared = bytes
        .get(8..12)
        .and_then(|length| length.try_into().ok())
        .map(u32::from_le_bytes);
    declared.is_some_and(|length| length as usize == bytes.len())
        && gltf::Glb::from_slice(&bytes).is_ok()
}

fn texture_intact(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(length) = file.metadata().map(|meta| meta.len()) else {
        return false;
    };
    let mut head = Vec::new();
    file.take(ktx2_writer::HEAD_LIMIT as u64)
        .read_to_end(&mut head)
        .is_ok()
        && ktx2_writer::is_intact(&head, length)
}

/// Путь облегчённой модели внутри `imported/`: путь исходной модели без `.glb`, первые знаки хеша исходника и
/// `.glb`; новое содержимое исходника — новое имя.
fn light_path(key: &str, source: &str) -> String {
    let stem = key.rsplit_once('.').map_or(key, |(stem, _)| stem);
    format!("{stem}.{}.glb", &source[..SOURCE_PREFIX])
}

pub fn game_name(game_dir: &Path) -> String {
    game_dir
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Все `.glb` игры, кроме папки `imported/`: путь от папки игры через `/` и путь на диске, по алфавиту.
pub fn find_models(game_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    collect_models(game_dir, "", true, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect_models(
    dir: &Path,
    prefix: &str,
    top: bool,
    found: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("папка не читается: {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let key = format!("{prefix}{name}");
        if entry.file_type()?.is_dir() {
            if !(top && name == IMPORTED) {
                collect_models(&entry.path(), &format!("{key}/"), false, found)?;
            }
        } else if name.to_lowercase().ends_with(".glb") {
            found.push((key, entry.path()));
        }
    }
    Ok(())
}

/// Файл пишется во временный рядом и переименовывается: читатель видит либо прежний, либо готовый.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("папка не создаётся: {}", parent.display()))?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes)
        .with_context(|| format!("файл не пишется: {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("файл не переименовывается: {}", path.display()))
}

/// Собирает в `stale` файлы `dir`, которых нет в `keep` (пути от `imported/` через `/`).
fn collect_stale(
    dir: &Path,
    prefix: &str,
    keep: &HashSet<String>,
    stale: &mut Vec<PathBuf>,
) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let key = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            collect_stale(&entry.path(), &format!("{key}/"), keep, stale)?;
        } else if !keep.contains(&key) {
            stale.push(entry.path());
        }
    }
    Ok(())
}

fn remove_empty_folders(dir: &Path) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_empty_folders(&entry.path());
            let _ = fs::remove_dir(entry.path());
        }
    }
}

fn pictures(count: usize) -> String {
    let word = match (count % 100, count % 10) {
        (11..=14, _) => "картинок",
        (_, 1) => "картинка",
        (_, 2..=4) => "картинки",
        _ => "картинок",
    };
    format!("{count} {word}")
}

#[cfg(test)]
mod tests;

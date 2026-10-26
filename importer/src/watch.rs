//! Вызовы `importer once` и `importer watch`: проход по всем играм и опрос папок раз в две секунды.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};

use crate::game::{self, Log, Refused};

const POLL: Duration = Duration::from_secs(2);

type State = (u64, Option<SystemTime>);
type Snapshot = BTreeMap<PathBuf, State>;
type Refusals = BTreeMap<PathBuf, Refused>;
type Stale = BTreeSet<PathBuf>;

pub fn once(root: &Path, log: Log) -> Result<()> {
    let games = games(root)?;
    let mut refusals = Refusals::new();
    let mut stale = Stale::new();
    let failed = games
        .iter()
        .filter(|game| sync(game, log, &mut refusals, &mut stale).is_err())
        .count();
    if failed > 0 {
        bail!("не обработано игр: {failed}");
    }
    Ok(())
}

pub fn watch(root: &Path, log: Log) -> Result<()> {
    let mut seen = snapshot(root)?;
    let mut refusals = Refusals::new();
    let mut stale = Stale::new();
    for game in games(root)? {
        let synced = sync(&game, log, &mut refusals, &mut stale).is_ok();
        settle(&mut seen, &game, synced);
    }
    loop {
        thread::sleep(POLL);
        poll(root, &mut seen, &mut refusals, &mut stale, log);
    }
}

/// Один опрос: игры с изменившимися моделями или файлами `imported/` проходятся заново, игры с удалённой папкой
/// забываются. Игры с ничейными файлами в `imported/` дочищаются, когда прошла минута, даже если ничего не менялось.
fn poll(root: &Path, seen: &mut Snapshot, refusals: &mut Refusals, stale: &mut Stale, log: Log) {
    let mut current = match snapshot(root) {
        Ok(current) => current,
        Err(error) => {
            log(&format!("ошибка опроса: {error:#}"));
            return;
        }
    };
    for game in changed_games(root, seen, &current) {
        if game.is_dir() {
            let synced = sync(&game, log, refusals, stale).is_ok();
            settle(&mut current, &game, synced);
        } else {
            refusals.remove(&game);
            stale.remove(&game);
        }
    }
    clean_stale(stale, log);
    *seen = current;
}

fn clean_stale(stale: &mut Stale, log: Log) {
    for game in stale.clone() {
        if !game.is_dir() {
            stale.remove(&game);
        } else if game::grace_over(&game) {
            match game::clean(&game) {
                Ok(true) => {}
                Ok(false) => {
                    stale.remove(&game);
                }
                Err(error) => log(&format!("ошибка: {} — {error:#}", game::game_name(&game))),
            }
        }
    }
}

/// Состояние игры после прохода. Удачный проход: файлы `imported/` берутся как лежат теперь, чтобы собственная
/// запись службы не вызвала новый проход, а модели остаются в состоянии до прохода — изменённую за время прохода
/// модель опрос увидит. Проход с ошибкой: игра забывается целиком, и следующий опрос повторяет проход.
fn settle(snapshot: &mut Snapshot, game: &Path, synced: bool) {
    if synced {
        let imported = game::imported_dir(game);
        snapshot.retain(|path, _| !path.starts_with(&imported));
        snapshot.extend(imported_state(game));
    } else {
        snapshot.retain(|path, _| !path.starts_with(game));
    }
}

/// Проход по игре; игра с ничейными файлами в `imported/` запоминается в `stale` до их удаления.
fn sync(game: &Path, log: Log, refusals: &mut Refusals, stale: &mut Stale) -> Result<()> {
    let result = game::sync(game, log, refusals.entry(game.to_path_buf()).or_default());
    match &result {
        Ok(true) => {
            stale.insert(game.to_path_buf());
        }
        Ok(false) => {
            stale.remove(game);
        }
        Err(error) => log(&format!("ошибка: {} — {error:#}", game::game_name(game))),
    }
    result.map(|_| ())
}

fn games(root: &Path) -> Result<Vec<PathBuf>> {
    let entries =
        fs::read_dir(root).with_context(|| format!("папка игр не читается: {}", root.display()))?;
    let mut games = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            games.push(entry.path());
        }
    }
    games.sort();
    Ok(games)
}

fn state_of(path: &Path) -> Option<State> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// Размер и время изменения каждого `.glb` каждой игры, её `imported/index.json` и каждого файла, который он
/// называет.
fn snapshot(root: &Path) -> Result<Snapshot> {
    let mut snapshot = Snapshot::new();
    for game in games(root)? {
        for (_, path) in game::find_models(&game)? {
            if let Some(state) = state_of(&path) {
                snapshot.insert(path, state);
            }
        }
        snapshot.extend(imported_state(&game));
    }
    Ok(snapshot)
}

/// То же для `imported/index.json` игры и названных в нём файлов; пропавшего файла в списке нет.
fn imported_state(game: &Path) -> Snapshot {
    std::iter::once(game::index_path(game))
        .chain(game::indexed_files(game))
        .filter_map(|path| Some((path.clone(), state_of(&path)?)))
        .collect()
}

fn changed_games(root: &Path, before: &Snapshot, after: &Snapshot) -> BTreeSet<PathBuf> {
    before
        .iter()
        .filter(|(path, state)| after.get(*path) != Some(*state))
        .map(|(path, _)| path)
        .chain(
            after
                .iter()
                .filter(|(path, state)| before.get(*path) != Some(*state))
                .map(|(path, _)| path),
        )
        .filter_map(|path| {
            let first = path.strip_prefix(root).ok()?.components().next()?;
            Some(root.join(first))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::testing::{Spec, TempDir, material, png};

    fn snapshot_of(files: &[(&str, u64)]) -> Snapshot {
        files
            .iter()
            .map(|(path, size)| (PathBuf::from(path), (*size, None)))
            .collect()
    }

    #[test]
    fn only_games_with_new_changed_or_removed_models_are_named() {
        let root = Path::new("games");
        let before = snapshot_of(&[
            ("games/a/models/x.glb", 1),
            ("games/b/models/y.glb", 2),
            ("games/c/models/z.glb", 3),
            ("games/d/models/w.glb", 4),
        ]);
        let after = snapshot_of(&[
            ("games/a/models/x.glb", 1),
            ("games/b/models/y.glb", 5),
            ("games/d/models/w.glb", 4),
            ("games/e/q.glb", 6),
        ]);
        let changed: Vec<PathBuf> = changed_games(root, &before, &after).into_iter().collect();
        assert_eq!(
            changed,
            [
                Path::new("games/b"),
                Path::new("games/c"),
                Path::new("games/e")
            ]
        );
    }

    #[test]
    fn nothing_changed_names_no_game() {
        let same = snapshot_of(&[("games/a/x.glb", 1)]);
        assert!(changed_games(Path::new("games"), &same, &same).is_empty());
    }

    #[test]
    fn a_vanished_game_folder_is_forgotten_without_an_error_line() {
        let dir = TempDir::new();
        let root = dir.path();
        let models = root.join("a/models");
        fs::create_dir_all(&models).unwrap();
        fs::write(models.join("x.glb"), png_model()).unwrap();

        let mut watch = Watch::default();
        let lines = watch.poll(root);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("сжато: a/models/x.glb"));
        assert_eq!(watch.refusals.len(), 1);

        fs::remove_dir_all(root.join("a")).unwrap();
        let lines = watch.poll(root);
        assert!(lines.is_empty(), "{lines:?}");
        assert!(watch.refusals.is_empty());
        assert!(watch.seen.is_empty());
    }

    #[derive(Default)]
    struct Watch {
        seen: Snapshot,
        refusals: Refusals,
        stale: Stale,
    }

    impl Watch {
        fn poll(&mut self, root: &Path) -> Vec<String> {
            let lines = RefCell::new(Vec::new());
            poll(
                root,
                &mut self.seen,
                &mut self.refusals,
                &mut self.stale,
                &|line: &str| lines.borrow_mut().push(line.to_string()),
            );
            lines.into_inner()
        }
    }

    fn age_index(game: &Path) {
        let index = fs::OpenOptions::new()
            .write(true)
            .open(game::index_path(game))
            .unwrap();
        index
            .set_modified(SystemTime::now() - Duration::from_secs(120))
            .unwrap();
    }

    fn game_with_a_model(root: &Path) -> PathBuf {
        let game = root.join("a");
        fs::create_dir_all(game.join("models")).unwrap();
        fs::write(game.join("models/x.glb"), png_model()).unwrap();
        game
    }

    #[test]
    fn a_vanished_or_changed_file_named_by_the_index_starts_a_pass_over_its_game() {
        let dir = TempDir::new();
        let root = dir.path();
        let game = game_with_a_model(root);
        let mut watch = Watch::default();
        assert_eq!(watch.poll(root).len(), 1);
        assert!(watch.poll(root).is_empty());

        let named = game::indexed_files(&game);
        assert_eq!(named.len(), 2, "the light model and one picture");
        for path in &named {
            assert!(watch.seen.contains_key(path), "{}", path.display());
        }
        assert!(watch.seen.contains_key(&game::index_path(&game)));

        for path in &named {
            let good = fs::read(path).unwrap();
            fs::remove_file(path).unwrap();
            let lines = watch.poll(root);
            assert_eq!(lines.len(), 1, "{}: {lines:?}", path.display());
            assert_eq!(fs::read(path).unwrap(), good, "{}", path.display());
            assert!(watch.poll(root).is_empty());

            fs::write(path, b"damaged").unwrap();
            let lines = watch.poll(root);
            assert_eq!(lines.len(), 1, "{}: {lines:?}", path.display());
            assert_eq!(fs::read(path).unwrap(), good, "{}", path.display());
            assert!(watch.poll(root).is_empty());
        }

        fs::remove_file(game::index_path(&game)).unwrap();
        assert!(watch.poll(root).is_empty(), "restored without a line");
        assert!(game::index_path(&game).is_file());
        assert!(watch.poll(root).is_empty());
    }

    #[test]
    fn a_pass_that_failed_is_not_remembered_and_runs_again_on_the_next_poll() {
        let dir = TempDir::new();
        let root = dir.path();
        let game = game_with_a_model(root);
        fs::create_dir_all(game::index_path(&game)).unwrap();
        let mut watch = Watch::default();

        let lines = watch.poll(root);
        assert!(
            lines.iter().any(|line| line.starts_with("ошибка: a")),
            "{lines:?}"
        );
        assert!(
            watch.seen.keys().all(|path| !path.starts_with(&game)),
            "{:?}",
            watch.seen
        );

        let again = watch.poll(root);
        assert!(
            again.iter().any(|line| line.starts_with("ошибка: a")),
            "{again:?}"
        );

        fs::remove_dir(game::index_path(&game)).unwrap();
        let fixed = watch.poll(root);
        assert!(
            fixed.is_empty(),
            "the files the failed pass wrote are ready: {fixed:?}"
        );
        assert!(game::index_path(&game).is_file());
        assert!(watch.poll(root).is_empty());
    }

    #[test]
    fn files_the_index_dropped_are_removed_after_a_minute_without_any_model_changing() {
        let dir = TempDir::new();
        let root = dir.path();
        let game = game_with_a_model(root);
        let mut watch = Watch::default();
        watch.poll(root);
        let old = game::indexed_files(&game);
        let model = root.join("a/models/x.glb");
        let other = Spec::new(
            vec![png(16, 16, 9)],
            vec![0],
            material(Some(0), None, None, None),
        );
        fs::write(&model, other.build()).unwrap();
        assert_eq!(watch.poll(root).len(), 1);
        assert!(
            old.iter().all(|path| path.is_file()),
            "the old files stay for the page"
        );
        assert!(watch.stale.contains(&game));

        assert!(watch.poll(root).is_empty());
        assert!(
            old.iter().all(|path| path.is_file()),
            "a minute has not passed"
        );

        age_index(&game);
        watch.seen.extend(imported_state(&game));
        assert!(watch.poll(root).is_empty(), "no model changed: no pass");
        assert!(old.iter().all(|path| !path.exists()));
        assert!(game::indexed_files(&game).iter().all(|path| path.is_file()));
        assert!(watch.stale.is_empty());
    }

    fn png_model() -> Vec<u8> {
        let materials = material(Some(0), None, None, None);
        Spec::new(vec![png(16, 16, 3)], vec![0], materials).build()
    }
}

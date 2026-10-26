use std::cell::RefCell;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use super::*;
use crate::testing::{Spec, TempDir, material, png};

struct Fixture {
    dir: TempDir,
    messages: RefCell<Vec<String>>,
    refused: RefCell<Refused>,
}

impl Fixture {
    fn new() -> Fixture {
        let fixture = Fixture {
            dir: TempDir::new(),
            messages: RefCell::new(Vec::new()),
            refused: RefCell::new(Refused::new()),
        };
        fs::create_dir_all(fixture.game().join("models")).unwrap();
        fixture
    }

    fn game(&self) -> PathBuf {
        self.dir.path().join("game")
    }

    fn put(&self, name: &str, bytes: &[u8]) {
        fs::write(self.game().join("models").join(name), bytes).unwrap();
    }

    fn run(&self) -> Vec<String> {
        self.messages.borrow_mut().clear();
        let log = |line: &str| self.messages.borrow_mut().push(line.to_string());
        sync(&self.game(), &log, &mut self.refused.borrow_mut()).unwrap();
        self.messages.borrow().clone()
    }

    fn index(&self) -> Index {
        let bytes = fs::read(self.game().join("imported/index.json")).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn files(&self, folder: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.game().join("imported").join(folder)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn snapshot(&self) -> Vec<(String, u64, Option<SystemTime>)> {
        let mut all = Vec::new();
        let mut stack = vec![self.game().join("imported")];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                let meta = entry.metadata().unwrap();
                if meta.is_dir() {
                    stack.push(entry.path());
                } else {
                    let name = entry.path().display().to_string();
                    all.push((name, meta.len(), meta.modified().ok()));
                }
            }
        }
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    }

    /// Делает вид, что `index.json` записан две минуты назад.
    fn age_index(&self) {
        fs::OpenOptions::new()
            .write(true)
            .open(self.game().join("imported/index.json"))
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(120))
            .unwrap();
    }

    fn light_name(entry: &Entry) -> String {
        entry
            .model
            .rsplit('/')
            .next()
            .expect("a path has a last part")
            .to_string()
    }

    fn vk_format(&self, image: &str) -> u32 {
        let bytes = fs::read(self.game().join(image)).unwrap();
        u32::from_le_bytes(bytes[12..16].try_into().unwrap())
    }
}

fn model(pictures: Vec<Vec<u8>>, materials: Value) -> Vec<u8> {
    let textures = (0..pictures.len()).collect();
    Spec::new(pictures, textures, materials).build()
}

fn coloured(picture: Vec<u8>) -> Vec<u8> {
    model(vec![picture], material(Some(0), None, None, None))
}

#[test]
fn two_models_with_one_picture_share_one_file() {
    let fixture = Fixture::new();
    let board = png(16, 16, 40);
    fixture.put("a.glb", &coloured(board.clone()));
    fixture.put("b.glb", &coloured(board));
    let messages = fixture.run();

    let index = fixture.index();
    let a = &index.models["models/a.glb"];
    let b = &index.models["models/b.glb"];
    assert_eq!(a.images, b.images);
    assert_eq!(fixture.files("textures").len(), 1);
    assert_eq!(a.model, format!("imported/models/a.{}.glb", &a.source[..8]));
    assert_ne!(a.model, b.model);
    assert!(
        messages[0].starts_with("сжато: game/models/a.glb — 1 картинка, новых 1, "),
        "{messages:?}"
    );
    assert!(messages[1].contains("новых 0"), "{messages:?}");
}

#[test]
fn formats_follow_roles_and_sides() {
    let fixture = Fixture::new();
    let pictures = vec![
        png(16, 16, 1),
        png(16, 16, 2),
        png(16, 16, 3),
        png(10, 8, 4),
    ];
    let materials = json!([
        {
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0},
                "metallicRoughnessTexture": {"index": 1}
            },
            "normalTexture": {"index": 2},
            "occlusionTexture": {"index": 1}
        },
        {"pbrMetallicRoughness": {"baseColorTexture": {"index": 3}}}
    ]);
    fixture.put("a.glb", &model(pictures, materials));
    fixture.run();

    let images = &fixture.index().models["models/a.glb"].images;
    let formats: Vec<u32> = images
        .iter()
        .map(|image| fixture.vk_format(image.as_ref().unwrap()))
        .collect();
    assert_eq!(formats, [146, 145, 141, 43]);
}

#[test]
fn picture_in_two_roles_gives_two_files() {
    let fixture = Fixture::new();
    let materials = material(Some(0), None, Some(0), None);
    fixture.put("a.glb", &model(vec![png(16, 16, 9)], materials));
    fixture.run();
    let entry = fixture.index().models["models/a.glb"].clone();
    assert_eq!(entry.images.len(), 2);
    assert_eq!(fixture.files("textures").len(), 2);
}

#[test]
fn second_pass_without_changes_writes_nothing() {
    let fixture = Fixture::new();
    fixture.put("a.glb", &coloured(png(16, 16, 5)));
    fixture.run();
    let before = fixture.snapshot();
    let messages = fixture.run();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn changed_model_is_compressed_again_and_its_lost_picture_removed() {
    let fixture = Fixture::new();
    let shared = png(16, 16, 5);
    fixture.put("a.glb", &coloured(png(16, 16, 6)));
    fixture.put("b.glb", &coloured(shared.clone()));
    fixture.run();
    let before = fixture.index().models["models/a.glb"].clone();
    let old = before.images[0].clone().unwrap();

    fixture.put("a.glb", &coloured(shared));
    let messages = fixture.run();
    assert_eq!(messages.len(), 1);
    assert!(
        messages[0].starts_with("сжато: game/models/a.glb"),
        "{messages:?}"
    );
    let index = fixture.index();
    assert_ne!(
        index.models["models/a.glb"].images[0].as_ref().unwrap(),
        &old
    );
    assert_ne!(index.models["models/a.glb"].model, before.model);
    assert!(
        fixture
            .game()
            .join(&index.models["models/a.glb"].model)
            .is_file()
    );
    assert!(
        fixture.game().join(&old).is_file() && fixture.game().join(&before.model).is_file(),
        "a page that read the old index still downloads these files"
    );

    fixture.age_index();
    assert!(fixture.run().is_empty());
    assert!(!fixture.game().join(&old).exists());
    assert!(!fixture.game().join(&before.model).exists());
    assert_eq!(fixture.files("textures").len(), 1);
}

#[test]
fn removed_model_leaves_the_index_but_shared_pictures_stay() {
    let fixture = Fixture::new();
    let shared = png(16, 16, 5);
    fixture.put("a.glb", &coloured(shared.clone()));
    let pictures = vec![shared, png(16, 16, 7)];
    fixture.put(
        "b.glb",
        &model(pictures, material(Some(0), Some(1), None, None)),
    );
    fixture.run();
    assert_eq!(fixture.files("textures").len(), 2);

    fs::remove_file(fixture.game().join("models/b.glb")).unwrap();
    let messages = fixture.run();
    assert_eq!(messages, ["удалено: game/models/b.glb"]);
    let index = fixture.index();
    assert_eq!(index.models.len(), 1);
    assert_eq!(fixture.files("models").len(), 2);
    assert_eq!(fixture.files("textures").len(), 2);

    fixture.age_index();
    fixture.run();
    assert_eq!(
        fixture.files("models"),
        [Fixture::light_name(&index.models["models/a.glb"])]
    );
    assert_eq!(fixture.files("textures").len(), 1);
}

#[test]
fn broken_and_draco_models_are_not_compressed() {
    let fixture = Fixture::new();
    fixture.put("good.glb", &coloured(png(16, 16, 5)));
    fixture.put("broken.glb", b"definitely not a model");
    let mut draco = Spec::new(Vec::new(), Vec::new(), Value::Null);
    draco.extensions_used = vec!["KHR_draco_mesh_compression"];
    fixture.put("draco.glb", &draco.build());
    let messages = fixture.run();

    let keys: Vec<String> = fixture.index().models.into_keys().collect();
    assert_eq!(keys, ["models/good.glb"]);
    for name in ["broken", "draco"] {
        let prefix = format!("не сжато: game/models/{name}.glb — ");
        assert!(
            messages.iter().any(|m| m.starts_with(&prefix)),
            "{messages:?}"
        );
    }
    let index = fixture.index();
    assert_eq!(
        fixture.files("models"),
        [Fixture::light_name(&index.models["models/good.glb"])]
    );
}

#[test]
fn a_refusal_is_logged_once_until_its_reason_changes() {
    let fixture = Fixture::new();
    fixture.put("good.glb", &coloured(png(16, 16, 5)));
    fixture.put("junk.glb", b"definitely not a model");
    let first = fixture.run();
    assert!(
        first
            .iter()
            .any(|m| m.starts_with("не сжато: game/models/junk.glb")),
        "{first:?}"
    );

    fixture.put("good.glb", &coloured(png(16, 16, 6)));
    let second = fixture.run();
    assert_eq!(second.len(), 1, "{second:?}");
    assert!(
        second[0].starts_with("сжато: game/models/good.glb"),
        "{second:?}"
    );

    let mut draco = Spec::new(Vec::new(), Vec::new(), Value::Null);
    draco.extensions_used = vec!["KHR_draco_mesh_compression"];
    fixture.put("junk.glb", &draco.build());
    let changed = fixture.run();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(
        changed[0].starts_with("не сжато: game/models/junk.glb"),
        "{changed:?}"
    );

    fixture.put("junk.glb", &coloured(png(16, 16, 7)));
    fixture.run();
    fs::write(fixture.game().join("models/junk.glb"), b"broken again").unwrap();
    let again = fixture.run();
    assert!(
        again
            .iter()
            .any(|m| m.starts_with("не сжато: game/models/junk.glb")),
        "{again:?}"
    );
}

#[test]
fn an_io_error_fails_the_whole_pass_instead_of_refusing_the_model() {
    let fixture = Fixture::new();
    fixture.put("good.glb", &coloured(png(16, 16, 5)));
    fixture.put("junk.glb", b"definitely not a model");
    let blocker = fixture.game().join("imported/textures");
    fs::create_dir_all(blocker.parent().unwrap()).unwrap();
    fs::write(&blocker, b"a file where the folder belongs").unwrap();

    let lines = RefCell::new(Vec::new());
    let log = |line: &str| lines.borrow_mut().push(line.to_string());
    let result = sync(&fixture.game(), &log, &mut fixture.refused.borrow_mut());
    assert!(result.is_err());
    assert!(
        !lines.borrow().iter().any(|line| line.contains("good.glb")),
        "{:?}",
        lines.borrow()
    );
    assert!(
        !fixture.game().join("imported/index.json").exists(),
        "a failed pass leaves the index alone"
    );

    fs::remove_file(&blocker).unwrap();
    let messages = fixture.run();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("сжато: game/models/good.glb")),
        "{messages:?}"
    );
    assert!(fixture.index().models.contains_key("models/good.glb"));
}

#[test]
fn model_that_became_broken_leaves_the_index() {
    let fixture = Fixture::new();
    fixture.put("a.glb", &coloured(png(16, 16, 5)));
    fixture.run();
    fixture.put("a.glb", b"broken now");
    fixture.run();
    fixture.age_index();
    fixture.run();
    assert!(fixture.index().models.is_empty());
    assert!(fixture.files("models").is_empty());
    assert!(fixture.files("textures").is_empty());
}

#[test]
fn index_is_written_last() {
    let fixture = Fixture::new();
    fixture.put("a.glb", &coloured(png(16, 16, 5)));
    fixture.run();
    let index_path = fixture.game().join("imported/index.json");
    let before = fs::read(&index_path).unwrap();
    let old = fixture.index();
    let old_light = &old.models["models/a.glb"].model;
    let old_light_bytes = fs::read(fixture.game().join(old_light)).unwrap();

    fixture.put("a.glb", &coloured(png(16, 16, 6)));
    fixture.put("b.glb", &coloured(png(16, 16, 8)));
    let pending = compress_changed(&fixture.game(), &|_| {}, &mut Refused::new()).unwrap();
    assert_eq!(fs::read(&index_path).unwrap(), before);
    assert!(
        old.models
            .values()
            .all(|entry| files_intact(&fixture.game(), entry))
    );
    assert_eq!(
        fs::read(fixture.game().join(old_light)).unwrap(),
        old_light_bytes,
        "the light model the old index names keeps its place and bytes"
    );
    assert_eq!(fixture.files("textures").len(), 3);
    assert_eq!(fixture.files("models").len(), 3);

    assert!(
        commit(pending).unwrap(),
        "the old files wait out the minute"
    );
    assert_ne!(fs::read(&index_path).unwrap(), before);
    assert_eq!(fixture.files("textures").len(), 3);
    assert_eq!(fixture.files("models").len(), 3);
    assert!(fixture.game().join(old_light).is_file());
    assert!(!grace_over(&fixture.game()));
    assert!(clean(&fixture.game()).unwrap());

    fixture.age_index();
    assert!(grace_over(&fixture.game()));
    assert!(!clean(&fixture.game()).unwrap());
    assert_eq!(fixture.files("textures").len(), 2);
    assert_eq!(fixture.files("models").len(), 2);
    assert!(!fixture.game().join(old_light).exists());
}

#[test]
fn a_persistent_io_error_prints_each_line_once_and_refuses_in_memory() {
    let fixture = Fixture::new();
    fixture.put("a.glb", &coloured(png(16, 16, 5)));
    fixture.put("b.glb", b"definitely not a model");
    let last = Spec::new(Vec::new(), Vec::new(), Value::Null).build();
    fixture.put("z.glb", &last);
    let source = texture::hex(&Sha256::digest(&last));
    let blocker = fixture
        .game()
        .join(format!("imported/models/z.{}.glb", &source[..8]));
    fs::create_dir_all(&blocker).unwrap();

    let lines = RefCell::new(Vec::new());
    let log = |line: &str| lines.borrow_mut().push(line.to_string());
    let mut refused = Refused::new();
    assert!(sync(&fixture.game(), &log, &mut refused).is_err());
    assert_eq!(lines.borrow().len(), 2, "{:?}", lines.borrow());
    assert!(lines.borrow()[0].starts_with("сжато: game/models/a.glb"));
    assert!(lines.borrow()[1].starts_with("не сжато: game/models/b.glb"));
    assert!(refused.contains_key("models/b.glb"));

    lines.borrow_mut().clear();
    assert!(sync(&fixture.game(), &log, &mut refused).is_err());
    assert!(lines.borrow().is_empty(), "{:?}", lines.borrow());
}

#[test]
fn game_without_models_gets_no_imported_folder() {
    let fixture = Fixture::new();
    fixture.run();
    assert!(!fixture.game().join("imported").exists());
}

#[test]
fn source_models_are_never_touched() {
    let fixture = Fixture::new();
    let source = coloured(png(16, 16, 5));
    fixture.put("a.glb", &source);
    fixture.run();
    assert_eq!(
        fs::read(fixture.game().join("models/a.glb")).unwrap(),
        source
    );
}

#[test]
fn models_in_the_game_root_and_in_models_get_their_own_light_files() {
    let fixture = Fixture::new();
    fs::write(fixture.game().join("a.glb"), coloured(png(16, 16, 1))).unwrap();
    fixture.put("a.glb", &coloured(png(16, 16, 2)));
    let messages = fixture.run();
    let index = fixture.index();
    assert_eq!(index.models.len(), 2, "{messages:?}");
    assert!(index.models["a.glb"].model.starts_with("imported/a."));
    assert!(
        index.models["models/a.glb"]
            .model
            .starts_with("imported/models/a.")
    );
    assert!(
        index
            .models
            .values()
            .all(|entry| fixture.game().join(&entry.model).is_file())
    );
}

/// Испорченные виды файла `good`: подпись и новое содержимое (`None` — файл удалён).
fn damages(good: &[u8], extra: &[u8]) -> Vec<(&'static str, Option<Vec<u8>>)> {
    vec![
        ("truncated", Some(good[..good.len() - extra.len()].to_vec())),
        ("too long", Some([good, extra].concat())),
        ("garbage", Some(b"garbage".to_vec())),
        ("missing", None),
    ]
}

fn damage(path: &Path, content: Option<Vec<u8>>) {
    match content {
        Some(bytes) => fs::write(path, bytes).unwrap(),
        None => fs::remove_file(path).unwrap(),
    }
}

#[test]
fn a_damaged_or_missing_compressed_file_is_written_again() {
    let fixture = Fixture::new();
    fixture.put("a.glb", &coloured(png(16, 16, 5)));
    fixture.run();
    let image = fixture.index().models["models/a.glb"].images[0]
        .clone()
        .unwrap();
    let path = fixture.game().join(&image);
    let good = fs::read(&path).unwrap();

    for (what, content) in damages(&good, b"x") {
        damage(&path, content);
        let messages = fixture.run();
        assert_eq!(messages.len(), 1, "{what}: {messages:?}");
        assert!(messages[0].contains("новых 1"), "{what}: {messages:?}");
        assert_eq!(fs::read(&path).unwrap(), good, "{what}");
        assert!(fixture.run().is_empty(), "{what}");
    }
}

#[test]
fn a_damaged_or_missing_light_model_is_written_again() {
    let fixture = Fixture::new();
    fixture.put("a.glb", &coloured(png(16, 16, 5)));
    fixture.run();
    let entry = fixture.index().models["models/a.glb"].clone();
    let path = fixture.game().join(&entry.model);
    let good = fs::read(&path).unwrap();

    for (what, content) in damages(&good, b"xxxx") {
        damage(&path, content);
        let messages = fixture.run();
        assert_eq!(messages.len(), 1, "{what}: {messages:?}");
        assert!(messages[0].contains("новых 0"), "{what}: {messages:?}");
        assert_eq!(fs::read(&path).unwrap(), good, "{what}");
        assert_eq!(fixture.index().models["models/a.glb"], entry, "{what}");
        assert!(fixture.run().is_empty(), "{what}");
    }
}

#[test]
fn picture_words_follow_russian_plurals() {
    let words: Vec<String> = [1, 2, 5, 11, 21, 12].map(pictures).into();
    assert_eq!(
        words,
        [
            "1 картинка",
            "2 картинки",
            "5 картинок",
            "11 картинок",
            "21 картинка",
            "12 картинок"
        ]
    );
}

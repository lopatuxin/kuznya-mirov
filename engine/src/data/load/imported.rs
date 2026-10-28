//! «Трёхмерная сцена» → «Загрузка и проверка»: сжатые модели службы сжатия. `imported/index.json` игры называет
//! для каждой модели облегчённый файл без картинок и сжатые файлы её картинок; здесь решается, что читать вместо
//! исходного `.glb`, и разбираются сжатые файлы, по одному разу на игру.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::core::model::PackedImage;
use crate::data::ktx2::parse_ktx2;

use super::ModelDecl;

const INDEX_VERSION: u32 = 1;

#[derive(Deserialize)]
struct IndexFile {
    version: u32,
    models: HashMap<String, IndexEntry>,
}

#[derive(Deserialize)]
struct IndexEntry {
    model: String,
    images: Vec<Option<String>>,
}

/// Какие модели игры читать сжатыми: по каждой модели `files.models` путь облегчённого файла, если в
/// `imported/index.json` есть её запись, и пути сжатых файлов всех таких моделей — каждый один раз.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImportedPlan {
    models: Vec<Option<String>>,
    textures: Vec<String>,
}

impl ImportedPlan {
    /// Текст `index.json`, которого нет или который не разбирается, ничего не сжимает: все модели читаются из
    /// исходных файлов.
    pub fn new(decls: &[ModelDecl], index_text: Option<&str>) -> ImportedPlan {
        let index = index_text
            .and_then(|text| serde_json::from_str::<IndexFile>(text).ok())
            .filter(|index| index.version == INDEX_VERSION);
        let mut plan = ImportedPlan::default();
        for decl in decls {
            let entry = index
                .as_ref()
                .and_then(|index| index.models.get(&decl.path));
            plan.models.push(entry.map(|entry| entry.model.clone()));
            for path in entry
                .into_iter()
                .flat_map(|entry| entry.images.iter().flatten())
            {
                if !plan.textures.contains(path) {
                    plan.textures.push(path.clone());
                }
            }
        }
        plan
    }

    /// Облегчённая модель `files.models[decl]`, если она сжата.
    pub fn model_path(&self, decl: usize) -> Option<&str> {
        self.models.get(decl)?.as_deref()
    }

    /// Пути сжатых файлов всех сжатых моделей игры по порядку первого упоминания.
    pub fn textures(&self) -> &[String] {
        &self.textures
    }
}

/// Путь от папки игры, на который указывает `uri` облегчённой модели `model_path`.
pub(super) fn resolve_uri(model_path: &str, uri: &str) -> String {
    let mut segments: Vec<&str> = model_path.split('/').collect();
    segments.pop();
    for part in uri.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// Чем кончилась попытка взять сжатый файл.
pub(super) enum PackedFault {
    Missing,
    Broken(String),
}

/// Сжатые файлы, которые прочитала страница, разобранные по первому требованию: один файл — один `Arc` на все
/// модели игры.
pub(super) struct PackedFiles<'a> {
    files: &'a [(String, Option<Vec<u8>>)],
    parsed: HashMap<&'a str, Result<Arc<PackedImage>, String>>,
}

impl<'a> PackedFiles<'a> {
    pub(super) fn new(files: &'a [(String, Option<Vec<u8>>)]) -> PackedFiles<'a> {
        PackedFiles {
            files,
            parsed: HashMap::new(),
        }
    }

    pub(super) fn get(&mut self, path: &str) -> Result<Arc<PackedImage>, PackedFault> {
        let Some((known, bytes)) = self.files.iter().find(|(known, _)| known == path) else {
            return Err(PackedFault::Missing);
        };
        let Some(bytes) = bytes.as_deref().filter(|bytes| !bytes.is_empty()) else {
            return Err(PackedFault::Missing);
        };
        self.parsed
            .entry(known.as_str())
            .or_insert_with(|| parse_ktx2(bytes).map(Arc::new))
            .clone()
            .map_err(PackedFault::Broken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Look, PackedFormat};
    use crate::data::ktx2::write_ktx2;

    fn decl(name: &str, path: &str) -> ModelDecl {
        ModelDecl {
            name: name.to_string(),
            path: path.to_string(),
            look: Look::Lit,
            solid: None,
            cutaway: None,
            idle: None,
            walk: None,
            feet: None,
            spine: None,
            hands: None,
        }
    }

    const INDEX: &str = r#"{"version":1,"models":{
        "models/a.glb":{"source":"x","model":"imported/models/a.glb","images":["imported/textures/1.ktx2",null,"imported/textures/2.ktx2"]},
        "models/b.glb":{"source":"y","model":"imported/models/b.glb","images":["imported/textures/2.ktx2"]}}}"#;

    #[test]
    fn a_model_with_an_entry_reads_its_light_file_and_each_packed_file_once() {
        let decls = [
            decl("a", "models/a.glb"),
            decl("c", "models/c.glb"),
            decl("b", "models/b.glb"),
        ];
        let plan = ImportedPlan::new(&decls, Some(INDEX));
        assert_eq!(plan.model_path(0), Some("imported/models/a.glb"));
        assert_eq!(plan.model_path(1), None, "записи нет — исходный файл");
        assert_eq!(plan.model_path(2), Some("imported/models/b.glb"));
        assert_eq!(plan.model_path(3), None);
        assert_eq!(
            plan.textures(),
            ["imported/textures/1.ktx2", "imported/textures/2.ktx2"]
        );
    }

    #[test]
    fn no_index_or_one_that_does_not_read_compresses_nothing() {
        let decls = [decl("a", "models/a.glb")];
        for text in [
            None,
            Some("{ not json"),
            Some(r#"{"version":2,"models":{}}"#),
            Some(r#"{"version":1}"#),
        ] {
            let plan = ImportedPlan::new(&decls, text);
            assert_eq!(plan.model_path(0), None, "{text:?}");
            assert!(plan.textures().is_empty());
        }
    }

    #[test]
    fn a_uri_is_resolved_from_the_folder_of_the_light_model() {
        assert_eq!(
            resolve_uri("imported/models/a.glb", "../textures/x.ktx2"),
            "imported/textures/x.ktx2"
        );
        assert_eq!(
            resolve_uri("imported/models/foo/a.glb", "../../textures/x.ktx2"),
            "imported/textures/x.ktx2"
        );
        assert_eq!(
            resolve_uri("imported/models/a.glb", "./x.ktx2"),
            "imported/models/x.ktx2"
        );
    }

    #[test]
    fn a_missing_empty_or_broken_file_is_a_fault_and_a_good_one_is_parsed_once() {
        let good = write_ktx2(145, 4, 4, &[vec![0; 16], vec![0; 16], vec![0; 16]]);
        let files = vec![
            ("good".to_string(), Some(good)),
            ("empty".to_string(), Some(Vec::new())),
            ("absent".to_string(), None),
            ("broken".to_string(), Some(vec![1, 2, 3])),
        ];
        let mut packed = PackedFiles::new(&files);
        assert!(matches!(packed.get("nowhere"), Err(PackedFault::Missing)));
        assert!(matches!(packed.get("empty"), Err(PackedFault::Missing)));
        assert!(matches!(packed.get("absent"), Err(PackedFault::Missing)));
        assert!(matches!(packed.get("broken"), Err(PackedFault::Broken(_))));
        let first = packed
            .get("good")
            .unwrap_or_else(|_| panic!("файл читается"));
        let second = packed
            .get("good")
            .unwrap_or_else(|_| panic!("файл читается"));
        assert_eq!(first.format, PackedFormat::Bc7);
        assert!(Arc::ptr_eq(&first, &second));
    }
}

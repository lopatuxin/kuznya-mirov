//! Фаза 51 — сжатые картинки моделей. Игры собираются в коде теста, модели — маленькие `.glb`, собранные тут же:
//! исходные, с картинками внутри, и облегчённые, как их пишет служба сжатия, с `uri` на файлы KTX2. Проверяются
//! чтение `imported/index.json` в `read_texts`, сборка сжатой модели в те же части и материалы, один файл на
//! несколько моделей, ошибки сжатых файлов, предупреждение о несжатой модели и вычисление Z мелкого рельефа.

use std::sync::Arc;

use engine::core::game::Game;
use engine::core::model::{Model, ModelImage, PackedFormat};
use engine::data::error::GameError;
use engine::data::load::{
    ImageVerdict, ModelFile, NeededMedia, load_rest_with_imports, read_entry, read_texts,
    read_texts_with_imported,
};
use engine::render::models::ImageSlots;

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag"}}"#;
const COLOR_FILE: &str = "imported/textures/00000000000000000000000000000001.ktx2";
const NORMAL_FILE: &str = "imported/textures/00000000000000000000000000000002.ktx2";
const NO_MATERIALS: &str = "";

fn game_json(models: &[&str]) -> String {
    let declared: Vec<String> = models
        .iter()
        .map(|name| format!(r#""{name}":{{"path":"models/{name}.glb"}}"#))
        .collect();
    format!(
        r##"{{"name":"M","scene":{{"width":40,"height":30,"background":"#000000","view_height":16,"camera":{{"pitch":55}}}},
"random_seed":1,"max_objects":64,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"models":{{{}}}}}}}"##,
        declared.join(",")
    )
}

/// `.glb` из одного треугольника; `images` — текст списка `images`, `materials` — материалы и их карты (пусто —
/// без материала). Картинок под номерами в `textures` столько же, сколько в `images`.
fn glb(images: &str, textures: usize, materials: &str) -> Vec<u8> {
    let positions: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let material = if materials.is_empty() {
        String::new()
    } else {
        r#","material":0"#.to_string()
    };
    let textures_json: Vec<String> = (0..textures)
        .map(|k| format!(r#"{{"source":{k}}}"#))
        .collect();
    let tail = if materials.is_empty() {
        String::new()
    } else {
        format!(
            r#","materials":[{materials}],"textures":[{}],"images":[{images}]"#,
            textures_json.join(",")
        )
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}{material}}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"buffers":[{{"byteLength":36}}]{tail}}}"#
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + positions.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((json.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(json);
    file.extend((positions.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(positions);
    file
}

const TWO_MAPS: &str = r#"{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}},"normalTexture":{"index":1,"scale":0.5}}"#;

/// Исходная модель: три картинки внутри файла, на две из них ссылается материал.
fn source_model() -> Vec<u8> {
    let image = r#"{"bufferView":0,"mimeType":"image/png"}"#;
    glb(&[image; 3].join(","), 2, TWO_MAPS)
}

/// Облегчённая модель той же формы: картинки — файлы рядом, третья никому не нужна и без `uri` на файл.
fn light_model() -> Vec<u8> {
    let images = [
        r#"{"uri":"../textures/00000000000000000000000000000001.ktx2","mimeType":"image/ktx2"}"#,
        r#"{"uri":"../textures/00000000000000000000000000000002.ktx2","mimeType":"image/ktx2"}"#,
        r#"{"uri":""}"#,
    ];
    glb(&images.join(","), 2, TWO_MAPS)
}

/// Файл KTX2 с ступенями `levels`, как его пишет служба сжатия: без сжатия поверх, одна грань, без kvd.
fn ktx2(vk_format: u32, width: u32, height: u32, levels: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![
        0xAB, b'K', b'T', b'X', b' ', b'2', b'0', 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
    ];
    for value in [vk_format, 1, width, height, 0, 0, 1, levels.len() as u32, 0] {
        out.extend(value.to_le_bytes());
    }
    out.extend([0u8; 32]);
    let first = 80 + levels.len() * 24;
    let mut index = Vec::new();
    let mut data = Vec::new();
    for level in levels {
        let offset = (first + data.len()).next_multiple_of(16);
        data.resize(offset - first, 0);
        for value in [offset, level.len(), level.len()] {
            index.extend((value as u64).to_le_bytes());
        }
        data.extend(level);
    }
    out.extend(index);
    out.extend(data);
    out
}

/// Файл BC7 4 × 4 с цветом: три ступени, каждая — один блок.
fn color_file() -> Vec<u8> {
    ktx2(146, 4, 4, &[vec![1; 16], vec![2; 16], vec![3; 16]])
}

/// Файл BC5 4 × 4 с картой мелкого рельефа.
fn normal_file() -> Vec<u8> {
    ktx2(141, 4, 4, &[vec![4; 16], vec![5; 16], vec![6; 16]])
}

fn textures() -> Vec<(String, Option<Vec<u8>>)> {
    vec![
        (COLOR_FILE.to_string(), Some(color_file())),
        (NORMAL_FILE.to_string(), Some(normal_file())),
    ]
}

fn index_for(models: &[&str]) -> String {
    let entries: Vec<String> = models
        .iter()
        .map(|name| {
            format!(
                r#""models/{name}.glb":{{"source":"x","model":"imported/models/{name}.glb","images":["{COLOR_FILE}","{NORMAL_FILE}",null]}}"#
            )
        })
        .collect();
    format!(r#"{{"version":1,"models":{{{}}}}}"#, entries.join(","))
}

/// Ответ страницы по исходной модели: файл и три разжатых картинки.
fn page_source(bytes: Vec<u8>) -> ModelFile {
    ModelFile {
        bytes: Some(bytes),
        images: (0..3)
            .map(|index| {
                (
                    index,
                    ImageVerdict::Ok {
                        width: 4,
                        height: 4,
                        pixels: vec![200; 64],
                    },
                )
            })
            .collect(),
    }
}

/// Ответ страницы по облегчённой модели: только файл, картинок в нём нет.
fn page_light(bytes: Vec<u8>) -> ModelFile {
    ModelFile {
        bytes: Some(bytes),
        images: Vec::new(),
    }
}

struct Run<'a> {
    models: &'a [&'a str],
    files: Vec<(String, ModelFile)>,
    index: Option<String>,
    textures: Vec<(String, Option<Vec<u8>>)>,
    skip_media: bool,
}

impl Run<'_> {
    fn load(&self) -> Result<(Game, Vec<GameError>), Vec<GameError>> {
        let text = game_json(self.models);
        let (config, _) = read_entry(&text).map_err(|failure| failure.errors)?;
        load_rest_with_imports(
            &text,
            config,
            Some(PROPS),
            Some(r#"{"objects":[]}"#),
            Some(r#"{"rules":[]}"#),
            Some(SCREENS),
            &[],
            &[],
            &[],
            &[],
            None,
            self.skip_media,
            &[],
            None,
            &[],
            &[],
            &[],
            &self.files,
            self.index.as_deref(),
            &self.textures,
        )
        .map(|(game, _, warnings, _)| (game, warnings))
        .map_err(|failure| failure.errors)
    }

    fn game(&self) -> (Game, Vec<GameError>) {
        self.load()
            .unwrap_or_else(|errors| panic!("игра не загрузилась: {errors:#?}"))
    }
}

fn compressed_run<'a>(models: &'a [&'a str]) -> Run<'a> {
    Run {
        models,
        files: models
            .iter()
            .map(|name| (name.to_string(), page_light(light_model())))
            .collect(),
        index: Some(index_for(models)),
        textures: textures(),
        skip_media: false,
    }
}

fn source_run<'a>(models: &'a [&'a str]) -> Run<'a> {
    Run {
        models,
        files: models
            .iter()
            .map(|name| (name.to_string(), page_source(source_model())))
            .collect(),
        index: None,
        textures: Vec::new(),
        skip_media: false,
    }
}

fn model_of<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель есть")
}

fn unpacked_warnings(warnings: &[GameError]) -> Vec<&GameError> {
    warnings
        .iter()
        .filter(|warning| warning.message.contains("не сжата"))
        .collect()
}

#[test]
fn a_compressed_model_is_the_same_parts_and_materials_as_the_source_one() {
    let (from_source, _) = source_run(&["a"]).game();
    let (from_light, _) = compressed_run(&["a"]).game();
    let (source, light) = (model_of(&from_source, "a"), model_of(&from_light, "a"));
    assert_eq!(light.parts, source.parts);
    assert_eq!(light.materials, source.materials);
    assert_eq!((light.size, light.origin), (source.size, source.origin));
    assert_eq!(
        light.images.len(),
        2,
        "третья картинка без ссылки не берётся"
    );
    assert!(
        source
            .images
            .iter()
            .all(|image| matches!(image, ModelImage::Pixels(_)))
    );
    let formats: Vec<PackedFormat> = light
        .images
        .iter()
        .map(|image| match image {
            ModelImage::Packed(file) => file.format,
            ModelImage::Pixels(_) => panic!("у облегчённой модели картинки сжатые"),
        })
        .collect();
    assert_eq!(formats, [PackedFormat::Bc7Srgb, PackedFormat::Bc5]);
}

#[test]
fn one_file_named_by_three_models_is_one_image_and_one_texture() {
    let names = ["a", "b", "c"];
    let (game, _) = compressed_run(&names).game();
    let file_of = |name: &str, image: usize| match &model_of(&game, name).images[image] {
        ModelImage::Packed(file) => Arc::clone(file),
        ModelImage::Pixels(_) => panic!("картинка не сжата"),
    };
    for image in 0..2 {
        assert!(Arc::ptr_eq(&file_of("a", image), &file_of("b", image)));
        assert!(Arc::ptr_eq(&file_of("a", image), &file_of("c", image)));
    }
    let mut slots = ImageSlots::default();
    let mut created = 0;
    for name in names {
        for (image, srgb) in [(0, true), (1, false)] {
            let (_, new) = slots.slot(&model_of(&game, name).images[image], srgb);
            created += usize::from(new);
        }
    }
    assert_eq!(created, 2, "шесть ссылок — две текстуры");
}

#[test]
fn equal_page_pixels_of_two_uncompressed_models_are_one_texture() {
    let (game, _) = source_run(&["a", "b"]).game();
    let mut slots = ImageSlots::default();
    let mut created = 0;
    for name in ["a", "b"] {
        for (image, srgb) in [(0, true), (1, false)] {
            let (_, new) = slots.slot(&model_of(&game, name).images[image], srgb);
            created += usize::from(new);
        }
    }
    assert_eq!(
        created, 2,
        "у обеих моделей одинаковые точки в тех же ролях"
    );
}

#[test]
fn only_a_model_loaded_from_its_source_file_with_images_is_warned_about() {
    let (_, warnings) = source_run(&["a"]).game();
    let found = unpacked_warnings(&warnings);
    assert_eq!(found.len(), 1, "{warnings:#?}");
    assert_eq!(
        found[0].message,
        "модель a не сжата: её картинки занимают на видеокарте в 4 раза больше; служба сжатия стенда сожмёт её сама"
    );
    assert_eq!(found[0].path, "files → models → a");

    let (_, warnings) = compressed_run(&["a"]).game();
    assert!(unpacked_warnings(&warnings).is_empty(), "{warnings:#?}");

    let bare = Run {
        models: &["a"],
        files: vec![("a".to_string(), page_source(glb("", 0, NO_MATERIALS)))],
        index: None,
        textures: Vec::new(),
        skip_media: false,
    };
    let (_, warnings) = bare.game();
    assert!(
        unpacked_warnings(&warnings).is_empty(),
        "картинок нет — сжимать нечего"
    );
}

#[test]
fn no_warning_is_written_while_the_media_files_are_not_checked() {
    let mut run = source_run(&["a"]);
    run.skip_media = true;
    let (_, warnings) = run.game();
    assert!(unpacked_warnings(&warnings).is_empty(), "{warnings:#?}");
}

#[test]
fn a_model_without_an_entry_in_the_index_is_read_from_its_source_and_warned_about() {
    let mut run = compressed_run(&["a", "b"]);
    run.index = Some(index_for(&["a"]));
    run.files[1] = ("b".to_string(), page_source(source_model()));
    let (game, warnings) = run.game();
    assert!(matches!(
        model_of(&game, "a").images[0],
        ModelImage::Packed(_)
    ));
    assert!(matches!(
        model_of(&game, "b").images[0],
        ModelImage::Pixels(_)
    ));
    let found = unpacked_warnings(&warnings);
    assert_eq!(found.len(), 1, "{warnings:#?}");
    assert!(found[0].message.contains("модель b не сжата"));
}

fn errors_of(run: &Run<'_>) -> Vec<String> {
    run.load()
        .err()
        .unwrap_or_else(|| panic!("игра должна была не загрузиться"))
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

#[test]
fn a_packed_file_that_is_absent_empty_or_broken_stops_the_game_and_names_the_file() {
    let wanted = format!("сжатая картинка {COLOR_FILE} модели a");
    for (label, bytes) in [("нет", None), ("пуст", Some(Vec::new()))] {
        let mut run = compressed_run(&["a"]);
        run.textures[0].1 = bytes;
        let errors = errors_of(&run);
        assert!(
            errors
                .iter()
                .any(|e| e.contains(&wanted) && e.contains("не найдена или пуста")),
            "{label}: {errors:#?}"
        );
    }
    let mut run = compressed_run(&["a"]);
    run.textures[0].1 = Some(vec![1, 2, 3]);
    let errors = errors_of(&run);
    assert!(
        errors.iter().any(|e| e.contains(&format!(
            "{wanted} испорчена — служба сжатия пересожмёт её, если исходная модель на месте"
        ))),
        "{errors:#?}"
    );
}

#[test]
fn a_packed_file_of_another_format_or_a_wrong_number_of_levels_is_the_same_error() {
    let cases = [
        ktx2(98, 4, 4, &[vec![0; 16], vec![0; 16], vec![0; 16]]),
        ktx2(146, 4, 4, &[vec![0; 16], vec![0; 16]]),
        ktx2(146, 4, 4, &[vec![0; 16], vec![0; 16], vec![0; 15]]),
    ];
    for bytes in cases {
        let mut run = compressed_run(&["a"]);
        run.textures[0].1 = Some(bytes);
        let errors = errors_of(&run);
        assert!(
            errors.iter().any(|e| e.contains("испорчена")),
            "{errors:#?}"
        );
    }
}

#[test]
fn a_light_model_that_the_page_did_not_find_is_an_error_naming_its_path() {
    let mut run = compressed_run(&["a"]);
    run.files[0].1.bytes = None;
    let errors = errors_of(&run);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("\"imported/models/a.glb\" не найден")),
        "{errors:#?}"
    );
}

fn needed(index: Option<&str>) -> NeededMedia {
    let text = game_json(&["a", "b", "c"]);
    let (config, _) = read_entry(&text).expect("game.json читается");
    read_texts_with_imported(&config, Some(PROPS), None, None, None, index)
}

#[test]
fn read_texts_names_light_models_and_each_packed_file_once_instead_of_the_source() {
    let index = index_for(&["a", "c"]);
    let media = needed(Some(&index));
    assert_eq!(
        media.models,
        [
            ("a".to_string(), "imported/models/a.glb".to_string()),
            ("b".to_string(), "models/b.glb".to_string()),
            ("c".to_string(), "imported/models/c.glb".to_string()),
        ]
    );
    assert_eq!(media.model_textures, [COLOR_FILE, NORMAL_FILE]);
}

#[test]
fn without_an_index_read_texts_names_the_source_models_and_no_packed_files() {
    let expected = [
        ("a".to_string(), "models/a.glb".to_string()),
        ("b".to_string(), "models/b.glb".to_string()),
        ("c".to_string(), "models/c.glb".to_string()),
    ];
    let media = needed(None);
    assert_eq!(media.models, expected);
    assert!(media.model_textures.is_empty());
    let text = game_json(&["a", "b", "c"]);
    let (config, _) = read_entry(&text).expect("game.json читается");
    assert_eq!(
        read_texts(&config, Some(PROPS), None, None).models,
        expected
    );
    assert_eq!(needed(Some("{ not json")).models, expected);
}

#[test]
fn the_model_shader_reads_two_channels_of_the_normal_map_and_computes_z() {
    let source = include_str!("../shaders/scene3d.wgsl");
    let start = source.find("fn model_point").expect("точка модели");
    let end = source[start..].find("@fragment").expect("точка входа") + start;
    let body = &source[start..end];
    assert!(
        body.contains("sqrt(max(0.0, 1.0 - dot(raw_tilt, raw_tilt)))"),
        "Z — sqrt(max(0, 1 − X² − Y²)) по двум каналам карты: {body}"
    );
    assert!(
        !body.contains("normal_sample.z") && !body.contains("normal_sample.xyz"),
        "синий канал карты не читается: {body}"
    );
}

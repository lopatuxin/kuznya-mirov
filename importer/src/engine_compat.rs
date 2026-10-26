//! Вывод службы сжатия, прочитанный загрузчиком движка: облегчённая модель и файлы KTX2, написанные
//! `game::sync` из маленького `.glb`, собираются в те же части и материалы, что исходная модель.

use std::fs;

use engine::core::game::Game;
use engine::core::model::{Material, Model, ModelImage, PackedFormat, Texture};
use engine::data::error::GameError;
use engine::data::load::{ImageVerdict, ModelFile, load_rest_with_imports, read_entry};

use crate::game::{self, Index, Refused};
use crate::testing::{Spec, TempDir, material, png};

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag"}}"#;
const GAME: &str = r##"{"name":"M","scene":{"width":40,"height":30,"background":"#000000","view_height":16,"camera":{"pitch":55}},
"random_seed":1,"max_objects":64,"start_screen":"main",
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},"models":{"a":{"path":"models/a.glb"}}}}"##;

type Textures = Vec<(String, Option<Vec<u8>>)>;

fn load(
    files: &[(String, ModelFile)],
    index: Option<&str>,
    textures: &Textures,
) -> (Game, Vec<GameError>) {
    let (config, _) = read_entry(GAME).unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
    let (game, _, warnings, _) = load_rest_with_imports(
        GAME,
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
        false,
        &[],
        None,
        &[],
        &[],
        &[],
        files,
        index,
        textures,
    )
    .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors));
    (game, warnings)
}

fn model_a(game: &Game) -> &Model {
    let models = game.world.models();
    models
        .get(models.index_of("a").expect("модель объявлена"))
        .expect("модель есть")
}

fn texture_slots(material: &Material) -> [Option<(Texture, f32)>; 4] {
    [
        material.base_texture.map(|texture| (texture, 0.0)),
        material.metal_rough_texture.map(|texture| (texture, 0.0)),
        material.normal_texture,
        material.occlusion_texture,
    ]
}

fn without_textures(material: &Material) -> Material {
    Material {
        base_texture: None,
        metal_rough_texture: None,
        normal_texture: None,
        occlusion_texture: None,
        ..material.clone()
    }
}

fn dimensions(model: &Model, texture: &Texture) -> (u32, u32) {
    match &model.images[texture.image] {
        ModelImage::Pixels(image) => (image.width, image.height),
        ModelImage::Packed(image) => (image.width, image.height),
    }
}

fn packed_format(model: &Model, texture: &Texture) -> (PackedFormat, usize) {
    match &model.images[texture.image] {
        ModelImage::Packed(image) => (image.format, image.levels.len()),
        ModelImage::Pixels(_) => panic!("картинка облегчённой модели должна быть сжатой"),
    }
}

fn is_unpacked_warning(warning: &GameError) -> bool {
    warning.message.contains("не сжата")
}

#[test]
fn the_engine_builds_the_same_model_from_the_importer_output_and_reads_its_files() {
    let pictures = vec![png(16, 16, 3), png(10, 8, 4)];
    let materials = material(Some(0), None, Some(0), Some(1));
    let source = Spec::new(pictures.clone(), vec![0, 1], materials).build();

    let dir = TempDir::new();
    let game_dir = dir.path().join("game");
    fs::create_dir_all(game_dir.join("models")).unwrap();
    fs::write(game_dir.join("models/a.glb"), &source).unwrap();
    game::sync(&game_dir, &|_| {}, &mut Refused::new()).unwrap();

    let index_text = fs::read_to_string(game::index_path(&game_dir)).unwrap();
    let index: Index = serde_json::from_str(&index_text).unwrap();
    let entry = &index.models["models/a.glb"];
    let light = ModelFile {
        bytes: Some(fs::read(game_dir.join(&entry.model)).unwrap()),
        images: Vec::new(),
    };
    let textures: Textures = entry
        .images
        .iter()
        .flatten()
        .map(|path| (path.clone(), Some(fs::read(game_dir.join(path)).unwrap())))
        .collect();
    assert_eq!(
        textures.len(),
        3,
        "цвет и мелкий рельеф одной картинки, затенение"
    );

    let page_pixels = ModelFile {
        bytes: Some(source),
        images: pictures
            .iter()
            .enumerate()
            .map(|(number, bytes)| {
                let decoded = image::load_from_memory(bytes).unwrap().to_rgba8();
                let verdict = ImageVerdict::Ok {
                    width: decoded.width(),
                    height: decoded.height(),
                    pixels: decoded.into_raw(),
                };
                (number, verdict)
            })
            .collect(),
    };

    let (from_source, source_warnings) = load(&[("a".into(), page_pixels)], None, &Vec::new());
    let (from_light, light_warnings) = load(&[("a".into(), light)], Some(&index_text), &textures);
    let (source, light) = (model_a(&from_source), model_a(&from_light));

    assert!(source_warnings.iter().any(is_unpacked_warning));
    assert!(
        !light_warnings.iter().any(is_unpacked_warning),
        "{light_warnings:#?}"
    );
    assert_eq!(light.parts, source.parts);
    assert_eq!((light.size, light.origin), (source.size, source.origin));
    assert_eq!(light.materials.len(), source.materials.len());
    for (light_material, source_material) in light.materials.iter().zip(&source.materials) {
        assert_eq!(
            without_textures(light_material),
            without_textures(source_material)
        );
        let slots = texture_slots(light_material).into_iter();
        for (light_slot, source_slot) in slots.zip(texture_slots(source_material)) {
            let (Some((light_texture, light_scale)), Some((source_texture, source_scale))) =
                (light_slot, source_slot)
            else {
                assert_eq!(light_slot.is_some(), source_slot.is_some());
                continue;
            };
            assert_eq!(light_texture.sampler, source_texture.sampler);
            assert_eq!(light_scale, source_scale);
            assert_eq!(
                dimensions(light, &light_texture),
                dimensions(source, &source_texture)
            );
        }
    }

    let material = &light.materials[0];
    let colour = material.base_texture.expect("цвет");
    let (normal, _) = material.normal_texture.expect("мелкий рельеф");
    let (occlusion, _) = material.occlusion_texture.expect("затенение");
    assert_ne!(
        colour.image, normal.image,
        "картинка в двух ролях — два файла"
    );
    assert_eq!(packed_format(light, &colour), (PackedFormat::Bc7Srgb, 5));
    assert_eq!(packed_format(light, &normal), (PackedFormat::Bc5, 5));
    assert_eq!(packed_format(light, &occlusion), (PackedFormat::Rgba8, 4));
}

//! Фаза 66 — картинка у полоски в мире (`image`, `back_image`) и след потери (`trail_color`, `trail_image`) у
//! шкалы экрана и полоски в мире. Игра собрана из текстов; страница здесь — тот же расчёт, что у
//! `wasm::Engine`: раскладка, память следов и время партии.

use engine::core::game::Game;
use engine::core::interface::{self, Part};
use engine::core::property;
use engine::core::scene::LayerView;
use engine::core::screens::{ScreenState, ScreensConfig, WorldElementKind};
use engine::core::trail::{TrailOwner, Trails};
use engine::core::world_elements::{BarDraw, add_trails, compute_world_draws};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest_with_tables, read_entry, read_texts};

const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];
const NO_RULES: &str = r#"{"rules":[]}"#;
const PROPS: &str = r#"{"properties":{"robber":"flag","hp":"number","max_hp":"number"}}"#;

const SCENE: &str = r#"{"objects":[
    {"name":"a","robber":true,"position":[2,5],"size":[1,2],"hp":60,"max_hp":60},
    {"name":"b","robber":true,"position":[6,5],"size":[1,2],"hp":60,"max_hp":60},
    {"name":"c","robber":true,"position":[10,5],"size":[1,2],"hp":60,"max_hp":60}
]}"#;

/// Подложка с наконечниками 78 × 12, заливка и след 60 × 12 с полями по 1 точке, видео.
const IMAGES: &str = r#""back":{"path":"back.png"},
    "fill":{"path":"fill.png","slice":[1,0,1,0]},
    "trail":{"path":"trail.png","slice":[1,0,1,0]},
    "clip":{"path":"clip.mp4"}"#;

const IMAGE_FILES: &[(&str, u32, u32)] = &[
    ("back", 78, 12),
    ("fill", 60, 12),
    ("trail", 60, 12),
    ("clip", 8, 16),
];

const BAR: &str = r#""kind":"bar","for":{"has":["robber"]},"anchor":"top","offset":[0,-0.55],
    "size":[0.867,0.133],"value":"hp","max":"max_hp""#;

fn screens_with(world_elements: &str, elements: &str) -> String {
    format!(
        r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[{elements}]}}],
        "world_elements":[{world_elements}]}}"#
    )
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn try_load(screens: &str) -> Result<Loaded, LoadFailure> {
    let game_json = format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"ui.ttf"}},"images":{{{IMAGES}}}}}}}"##
    );
    let (config, _) = read_entry(&game_json)?;
    let fonts: Vec<(String, Option<Vec<u8>>)> =
        read_texts(&config, Some(PROPS), Some(screens), None)
            .fonts
            .into_iter()
            .map(|(name, _)| (name, Some(FONT_BYTES.to_vec())))
            .collect();
    let verdicts: Vec<(String, ImageVerdict)> = IMAGE_FILES
        .iter()
        .map(|(name, width, height)| {
            let verdict = if *name == "clip" {
                ImageVerdict::Video {
                    width: *width,
                    height: *height,
                    seconds: 1.0,
                    reverse: None,
                }
            } else {
                ImageVerdict::Ok {
                    width: *width,
                    height: *height,
                    pixels: vec![255; (*width * *height * 4) as usize],
                }
            };
            ((*name).to_string(), verdict)
        })
        .collect();
    load_rest_with_tables(
        &game_json,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(NO_RULES),
        Some(screens),
        &fonts,
        &[],
        &[],
        &verdicts,
        None,
        false,
        &[],
    )
}

fn load(screens: &str) -> (Game, ScreensConfig, Vec<ImageDecl>) {
    match try_load(screens) {
        Ok((game, config, _warnings, images)) => (game, config, images),
        Err(failure) => panic!("игра не загрузилась: {:#?}", failure.errors),
    }
}

/// Каждая ошибка как `(путь, текст)`.
fn errors_of(screens: &str) -> Vec<(String, String)> {
    match try_load(screens) {
        Ok(_) => Vec::new(),
        Err(failure) => failure
            .errors
            .into_iter()
            .map(|e| (e.path, e.message))
            .collect(),
    }
}

fn expect_error(world_elements: &str, elements: &str, path: &str, fragment: &str) {
    let errors = errors_of(&screens_with(world_elements, elements));
    assert!(
        errors
            .iter()
            .any(|(at, message)| at == path && message.contains(fragment)),
        "ожидалась ошибка по пути «{path}» с «{fragment}», есть {errors:?}"
    );
}

fn id_of(game: &Game, name: &str) -> u32 {
    game.world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some(name))
        .unwrap_or_else(|| panic!("нет объекта {name}"))
}

fn set_hp(game: &mut Game, name: &str, hp: f64) {
    let id = id_of(game, name);
    let prop = game.properties.resolve("hp").expect("hp");
    game.world.set_number(id, prop, hp);
}

fn image_id(images: &[ImageDecl], name: &str) -> usize {
    images
        .iter()
        .position(|decl| decl.name == name)
        .unwrap_or_else(|| panic!("нет картинки {name}"))
}

/// Полоски кадра на `steps` шаге партии, как их считает `wasm::compose_world_elements`.
fn bars_at(game: &Game, config: &ScreensConfig, trails: &mut Trails, steps: u64) -> Vec<BarDraw> {
    let (mut bars, _, _) = compute_world_draws(
        &game.world,
        &game.scene,
        &game.properties,
        &config.world_elements,
        &LayerView::default(),
    );
    let mut pass = trails.pass(TrailOwner::World, steps);
    add_trails(&mut bars, &config.world_elements, &game.world, &mut pass);
    bars
}

fn bar_of(bars: &[BarDraw], game: &Game, name: &str) -> BarDraw {
    let id = id_of(game, name);
    *bars
        .iter()
        .find(|bar| bar.object == id)
        .unwrap_or_else(|| panic!("нет полоски над {name}"))
}

fn robber_bar(extra: &str) -> String {
    let comma = if extra.is_empty() { "" } else { "," };
    format!("{{{BAR}{comma}{extra}}}")
}

const FULL: &str = r#""image":"fill","back_image":"back","trail_image":"trail""#;

#[test]
fn every_picture_a_bar_names_counts_as_used() {
    let unused_in = |warnings: &[GameError]| -> Vec<String> {
        warnings
            .iter()
            .filter(|warning| warning.message.contains("объявлена, но её не называет"))
            .map(|warning| warning.message.clone())
            .collect()
    };
    let (_, _, warnings, _) = try_load(&screens_with(&robber_bar(FULL), ""))
        .unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
    let unused = unused_in(&warnings);
    assert_eq!(unused.len(), 1, "{unused:?}");
    assert!(unused[0].contains("clip"), "{unused:?}");
    let screen_bar = r##"{"kind":"bar","anchor":"top_left","size":[100,10],"for":{"has":["robber"]},
        "value":"hp","max":"max_hp","image":"fill","back_image":"back","trail_image":"trail"}"##;
    let (_, _, warnings, _) = try_load(&screens_with("", screen_bar))
        .unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
    assert_eq!(unused_in(&warnings).len(), 1, "{warnings:?}");
}

#[test]
fn a_world_bar_takes_one_fill_at_most_one_backing_and_at_most_one_trail() {
    let path = "world_elements[0]";
    expect_error(
        &robber_bar(r##""color":"#ff0000","image":"fill""##),
        "",
        path,
        "заданы и color, и image",
    );
    expect_error(&robber_bar(""), "", path, "нужен один из color или image");
    expect_error(
        &robber_bar(r##""image":"fill","back_color":"#000000","back_image":"back""##),
        "",
        path,
        "заданы и \"back_color\", и \"back_image\"",
    );
    expect_error(
        &robber_bar(r##""image":"fill","trail_color":"#ffffff","trail_image":"trail""##),
        "",
        path,
        "заданы и \"trail_color\", и \"trail_image\"",
    );
}

#[test]
fn a_world_bar_picture_has_to_be_a_declared_picture_not_a_video() {
    expect_error(
        &robber_bar(r#""image":"nothing""#),
        "",
        "world_elements[0] → image",
        "nothing",
    );
    expect_error(
        &robber_bar(r#""image":"clip""#),
        "",
        "world_elements[0] → image",
        "объявлена видео",
    );
    expect_error(
        &robber_bar(r#""image":"fill","back_image":"clip""#),
        "",
        "world_elements[0] → back_image",
        "объявлена видео",
    );
    expect_error(
        &robber_bar(r#""image":"fill","trail_image":"nothing""#),
        "",
        "world_elements[0] → trail_image",
        "nothing",
    );
}

#[test]
fn a_world_bar_picture_takes_no_property_substitution() {
    expect_error(
        &robber_bar(r#""image":"{hp}""#),
        "",
        "world_elements[0] → image",
        "{hp}",
    );
}

#[test]
fn a_screen_bar_takes_at_most_one_trail_and_a_declared_one() {
    let bar = r##"{"kind":"bar","anchor":"top_left","size":[100,10],"for":{"has":["robber"]},
        "value":"hp","max":"max_hp","color":"#ff0000""##;
    expect_error(
        "",
        &format!(r##"{bar},"trail_color":"#ffffff","trail_image":"trail"}}"##),
        "screens[0] → elements → [0]",
        "заданы и \"trail_color\", и \"trail_image\"",
    );
    expect_error(
        "",
        &format!(r#"{bar},"trail_image":"nothing"}}"#),
        "screens[0] → elements → [0] → trail_image",
        "nothing",
    );
    expect_error(
        "",
        &format!(r#"{bar},"trail_image":"clip"}}"#),
        "screens[0] → elements → [0] → trail_image",
        "объявлена видео",
    );
}

#[test]
fn a_world_bar_half_full_fills_half_of_its_width_with_its_pictures() {
    let (mut game, config, images) = load(&screens_with(&robber_bar(FULL), ""));
    set_hp(&mut game, "a", 30.0);
    let bars = bars_at(&game, &config, &mut Trails::default(), 0);
    let bar = bar_of(&bars, &game, "a");
    assert!((bar.fill.size[0] - 0.4335).abs() < 1e-4, "{:?}", bar.fill);
    assert!((bar.fill.size[1] - 0.133).abs() < 1e-6);
    assert_eq!(bar.share, 0.5);
    assert!((bar.width - 0.867).abs() < 1e-6);
    assert_eq!(bar.fill.image, Some(image_id(&images, "fill")));
    assert_eq!(
        bar.back.expect("подложка").image,
        Some(image_id(&images, "back"))
    );
    assert_eq!(bar.back.expect("подложка").size, [bar.width, 0.133]);
    assert_eq!(bar.trail, None, "потери ещё не было");
    let full = bar_of(&bars, &game, "b");
    assert!((full.fill.size[0] - 0.867).abs() < 1e-6);
}

#[test]
fn a_world_bar_without_new_fields_is_drawn_as_before() {
    let (game, config, _) = load(&screens_with(
        &robber_bar(r##""color":"#d03030","back_color":"#00000080""##),
        "",
    ));
    let bars = bars_at(&game, &config, &mut Trails::default(), 0);
    let bar = bar_of(&bars, &game, "a");
    assert_eq!(
        bar.fill.color,
        [208.0 / 255.0, 48.0 / 255.0, 48.0 / 255.0, 1.0]
    );
    assert_eq!(bar.fill.image, None);
    assert_eq!(bar.back.expect("подложка").image, None);
    assert_eq!(bar.trail, None);
    let WorldElementKind::Bar { trail, .. } = &config.world_elements[0].kind else {
        panic!("полоска");
    };
    assert!(trail.is_none());
}

#[test]
fn a_loss_leaves_a_trail_from_the_new_share_to_the_old_one_for_three_tenths_of_a_second() {
    let (mut game, config, images) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 46.0);
    let bars = bars_at(&game, &config, &mut trails, 1);
    let bar = bar_of(&bars, &game, "a");
    let trail = bar.trail.expect("след");
    assert_eq!(trail.image, Some(image_id(&images, "trail")));
    assert_eq!(trail.position, bar.fill.position);
    assert!(
        (trail.size[0] - bar.width).abs() < 1e-6,
        "до прежней доли, целой"
    );
    assert!(trail.size[0] > bar.fill.size[0], "заливка уже встала на 46");
    assert_eq!(
        bar_of(&bars, &game, "b").trail,
        None,
        "у другого разбойника следа нет"
    );
    assert_eq!(bar_of(&bars, &game, "c").trail, None);

    let standing = bars_at(&game, &config, &mut trails, 1 + 18);
    assert_eq!(
        bar_of(&standing, &game, "a").trail,
        Some(trail),
        "0,3 с — стоит"
    );
    let shrinking = bars_at(&game, &config, &mut trails, 1 + 27);
    let shrunk = bar_of(&shrinking, &game, "a").trail.expect("сжимается");
    assert!(shrunk.size[0] < trail.size[0] && shrunk.size[0] > bar.fill.size[0]);
    let gone = bars_at(&game, &config, &mut trails, 1 + 36);
    assert_eq!(bar_of(&gone, &game, "a").trail, None, "0,6 с — пропал");
}

#[test]
fn two_losses_in_a_row_keep_the_right_edge_and_restart_the_standing() {
    let (mut game, config, _) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 46.0);
    bars_at(&game, &config, &mut trails, 1);
    set_hp(&mut game, "a", 32.0);
    let second = bars_at(&game, &config, &mut trails, 13);
    let bar = bar_of(&second, &game, "a");
    assert!((bar.share - 32.0 / 60.0).abs() < 1e-6);
    let trail = bar.trail.expect("след");
    assert!((trail.size[0] - bar.width).abs() < 1e-6, "от 32 до 60");
    let later = bars_at(&game, &config, &mut trails, 13 + 18);
    assert_eq!(
        bar_of(&later, &game, "a").trail,
        Some(trail),
        "0,3 с от второго удара"
    );
    let gone = bars_at(&game, &config, &mut trails, 13 + 36);
    assert_eq!(bar_of(&gone, &game, "a").trail, None);
}

#[test]
fn a_gain_removes_the_trail_at_once_and_a_standing_world_keeps_it() {
    let (mut game, config, _) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 30.0);
    let first = bars_at(&game, &config, &mut trails, 5);
    let trail = bar_of(&first, &game, "a").trail.expect("след");
    for _ in 0..120 {
        let paused = bars_at(&game, &config, &mut trails, 5);
        assert_eq!(
            bar_of(&paused, &game, "a").trail,
            Some(trail),
            "мир стоит — след стоит"
        );
    }
    set_hp(&mut game, "a", 40.0);
    let healed = bars_at(&game, &config, &mut trails, 6);
    assert_eq!(bar_of(&healed, &game, "a").trail, None);
    assert!((bar_of(&healed, &game, "a").share - 40.0 / 60.0).abs() < 1e-6);
}

#[test]
fn a_changed_max_with_the_same_value_is_a_change_of_the_share() {
    let (mut game, config, _) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    let id = id_of(&game, "a");
    let max = game.properties.resolve("max_hp").expect("max_hp");
    game.world.set_number(id, max, 120.0);
    let bars = bars_at(&game, &config, &mut trails, 1);
    let bar = bar_of(&bars, &game, "a");
    assert_eq!(bar.share, 0.5);
    assert!(bar.trail.is_some(), "доля упала — след");
}

#[test]
fn a_value_over_the_max_is_full_without_a_trail_and_below_zero_trails_from_zero() {
    let (mut game, config, _) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 90.0);
    let over = bars_at(&game, &config, &mut trails, 1);
    assert_eq!(bar_of(&over, &game, "a").share, 1.0);
    assert_eq!(bar_of(&over, &game, "a").trail, None);
    set_hp(&mut game, "a", -5.0);
    let below = bars_at(&game, &config, &mut trails, 2);
    let bar = bar_of(&below, &game, "a");
    assert_eq!(bar.share, 0.0);
    assert_eq!(bar.fill.size[0], 0.0, "заливки нет");
    assert!(
        (bar.trail.expect("след").size[0] - bar.width).abs() < 1e-6,
        "от нуля до прежней доли"
    );
}

#[test]
fn a_gone_object_loses_its_trail_and_a_new_one_starts_without() {
    let (mut game, config, _) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 10.0);
    bars_at(&game, &config, &mut trails, 1);
    let a = id_of(&game, "a");
    game.world.delete(a);
    let without = bars_at(&game, &config, &mut trails, 2);
    assert_eq!(without.len(), 2);

    let born = game.world.create();
    let hp = game.properties.resolve("hp").expect("hp");
    let max = game.properties.resolve("max_hp").expect("max_hp");
    let robber = game.properties.resolve("robber").expect("robber");
    game.world.set_vec2(born, property::POSITION, [2.0, 5.0]);
    game.world.set_vec2(born, property::SIZE, [1.0, 2.0]);
    game.world.set_number(born, hp, 20.0);
    game.world.set_number(born, max, 60.0);
    game.world.set_flag(born, robber, true);
    assert_eq!(born, a, "на месте прежнего");
    let bars = bars_at(&game, &config, &mut trails, 3);
    let bar = bars.iter().find(|bar| bar.object == born).expect("полоска");
    assert_eq!(bar.trail, None, "новый объект начинает без следа");
}

#[test]
fn an_object_without_value_or_max_has_no_bar_and_forgets_its_trail() {
    let (mut game, config, _) = load(&screens_with(&robber_bar(FULL), ""));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 30.0);
    bars_at(&game, &config, &mut trails, 1);
    let a = id_of(&game, "a");
    let max = game.properties.resolve("max_hp").expect("max_hp");
    game.world.clear_property(a, max);
    let gone = bars_at(&game, &config, &mut trails, 2);
    assert!(gone.iter().all(|bar| bar.object != a), "полоски нет");
    let max_again = 60.0;
    game.world.set_number(a, max, max_again);
    let back = bars_at(&game, &config, &mut trails, 3);
    assert_eq!(bar_of(&back, &game, "a").trail, None, "след забыт");
}

#[test]
fn a_bar_without_a_trail_field_never_gets_one() {
    let (mut game, config, _) = load(&screens_with(
        &robber_bar(r#""image":"fill","back_image":"back""#),
        "",
    ));
    let mut trails = Trails::default();
    bars_at(&game, &config, &mut trails, 0);
    set_hp(&mut game, "a", 10.0);
    let bars = bars_at(&game, &config, &mut trails, 1);
    assert_eq!(bar_of(&bars, &game, "a").trail, None);
}

/// Шкала экрана с `trail_color`: раскладка даёт её прямоугольник, память следов — правый край следа.
#[test]
fn a_screen_bar_trail_lies_from_the_left_edge_to_the_old_share_under_the_fill() {
    let bar = r##"{"kind":"bar","anchor":"top_left","offset":[10,10],"size":[200,20],
        "for":{"has":["robber"]},"value":"hp","max":"max_hp","image":"fill","trail_color":"#ffffff80"}"##;
    let (mut game, config, _) = load(&screens_with("", bar));
    let screen = &config.screens[0];
    let Some(Part::Element(placed)) =
        interface::layout(screen, 0, &game.world, &ScreenState::new(0), [800.0, 600.0])
            .into_iter()
            .next()
    else {
        panic!("шкала на экране");
    };
    assert_eq!(placed.rect, [10.0, 10.0, 200.0, 20.0]);
    let object = placed.object.expect("объект шкалы");
    let key = placed.key;
    let mut trails = Trails::default();
    let mut observe = |game: &Game, steps: u64| {
        let share = interface::bar_share(placed.element, placed.object, &game.world).expect("доля");
        let mut pass = trails.pass(TrailOwner::Screen(0), steps);
        let edge = pass.observe(
            key.element,
            key.inner,
            object,
            game.world.generation(object),
            share,
        );
        (share, edge)
    };
    assert_eq!(observe(&game, 0), (1.0, None));
    set_hp(&mut game, "a", 30.0);
    let (share, edge) = observe(&game, 1);
    assert_eq!(share, 0.5);
    let edge = edge.expect("след");
    assert_eq!(
        interface::bar_fill_rect(placed.rect, share),
        Some([10.0, 10.0, 100.0, 20.0])
    );
    assert_eq!(
        interface::bar_fill_rect(placed.rect, edge),
        Some([10.0, 10.0, 200.0, 20.0]),
        "след — до прежней доли, под заливкой"
    );
    assert_eq!(observe(&game, 1 + 36).1, None);
}

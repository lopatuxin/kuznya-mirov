//! Фаза 40 — списки и кнопки с кодом: элемент при объекте, список по отбору с прокруткой, команда `run`
//! у кнопок и клавиш экрана, проверки перед запуском. Игра собрана из текстов, её ведёт `ui_support::Ui` теми
//! же вызовами, что `wasm::Engine`.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::interface::{self, Part};
use engine::core::screens::{Element, TextPart, format_text_for};
use engine::data::load::GameError;
use engine::data::session::PlaySession;
use support::{Files, Ui, VIEWPORT};

const PROPS: &str = r#"{"properties":{"order":"flag","title":"text","prio":"number","note":"text",
    "taken":"flag","selected":"flag","score":"number"}}"#;

// Семь заказов с приоритетами: у «Лука» его нет, у «Сыра» и «Рыбы» он одинаковый.
const SCENE: &str = r#"{"objects":[
    {"name":"o0","order":true,"title":"Хлеб","prio":50,"note":"n0"},
    {"name":"o1","order":true,"title":"Молоко","prio":10,"note":"n1"},
    {"name":"o2","order":true,"title":"Сыр","prio":30,"note":"n2"},
    {"name":"o3","order":true,"title":"Лук"},
    {"name":"o4","order":true,"title":"Рыба","prio":30,"note":"n4"},
    {"name":"o5","order":true,"title":"Чай","prio":20,"note":"n5"},
    {"name":"o6","order":true,"title":"Мёд","prio":40,"note":"n6"},
    {"name":"game","score":0}
]}"#;

const CODE: &str = r#"
-- Больше трети предела операций на вызов: три вызова подряд пройдут, только если у каждого предел свой.
function take(o)
  local n = 0
  for i = 1, 400000 do n = n + 1 end
  o.taken = true
end
function finish(o) delete(o) end
function mark(o) o.selected = true end
function count() find{has = {"score"}}[1].score = #find{has = {"order"}} end
function boom() error("плохо") end
function spin() while true do end end
"#;

/// Элементы обоих экранов: список заказов 300 × 300 (по 64 с промежутком 4 — видно четыре строки), панель
/// и надпись выбранного заказа, кнопка при нём и две кнопки без объекта.
fn elements(sort: &str) -> String {
    format!(
        r##"[
    {{"kind":"list","anchor":"top_left","offset":[0,0],"size":[300,300],
      "for":{{"has":["order"],"without":["taken"]}},{sort}"row_height":64,"gap":4,
      "row":[
        {{"kind":"panel","offset":[0,0],"size":[280,64],"color":"#f4efe6ee"}},
        {{"kind":"label","offset":[10,6],"size":[130,22],"text":"{{title}}","font":"ui"}},
        {{"kind":"label","offset":[10,34],"size":[130,20],"text":"{{prio}}","font":"ui"}},
        {{"kind":"button","offset":[150,6],"size":[60,24],"text":"Взять","font":"ui",
          "color":"#2f6f4f","on_click":["run","take"]}},
        {{"kind":"button","offset":[220,6],"size":[60,24],"text":"Готово","font":"ui",
          "color":"#8a4b2f","on_click":["run","finish"]}}
      ]}},
    {{"kind":"panel","anchor":"top_left","offset":[320,0],"size":[200,100],"color":"#ffffffee",
      "for":{{"has":["selected"]}}}},
    {{"kind":"label","anchor":"top_left","offset":[320,0],"size":[200,100],"wrap":true,
      "text":"{{title}}: {{note}}","font":"ui","for":{{"has":["selected"]}}}},
    {{"kind":"button","anchor":"top_left","offset":[320,120],"size":[100,30],"text":"Взять выбранный",
      "font":"ui","color":"#2f6f4f","on_click":["run","take"],"for":{{"has":["selected"]}}}},
    {{"kind":"button","anchor":"top_left","offset":[320,200],"size":[100,30],"text":"Сломать",
      "font":"ui","color":"#aa0000","on_click":["run","boom"]}},
    {{"kind":"button","anchor":"top_left","offset":[320,240],"size":[100,30],"text":"Зациклить",
      "font":"ui","color":"#aa0000","on_click":["run","spin"]}},
    {{"kind":"label","anchor":"top_left","offset":[320,280],"size":[200,20],
      "text":"Очки: {{game.score}}","font":"ui"}}
]"##
    )
}

fn screens_json(sort: &str) -> String {
    let elements = elements(sort);
    format!(
        r##"{{"screens":[
    {{"name":"main","world_runs":true,
      "keys":{{"KeyP":["run","count"],"Escape":["show_screen","pause"]}},"elements":{elements}}},
    {{"name":"pause","world_runs":false,"keys":{{"Escape":["resume"]}},"elements":{elements}}}
]}}"##
    )
}

const SORTED: &str = r#""sort_by":"prio","#;

fn load_with(sort: &str) -> Ui {
    let screens = screens_json(sort);
    Files::new(PROPS, SCENE, &screens).with_code(CODE).load()
}

fn load() -> Ui {
    load_with(SORTED)
}

/// Номера объектов строк списка `element` активного экрана, сверху вниз, как они видны.
fn visible_rows(ui: &Ui, element: usize) -> Vec<u32> {
    let screen_id = ui.state.active();
    let parts = interface::layout(
        &ui.config.screens[screen_id],
        screen_id,
        &ui.game.world,
        &ui.state,
        VIEWPORT,
    );
    let mut rows = Vec::new();
    for part in parts {
        if let Part::Element(placed) = part
            && placed.key.element == element
            && placed.key.inner == 0
            && let Some(object) = placed.object
        {
            rows.push(object);
        }
    }
    rows
}

fn all_rows(ui: &Ui, descending: bool, sort: bool) -> Vec<u32> {
    let Element::List { for_, sort_by, .. } = &ui.config.screens[0].elements[0] else {
        panic!("первый элемент — список");
    };
    interface::list_rows(for_, sort_by.filter(|_| sort), descending, &ui.game.world)
}

fn layout_parts_count(ui: &Ui) -> (usize, Vec<[f32; 4]>) {
    let screen_id = ui.state.active();
    let parts = interface::layout(
        &ui.config.screens[screen_id],
        screen_id,
        &ui.game.world,
        &ui.state,
        VIEWPORT,
    );
    let bars = parts
        .iter()
        .filter_map(|part| match part {
            Part::ScrollBar { rect, .. } => Some(*rect),
            Part::Element(_) => None,
        })
        .collect();
    (parts.len(), bars)
}

#[test]
fn the_fixture_loads_without_warnings() {
    let screens = screens_json(SORTED);
    let (_, _, warnings) = Files::new(PROPS, SCENE, &screens)
        .with_code(CODE)
        .try_load()
        .expect("грузится");
    assert_eq!(warnings, Vec::<GameError>::new(), "{warnings:?}");
}

// -------------------------------------------------------------------------------------------
// Элемент при объекте
// -------------------------------------------------------------------------------------------

#[test]
fn an_element_with_for_lives_only_while_its_object_does_and_reads_its_properties() {
    let mut ui = load();
    let labelled = |ui: &Ui| -> Vec<(u32, String)> {
        let screen_id = ui.state.active();
        interface::layout(
            &ui.config.screens[screen_id],
            screen_id,
            &ui.game.world,
            &ui.state,
            VIEWPORT,
        )
        .into_iter()
        .filter_map(|part| match part {
            Part::Element(placed) if placed.key.element == 2 => {
                let Element::Label { text, .. } = placed.element else {
                    return None;
                };
                let object = placed.object?;
                Some((
                    object,
                    format_text_for(text, &ui.game.world, &ui.game.properties, Some(object)),
                ))
            }
            _ => None,
        })
        .collect()
    };
    assert!(
        labelled(&ui).is_empty(),
        "выбранного заказа нет — надписи нет"
    );

    let o2 = ui.id_of("o2");
    let o4 = ui.id_of("o4");
    let selected = ui.game.properties.resolve("selected").unwrap();
    ui.game.world.set_flag(o4, selected, true);
    assert_eq!(labelled(&ui), vec![(o4, "Рыба: n4".to_string())]);

    ui.game.world.set_flag(o2, selected, true);
    assert_eq!(
        labelled(&ui),
        vec![(o2, "Сыр: n2".to_string())],
        "объект элемента — подходящий с меньшим номером"
    );
    ui.game.world.set_flag(o2, selected, false);
    ui.game.world.set_flag(o4, selected, false);
    assert!(labelled(&ui).is_empty());
}

#[test]
fn a_button_with_for_is_clicked_only_while_its_object_exists_and_gets_it() {
    let mut ui = load();
    // Кнопка «Взять выбранный» стоит в [320, 120] размером 100 × 30.
    ui.click(350.0, 130.0);
    assert_eq!(ui.count("taken"), 0, "без выбранного заказа кнопки нет");

    let o6 = ui.id_of("o6");
    ui.game
        .world
        .set_flag(o6, ui.game.properties.resolve("selected").unwrap(), true);
    ui.click(350.0, 130.0);
    assert!(
        ui.flag(o6, "taken"),
        "кнопка передала функции выбранный заказ"
    );
    assert_eq!(ui.count("taken"), 1);
}

#[test]
fn a_wrapped_label_is_marked_as_wrapped() {
    let ui = load();
    let Element::Label { wrap, text, .. } = &ui.config.screens[0].elements[2] else {
        panic!("третий элемент — надпись");
    };
    assert!(*wrap);
    assert!(
        matches!(text[0], TextPart::Own(_)),
        "{{title}} — свойство объекта элемента"
    );
    let Element::Label { wrap, .. } = &ui.config.screens[0].elements[6] else {
        panic!("седьмой элемент — надпись");
    };
    assert!(!wrap, "без wrap — одна строка");
}

// -------------------------------------------------------------------------------------------
// Список
// -------------------------------------------------------------------------------------------

#[test]
fn rows_follow_sort_by_and_descending_with_missing_values_last_and_ties_by_number() {
    let ui = load();
    let o = |name: &str| ui.id_of(name);
    assert_eq!(
        all_rows(&ui, false, true),
        vec![
            o("o1"),
            o("o5"),
            o("o2"),
            o("o4"),
            o("o6"),
            o("o0"),
            o("o3")
        ],
        "по возрастанию; у «Лука» приоритета нет — в конце; «Сыр» перед «Рыбой» по номеру"
    );
    assert_eq!(
        all_rows(&ui, true, true),
        vec![
            o("o0"),
            o("o6"),
            o("o2"),
            o("o4"),
            o("o5"),
            o("o1"),
            o("o3")
        ],
        "по убыванию; без свойства всё равно в конце"
    );
    assert_eq!(
        all_rows(&ui, false, false),
        (0..7).collect::<Vec<u32>>(),
        "без sort_by — по номеру"
    );
}

#[test]
fn a_list_shows_whole_rows_and_a_scroll_bar_of_the_visible_share() {
    let ui = load();
    // Пример требования 11: высота 300, строки по 64, промежуток 4 — видно ⌊304 / 68⌋ = 4 строки.
    let (count, bars) = layout_parts_count(&ui);
    assert_eq!(visible_rows(&ui, 0).len(), 4);
    assert_eq!(
        count,
        4 * 5 + 1 + 3,
        "четыре строки по пять элементов, полоса, две кнопки и надпись"
    );
    let [x, y, width, height] = bars[0];
    assert_eq!(width, 4.0, "полоса шириной 4 точки");
    assert_eq!(x, 300.0 - 4.0, "у правого края списка");
    assert_eq!(y, 0.0);
    assert!(
        (height - 300.0 * 4.0 / 7.0).abs() < 1e-3,
        "высота — доля видимых строк, 4/7: {height}"
    );
}

#[test]
fn the_wheel_moves_the_rows_one_per_hundred_points_and_the_bar_with_them() {
    let mut ui = load();
    let sorted = all_rows(&ui, false, true);
    assert_eq!(visible_rows(&ui, 0), sorted[0..4].to_vec());

    assert!(
        ui.wheel(20.0, 20.0, 100.0),
        "колесо над списком отдано списку"
    );
    assert_eq!(visible_rows(&ui, 0), sorted[1..5].to_vec());
    let (_, bars) = layout_parts_count(&ui);
    assert!(
        (bars[0][1] - 300.0 / 7.0).abs() < 1e-3,
        "полоса стоит на месте видимых строк"
    );

    ui.wheel(20.0, 20.0, 100.0 * 10.0);
    assert_eq!(
        visible_rows(&ui, 0),
        sorted[3..7].to_vec(),
        "до последних четырёх строк"
    );
    let (_, bars) = layout_parts_count(&ui);
    assert!((bars[0][1] - 300.0 * 3.0 / 7.0).abs() < 1e-3);

    ui.wheel(20.0, 20.0, -100.0 * 10.0);
    assert_eq!(
        visible_rows(&ui, 0),
        sorted[0..4].to_vec(),
        "и обратно к первой"
    );

    assert!(
        !ui.wheel(600.0, 20.0, 100.0),
        "над миром колесо списку не нужно"
    );
}

#[test]
fn sixty_points_of_the_wheel_move_nothing_and_a_hundred_move_one_row() {
    let mut ui = load();
    let sorted = all_rows(&ui, false, true);
    ui.wheel(20.0, 20.0, 60.0);
    assert_eq!(visible_rows(&ui, 0), sorted[0..4].to_vec());
    ui.wheel(20.0, 20.0, 40.0);
    assert_eq!(visible_rows(&ui, 0), sorted[1..5].to_vec());
}

#[test]
fn a_list_takes_the_wheel_away_from_the_camera_even_when_all_rows_fit() {
    let mut ui = load();
    let removed: Vec<u32> = ui
        .game
        .world
        .ids()
        .filter(|id| (3..7).contains(id))
        .collect();
    for id in removed {
        ui.game.world.delete(id);
    }
    assert_eq!(visible_rows(&ui, 0).len(), 3);
    assert!(ui.wheel(20.0, 20.0, 100.0));
    let (_, bars) = layout_parts_count(&ui);
    assert!(bars.is_empty(), "все строки видны — полосы нет");
    assert!(
        engine::core::screens::over_interface(
            &ui.config,
            &ui.state,
            &ui.game.world,
            VIEWPORT,
            [20.0, 290.0]
        ),
        "место списка не отдаётся камере"
    );
    assert!(
        !engine::core::screens::over_interface(
            &ui.config,
            &ui.state,
            &ui.game.world,
            VIEWPORT,
            [700.0, 500.0]
        ),
        "над миром камера работает"
    );
}

#[test]
fn the_scroll_follows_the_rows_stays_between_calls_and_a_new_game_resets_it() {
    let mut ui = load();
    ui.wheel(20.0, 20.0, 300.0);
    let sorted = all_rows(&ui, false, true);
    assert_eq!(visible_rows(&ui, 0), sorted[3..7].to_vec());

    // Строки убыли — прокрутка прижимается к концу.
    ui.game.world.delete(ui.id_of("o6"));
    ui.game.world.delete(ui.id_of("o0"));
    ui.game.world.delete(ui.id_of("o3"));
    let rest = all_rows(&ui, false, true);
    assert_eq!(rest.len(), 4);
    assert_eq!(visible_rows(&ui, 0), rest, "четыре строки влезают целиком");

    // Прокрутка помнится у списка своего экрана и не уходит со сменой экрана.
    let mut ui = load();
    ui.wheel(20.0, 20.0, 200.0);
    ui.show("pause");
    assert_eq!(
        ui.state.list_offset(1, 0),
        0.0,
        "у списка паузы своя прокрутка"
    );
    ui.show("main");
    assert_eq!(ui.state.list_offset(0, 0), 2.0);

    let main = ui.screen_id("main");
    engine::core::screens::apply_command(
        engine::core::screens::ButtonCommand::NewGame(main, None),
        &mut ui.game,
        &ui.config,
        &mut ui.state,
    );
    assert_eq!(
        ui.state.list_offset(0, 0),
        0.0,
        "новая партия сбрасывает прокрутку"
    );
}

// -------------------------------------------------------------------------------------------
// Кнопка с кодом
// -------------------------------------------------------------------------------------------

/// Точка щелчка по кнопке `slot`-й строки: «Взять» — 0, «Готово» — 1.
fn row_button(slot: usize, which: usize) -> (f32, f32) {
    (180.0 + 70.0 * which as f32, slot as f32 * 68.0 + 18.0)
}

#[test]
fn run_calls_the_function_with_the_object_of_the_row_and_its_writes_are_seen_at_once() {
    let mut ui = load();
    let sorted = all_rows(&ui, false, true);
    let (x, y) = row_button(2, 0);
    ui.click(x, y);
    assert!(
        ui.flag(sorted[2], "taken"),
        "функция получила объект третьей строки"
    );
    assert_eq!(ui.count("taken"), 1);
    assert_eq!(ui.game.step_count(), 0, "работает между шагами");
    assert_eq!(
        visible_rows(&ui, 0),
        vec![sorted[0], sorted[1], sorted[3], sorted[4]],
        "взятый заказ ушёл из отбора уже в этом кадре"
    );
}

#[test]
fn deletions_of_the_function_are_executed_right_after_it() {
    let mut ui = load();
    let sorted = all_rows(&ui, false, true);
    let (x, y) = row_button(1, 1);
    ui.click(x, y);
    assert!(
        !ui.game.world.is_alive(sorted[1]),
        "заказ удалён сразу после вызова"
    );
    assert_eq!(ui.game.world.alive_count(), 7);
    assert_eq!(
        visible_rows(&ui, 0),
        vec![sorted[0], sorted[2], sorted[3], sorted[4]]
    );
}

#[test]
fn buttons_work_on_a_paused_screen_where_the_world_stands() {
    let mut ui = load();
    ui.show("pause");
    assert!(!ui.state.is_live(&ui.config));
    ui.tick(1.0);
    assert_eq!(ui.game.step_count(), 0, "мир стоит");
    let sorted = all_rows(&ui, false, true);
    let (x, y) = row_button(0, 0);
    ui.click(x, y);
    assert!(ui.flag(sorted[0], "taken"), "кнопка работает на паузе");
    let (x, y) = row_button(0, 1);
    ui.click(x, y);
    assert!(!ui.game.world.is_alive(sorted[1]));
    assert_eq!(ui.game.step_count(), 0, "мир по-прежнему стоит");
}

#[test]
fn a_screen_key_runs_a_function_without_an_object_when_released() {
    let mut ui = load();
    ui.key("KeyP");
    let game = ui.id_of("game");
    assert_eq!(
        ui.number(game, "score"),
        Some(7.0),
        "count() записал число заказов"
    );
}

#[test]
fn a_button_pressed_in_a_row_that_moved_before_the_release_does_nothing() {
    let mut ui = load();
    let sorted = all_rows(&ui, false, true);
    let (x, y) = row_button(0, 0);
    ui.press(x, y);
    ui.game.world.delete(sorted[0]);
    ui.release();
    assert_eq!(
        ui.count("taken"),
        0,
        "под указателем теперь кнопка другой строки"
    );

    let (x, y) = row_button(0, 0);
    ui.press(x, y);
    ui.release();
    assert_eq!(
        ui.count("taken"),
        1,
        "а нажатая и отпущенная на месте работает"
    );
}

#[test]
fn an_error_in_the_function_stops_the_game_and_names_the_screen_and_the_button() {
    let mut ui = load();
    ui.click(350.0, 210.0);
    let error = ui.game.code_error().expect("ошибка кода");
    assert_eq!(error.function.as_deref(), Some("boom"));
    assert!(error.message.contains("плохо"), "{error:?}");
    let button = error.button.as_deref().expect("называет кнопку");
    assert!(
        button.contains("экран \"main\"") && button.contains("кнопка \"Сломать\""),
        "{button}"
    );
    assert_eq!(error.rule, None, "вместо правила — кнопка");
    assert!(!ui.game.is_running());
}

#[test]
fn an_endless_function_runs_into_its_own_limit_and_stops_the_game() {
    let mut ui = load();
    ui.click(350.0, 250.0);
    let error = ui.game.code_error().expect("ошибка кода");
    assert!(error.message.contains("предел операций"), "{error:?}");
    assert!(error.button.as_deref().unwrap_or("").contains("Зациклить"));
}

#[test]
fn each_call_gets_its_own_limit_like_each_step() {
    let mut ui = load();
    // Две подряд тяжёлых, но не бесконечных работы не складываются в один предел: у каждой он свой.
    let sorted = all_rows(&ui, false, true);
    for slot in 0..3 {
        let (x, y) = row_button(0, 0);
        ui.click(x, y);
        assert!(ui.game.code_error().is_none(), "вызов {slot}");
    }
    assert_eq!(ui.count("taken"), 3);
    assert!(ui.game.world.is_alive(sorted[0]));
}

#[test]
fn run_does_nothing_without_a_world() {
    let mut ui = load();
    let quit = engine::core::screens::ButtonCommand::Quit;
    engine::core::screens::apply_command(quit, &mut ui.game, &ui.config, &mut ui.state);
    assert!(!ui.game.has_world());
    ui.key("KeyP");
    assert!(ui.game.code_error().is_none());
}

// -------------------------------------------------------------------------------------------
// Проверка перед запуском
// -------------------------------------------------------------------------------------------

fn list_with(extra: &str, row: &str) -> String {
    format!(
        r##"{{"screens":[{{"name":"main","world_runs":true,"elements":[
    {{"kind":"list","anchor":"top_left","offset":[0,0],"size":[300,300],
      "for":{{"has":["order"]}},{extra}"row_height":64,"row":[{row}]}}
]}}]}}"##
    )
}

const PLAIN_ROW: &str = r##"{"kind":"panel","offset":[0,0],"size":[280,64],"color":"#ffffff"}"##;

fn errors_of(screens: &str, code: Option<&str>) -> Vec<String> {
    let mut files = Files::new(PROPS, SCENE, screens);
    if let Some(code) = code {
        files = files.with_code(code);
    }
    match files.try_load() {
        Ok(_) => Vec::new(),
        Err(failure) => failure.errors.into_iter().map(|e| e.message).collect(),
    }
}

fn expect_error(screens: &str, code: Option<&str>, fragment: &str) {
    let errors = errors_of(screens, code);
    assert!(
        errors.iter().any(|message| message.contains(fragment)),
        "ожидалась ошибка с «{fragment}», есть {errors:?}"
    );
}

#[test]
fn a_list_needs_for_row_height_and_row() {
    let no_for = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"list","anchor":"top_left","size":[300,300],"row_height":64,"row":[]}]}]}"##;
    expect_error(no_for, None, "отсутствует обязательная настройка \"for\"");
    let no_height = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"list","anchor":"top_left","size":[300,300],"for":{"has":["order"]},"row":[]}]}]}"##;
    expect_error(
        no_height,
        None,
        "отсутствует обязательная настройка \"row_height\"",
    );
    let no_row = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"list","anchor":"top_left","size":[300,300],"for":{"has":["order"]},"row_height":64}]}]}"##;
    expect_error(no_row, None, "отсутствует обязательная настройка \"row\"");
}

#[test]
fn row_height_gap_and_sort_by_have_to_make_sense() {
    let zero = list_with("", PLAIN_ROW).replace("\"row_height\":64", "\"row_height\":0");
    expect_error(
        &zero,
        None,
        "свойство \"row_height\" должно быть больше нуля",
    );
    let negative_gap = list_with("\"gap\":-1,", PLAIN_ROW);
    expect_error(
        &negative_gap,
        None,
        "свойство \"gap\" не может быть меньше нуля",
    );
    let text_sort = list_with("\"sort_by\":\"title\",", PLAIN_ROW);
    expect_error(
        &text_sort,
        None,
        "свойство \"sort_by\" должно называть свойство вида number, time или timer",
    );
    let unknown_sort = list_with("\"sort_by\":\"nothing\",", PLAIN_ROW);
    expect_error(&unknown_sort, None, "неизвестное свойство \"nothing\"");
    let good = list_with("\"sort_by\":\"prio\",\"gap\":2,", PLAIN_ROW);
    assert_eq!(errors_of(&good, None), Vec::<String>::new());
}

#[test]
fn a_row_holds_only_panels_labels_and_buttons_without_anchor_or_for() {
    let nested = list_with(
        "",
        r##"{"kind":"list","offset":[0,0],"size":[100,50],"for":{"has":["order"]},"row_height":10,"row":[]}"##,
    );
    expect_error(&nested, None, "в строке списка нет вложенных списков");
    let unknown = list_with("", r##"{"kind":"bar","offset":[0,0],"size":[100,50]}"##);
    expect_error(&unknown, None, "ожидался panel, label или button");
    let anchored = list_with(
        "",
        r##"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[10,10],"color":"#ffffff"}"##,
    );
    expect_error(&anchored, None, "у элемента строки списка нет \"anchor\"");
    let filtered = list_with(
        "",
        r##"{"kind":"panel","offset":[0,0],"size":[10,10],"color":"#ffffff","for":{"has":["order"]}}"##,
    );
    expect_error(&filtered, None, "у элемента строки списка нет \"for\"");
}

#[test]
fn a_bare_substitution_needs_an_object_of_the_element_and_a_declared_property() {
    let outside = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"label","anchor":"top_left","size":[100,20],"text":"{title}","font":"ui"}]}]}"##;
    expect_error(outside, None, "без имени объекта");
    let unknown_in_row = list_with(
        "",
        r##"{"kind":"label","offset":[0,0],"size":[100,20],"text":"{nothing}","font":"ui"}"##,
    );
    expect_error(&unknown_in_row, None, "неизвестное свойство \"nothing\"");
    let in_row = list_with(
        "",
        r##"{"kind":"label","offset":[0,0],"size":[100,20],"text":"{title}","font":"ui"}"##,
    );
    assert_eq!(errors_of(&in_row, None), Vec::<String>::new());
    let with_for = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"label","anchor":"top_left","size":[100,20],"text":"{title}","font":"ui",
         "for":{"has":["order"]}}]}]}"##;
    assert_eq!(errors_of(with_for, None), Vec::<String>::new());
}

#[test]
fn run_needs_a_code_file_and_a_function_that_exists() {
    let button = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"button","anchor":"top_left","size":[100,20],"text":"Ой","font":"ui","color":"#ffffff",
         "on_click":["run","nope"]}]}]}"##;
    expect_error(button, None, "run в игре без files.code");
    expect_error(
        button,
        Some("function other() end"),
        "функции \"nope\" в коде нет",
    );
    assert_eq!(
        errors_of(button, Some("function nope() end")),
        Vec::<String>::new()
    );
    let key = r##"{"screens":[{"name":"main","world_runs":true,"keys":{"KeyP":["run","nope"]},
        "elements":[]}]}"##;
    expect_error(
        key,
        Some("function other() end"),
        "функции \"nope\" в коде нет",
    );
}

#[test]
fn wrap_is_true_or_false() {
    let label = |wrap: &str| {
        format!(
            r##"{{"screens":[{{"name":"main","world_runs":true,"elements":[
        {{"kind":"label","anchor":"top_left","size":[100,20],"text":"x","font":"ui","wrap":{wrap}}}]}}]}}"##
        )
    };
    expect_error(
        &label("\"yes\""),
        None,
        "свойство \"wrap\" должно быть true или false",
    );
    assert_eq!(errors_of(&label("true"), None), Vec::<String>::new());
}

#[test]
fn warnings_name_an_empty_selector_and_a_row_element_outside_its_row() {
    let props = r#"{"properties":{"order":"flag","title":"text","prio":"number","note":"text",
        "taken":"flag","selected":"flag","score":"number","ghost":"flag"}}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"list","anchor":"top_left","size":[300,300],"for":{"has":["ghost"]},
         "row_height":64,"row":[
           {"kind":"panel","offset":[250,0],"size":[100,64],"color":"#ffffff"}]},
        {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff",
         "for":{"has":["order"],"without":["ghost"]}}
    ]}]}"##;
    let (_, _, warnings) = Files::new(props, SCENE, screens)
        .try_load()
        .expect("с предупреждениями грузится");
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.path == "screens[0] → elements[0] → for"
                && w.message.contains("заведомо не подходит")),
        "{messages:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.path == "screens[0] → elements[0] → row[0]"
                && w.message.contains("выходит за строку")),
        "{messages:?}"
    );
    assert!(
        !warnings.iter().any(|w| w.path.contains("elements[1]")),
        "подходящий отбор без предупреждения: {messages:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Запись партии
// -------------------------------------------------------------------------------------------

fn world_snapshot(ui: &Ui) -> Vec<(u32, bool, Option<String>)> {
    ui.game
        .world
        .ids()
        .map(|id| (id, ui.flag(id, "taken"), ui.text(id, "title")))
        .collect()
}

#[test]
fn a_replay_with_run_buttons_gives_the_same_partiya_step_for_step() {
    let mut ui = load();
    let mut session = PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    let sorted = all_rows(&ui, false, true);
    let frame = 1.0 / 60.0;
    let live = |ui: &mut Ui, session: &mut PlaySession, clicks: &[(f32, f32)]| {
        for &(x, y) in clicks {
            ui.queue.push_mouse_move(x, y);
            ui.queue.push_mouse_down();
            ui.queue.push_mouse_up();
        }
        session.tick_live(
            &mut ui.queue,
            &mut ui.mouse,
            &mut ui.runner,
            &mut ui.game,
            &ui.config,
            &mut ui.state,
            VIEWPORT,
            frame,
            &[],
        );
    };
    live(&mut ui, &mut session, &[]);
    live(&mut ui, &mut session, &[row_button(2, 0)]);
    live(&mut ui, &mut session, &[]);
    live(&mut ui, &mut session, &[row_button(0, 1), row_button(0, 1)]);
    live(&mut ui, &mut session, &[]);
    let steps = ui.game.session_step_count();
    assert_eq!(steps, 5);
    session.end(&mut ui.game);
    let text = session.recording_text(&ui.game);
    assert!(
        text.contains(&format!("[\"run\",\"take\",{}]", sorted[2])),
        "запись называет функцию и номер объекта: {text}"
    );
    let expected = world_snapshot(&ui);
    assert_eq!(ui.count("taken"), 1);
    assert!(!ui.game.world.is_alive(sorted[0]) && !ui.game.world.is_alive(sorted[1]));

    let mut again = load();
    let mut replay =
        PlaySession::begin_replay(&text, &mut again.game, &again.config, &mut again.state)
            .expect("запись читается");
    while again.game.session_step_count() < steps {
        replay.step_once(
            &mut again.queue,
            &mut again.mouse,
            &mut again.game,
            &again.config,
            &mut again.state,
            VIEWPORT,
            &[],
        );
    }
    assert_eq!(world_snapshot(&again), expected, "повтор дал ту же партию");
}

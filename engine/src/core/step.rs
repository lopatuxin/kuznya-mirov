use std::collections::HashSet;

use super::code::{CodeError, Runner as CodeRunner};
use super::footprint::Footprint;
use super::grid::{Rect, SpatialGrid, largest_dimension};
use super::input::{KeyAction, KeyEvent};
use super::keys::EditValue;
use super::math3::Vec3;
use super::pathfind::{self, WalkCaches};
use super::property::{self, PropertyId, PropertyTable};
use super::report::{DeleteCause, RuleFired, StepReportBuilder};
use super::rng::Rng;
use super::rules::{
    CollideEffect, CommonAction, Condition, NumberExpr, Outcome, Rule, Selector, SetValue,
    ShiftSpec, SpawnPlace, SpawnVariant, TemplateValue, TurnDir, TurnSpec,
};
use super::scene::{self, SceneConfig};
use super::sound::SoundMarks;
use super::value::{Rotation, Value, Vec2};
use super::world::World;

/// «Код игры»: что `["run", "<функция>"]` требует на месте вызова — исполнитель на партию и то,
/// чем код смеет пользоваться помимо мира: счётчик случайности и `messages` для `print`.
/// `None`, когда у игры нет `files.code` — тогда `run` сам по себе ошибка (проверяется при
/// загрузке, но и здесь на всякий случай, а не паникой).
pub struct CodeEnv<'a> {
    pub runner: &'a mut CodeRunner,
    pub messages: &'a mut Vec<String>,
}

fn run_missing() -> CodeError {
    CodeError {
        message: "run в игре без files.code".to_string(),
        line: None,
        function: None,
        rule: None,
        step: None,
    }
}

/// The one place `["run", "<функция>"]` actually calls into `code::Runner` — every action-list
/// executor below goes through this rather than calling `CodeEnv::runner.run` itself, so the
/// «run в игре без files.code» fallback and the marks re-borrow (`Runner::run` takes `SoundMarks`
/// by value, callers here only ever hold `&mut SoundMarks`) live in one place. `rng` is threaded
/// in separately, not carried on `CodeEnv`: it is «Исполнение игры»'s one counter shared with
/// `random_cell`/`pick_one`, so a caller that also needs it for those keeps a single `&mut Rng`
/// rather than two live borrows of the same field. `rule` is this rule's `rules[N]` label —
/// «Код игры» → требование 20: the text of a runtime code error names the rule that called it,
/// and this is the one place that knows both the rule's own index and its call into `Runner::run`.
#[allow(clippy::too_many_arguments)]
fn run_code(
    code: &mut Option<CodeEnv<'_>>,
    world: &mut World,
    rng: &mut Rng,
    deletes: &mut Vec<u32>,
    moved: &mut [bool],
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    rule: &str,
    function: &str,
    args: &[u32],
) -> Result<(), CodeError> {
    let Some(env) = code else {
        let mut err = run_missing();
        err.rule = Some(rule.to_string());
        err.function = Some(function.to_string());
        return Err(err);
    };
    let deletes_before = deletes.len();
    env.runner
        .run(
            function,
            args,
            world,
            rng,
            deletes,
            moved,
            marks.reborrow(),
            env.messages,
        )
        .map_err(|mut err| {
            err.rule = Some(rule.to_string());
            err
        })?;
    // «Редактор», требование 23: a delete this call queued (`obj.delete()` in Lua) is attributed
    // to the function and the rule that called it, distinct from the rule's own direct effects.
    if let Some(r) = report.as_mut() {
        for &id in &deletes[deletes_before..] {
            let name = world.text(id, property::NAME).map(str::to_string);
            r.deletes.push((
                id,
                name,
                DeleteCause::Code {
                    function: function.to_string(),
                    rule: rule.to_string(),
                },
            ));
        }
    }
    Ok(())
}

pub fn selector_matches(selector: &Selector, world: &World, id: u32) -> bool {
    world.has_all(id, &selector.has) && world.has_none(id, &selector.without)
}

/// «Трёхмерная сцена» → `rotation`: место объекта на земле — в плоской сцене его прямоугольник, в
/// трёхмерной — повёрнутый на `rotation`.
fn footprint_of(world: &World, id: u32) -> Option<Footprint> {
    let position = world.vec2(id, property::POSITION)?;
    let size = world.vec2(id, property::SIZE)?;
    Some(if world.three_d() {
        Footprint::rotated(position, size, world.rotation(id, property::ROTATION))
    } else {
        Footprint::flat(position, size)
    })
}

/// «Формат игры» / «Мышь в мире», требование 18: an object's own midpoint clamped so it never
/// leaves the scene, centered under a point — shared by `apply_follow_mouse` and by a `"cursor"`
/// record onto `position`.
fn fit_center_in_scene(point: f64, len: f64, scene_len: u32) -> f64 {
    (point - len / 2.0).clamp(0.0, (scene_len as f64 - len).max(0.0))
}

/// «Мышь в мире», требования 17–18: resolves one record's value — a plain constant, or `"cursor"`
/// (`EditValue::Cursor`), the point under the cursor when this press/release/click arrived.
/// `None` — either the point isn't known yet (требование, «Крайние случаи»: мышь ещё не
/// двигалась) — this one record is skipped, the rest of the same key/`on_click` still apply. A
/// `Cursor` record only ever names a `Vec2` property — checked at load time — so `prop`'s kind is
/// assumed `Vec2` here without checking again.
fn resolve_edit_value(
    value: &EditValue,
    cursor: Option<Vec2>,
    prop: PropertyId,
    world: &World,
    id: u32,
    scene: &SceneConfig,
) -> Option<Value> {
    match value {
        EditValue::Const(v) => Some(v.clone()),
        EditValue::Cursor => {
            let cursor = cursor?;
            let point = if scene.is_3d() {
                scene.clamp_point(cursor)
            } else {
                cursor
            };
            if prop == property::POSITION {
                let size = world.vec2(id, property::SIZE)?;
                Some(Value::Vec2([
                    fit_center_in_scene(point[0], size[0], scene.width),
                    fit_center_in_scene(point[1], size[1], scene.height),
                ]))
            } else {
                Some(Value::Vec2(point))
            }
        }
    }
}

/// «Мышь в мире», требование 19: the topmost live object with `on_click` whose rectangle
/// contains `point` gets its own records applied — «keys, потом on_click» (требование 20) is
/// already the caller's own ordering, this only runs once, after every key edit of the same
/// batch.
fn apply_on_click(world: &mut World, scene: &SceneConfig, point: Vec2, eye: Option<Vec3>) {
    let target = if scene.is_3d() {
        scene::on_click_target_ray(world, scene, eye, point)
    } else {
        scene::on_click_target(world, scene, point)
    };
    let Some(id) = target else {
        return;
    };
    let Some(edits) = world.on_click(id, property::ON_CLICK).map(<[_]>::to_vec) else {
        return;
    };
    for edit in edits {
        if let Some(value) =
            resolve_edit_value(&edit.value, Some(point), edit.property, world, id, scene)
        {
            world.set_value(id, edit.property, &value);
        }
    }
}

/// Stage 2: press/release events turn into property writes named by each object's `keys` table —
/// applied in `events`' own order, not grouped by press-then-release, so a release and a press of
/// the same key landing in the same real-time gap leave the object in the state the later of the
/// two actually calls for. «Мышь в мире», требования 17, 19–20: `cursor` resolves any `"cursor"`
/// record in that same batch (a single point for the whole call, same simplification
/// `apply_follow_mouse` already makes); a `MouseLeft` press in `events` fires `on_click` right
/// after every key edit has applied.
pub fn apply_input(
    world: &mut World,
    events: &[KeyEvent],
    cursor: Option<Vec2>,
    eye: Option<Vec3>,
    scene: &SceneConfig,
) {
    let mut edits: Vec<(u32, PropertyId, EditValue)> = Vec::new();
    for event in events {
        for id in world.ids() {
            let Some(table) = world.keys(id, property::KEYS) else {
                continue;
            };
            let Some(binding) = table.get(&event.code) else {
                continue;
            };
            let side = match event.action {
                KeyAction::Press => &binding.press,
                KeyAction::Release => &binding.release,
            };
            edits.extend(side.iter().map(|e| (id, e.property, e.value.clone())));
        }
    }
    for (id, prop, edit_value) in edits {
        if let Some(value) = resolve_edit_value(&edit_value, cursor, prop, world, id, scene) {
            world.set_value(id, prop, &value);
        }
    }
    if let Some(point) = cursor {
        for event in events {
            if event.code == "MouseLeft" && event.action == KeyAction::Press {
                apply_on_click(world, scene, point, eye);
            }
        }
    }
}

/// Stage 2, after key bindings: «Курсор в мире», требование 26–27 — every object with
/// `follow_mouse` is set centered under `cursor` along its named axes, only when `cursor` is
/// `Some` (this step actually got a fresh cursor position — see `Game::take_input_snapshot`).
/// `None` — the cursor hasn't moved since the previous step, or has never moved at all — leaves
/// every `follow_mouse` object untouched. Never marks a move for `after_move_of`, same as a key's
/// own `position` write.
pub fn apply_follow_mouse(world: &mut World, cursor: Option<Vec2>, scene: &SceneConfig) {
    let Some(cursor) = cursor else { return };
    for id in world.ids().collect::<Vec<_>>() {
        let Some(axis) = world.follow_mouse(id, property::FOLLOW_MOUSE) else {
            continue;
        };
        let (Some(pos), Some(size)) = (
            world.vec2(id, property::POSITION),
            world.vec2(id, property::SIZE),
        ) else {
            continue;
        };
        let mut new_pos = pos;
        if axis.affects_x() {
            new_pos[0] = fit_center_in_scene(cursor[0], size[0], scene.width);
        }
        if axis.affects_y() {
            new_pos[1] = fit_center_in_scene(cursor[1], size[1], scene.height);
        }
        world.set_vec2(id, property::POSITION, new_pos);
    }
}

/// Stage 3: lifetime, grid-hop and `timer` countdowns. Returns objects whose lifetime just
/// expired.
pub fn tick_counters(
    world: &mut World,
    grid_hop_ready: &mut [bool],
    properties: &PropertyTable,
) -> Vec<u32> {
    let timer_props: Vec<PropertyId> = properties
        .iter()
        .filter(|(_, def)| def.kind == super::value::PropKind::Timer)
        .map(|(id, _)| id)
        .collect();
    let mut expired = Vec::new();
    for id in world.ids().collect::<Vec<_>>() {
        if let Some(lifetime) = world.time(id, property::LIFETIME) {
            let remaining = lifetime - 1;
            world.set_time(id, property::LIFETIME, remaining);
            if remaining <= 0 {
                expired.push(id);
            }
        }
        if world.has(id, property::GRID) {
            let counter = world.grid_counter(id) - 1;
            world.set_grid_counter(id, counter);
            grid_hop_ready[id as usize] = counter <= 0;
        }
        for &prop in &timer_props {
            if world.has(id, prop) {
                world.tick_timer(id, prop);
            }
        }
    }
    expired
}

fn apply_move_rule(
    world: &mut World,
    for_: &Selector,
    grid_hop_ready: &mut [bool],
    moved: &mut [bool],
    report: &mut Option<StepReportBuilder>,
    rule_index: usize,
) {
    let mut moved_objects: Vec<u32> = Vec::new();
    for id in world.ids().collect::<Vec<_>>() {
        if !selector_matches(for_, world, id) {
            continue;
        }
        let (Some(pos), Some(vel)) = (
            world.vec2(id, property::POSITION),
            world.vec2(id, property::VELOCITY),
        ) else {
            continue;
        };
        let new_pos = if let Some(spec) = world.grid(id, property::GRID) {
            if !grid_hop_ready[id as usize] {
                continue;
            }
            let hopped = [pos[0] + vel[0], pos[1] + vel[1]];
            if hopped != pos {
                // «Исполнение игры»: счётчик хода снова равен интервалу сразу после
                // прыжка — нулевая скорость прыжком не считается, счётчик не трогаем.
                grid_hop_ready[id as usize] = false;
                world.set_grid_counter(id, spec.interval_steps);
            }
            hopped
        } else {
            [pos[0] + vel[0] / 60.0, pos[1] + vel[1] / 60.0]
        };
        if new_pos != pos {
            world.set_vec2(id, property::POSITION, new_pos);
            moved[id as usize] = true;
            if report.is_some() {
                moved_objects.push(id);
            }
        }
    }
    if let Some(r) = report.as_mut()
        && !moved_objects.is_empty()
    {
        r.fired.push(RuleFired::Move {
            rule: format!("rules[{rule_index}]"),
            objects: moved_objects,
        });
    }
}

/// General condition evaluator — «Правила игры» → «Условия», требования 4–9: shared by `check`
/// and `delete`'s per-object leaves (`id: Some(...)`) and `spawn`'s parent-less global tree
/// (`id: None`, `Compare`/`OutsideScene` never appear in it — checked at load time, so returning
/// `false` for them here is unreachable in practice, not a silent wrong answer). `FewerThan`/
/// `AfterMoveOf` are already global facts, the same value regardless of `id` — `count_selector`
/// lets stage 4 (excludes only already-deleted objects) and stage 7 (also counts pending creates)
/// share this one evaluator with their own counting rule.
pub fn condition_holds(
    condition: &Condition,
    world: &World,
    id: Option<u32>,
    scene: &SceneConfig,
    moved: &[bool],
    count_selector: &dyn Fn(&Selector) -> u32,
) -> bool {
    match condition {
        Condition::Compare { prop, op, value } => id.is_some_and(|id| {
            world
                .number_like(id, *prop)
                .is_some_and(|lhs| op.apply(lhs, *value))
        }),
        Condition::OutsideScene => id.is_some_and(|id| {
            footprint_of(world, id).is_some_and(|place| outside_scene(&place, scene))
        }),
        Condition::FewerThan { count, of } => count_selector(of) < *count,
        Condition::AfterMoveOf { of } => world
            .ids()
            .any(|oid| moved[oid as usize] && selector_matches(of, world, oid)),
        Condition::All(list) => list
            .iter()
            .all(|c| condition_holds(c, world, id, scene, moved, count_selector)),
        Condition::Any(list) => list
            .iter()
            .any(|c| condition_holds(c, world, id, scene, moved, count_selector)),
        Condition::Not(inner) => !condition_holds(inner, world, id, scene, moved, count_selector),
    }
}

fn count_selector_excluding(world: &World, selector: &Selector, deleted: &[u32]) -> u32 {
    world
        .ids()
        .filter(|id| !deleted.contains(id))
        .filter(|&id| selector_matches(selector, world, id))
        .count() as u32
}

/// «shift»/«turn», требование 14: every object `group` picks out that carries `position` and
/// `size`, without a delete request, in ascending id order.
fn selected_group(world: &World, group: &Selector, deleted: &[u32]) -> Vec<u32> {
    world
        .ids()
        .filter(|id| !deleted.contains(id))
        .filter(|&id| selector_matches(group, world, id))
        .filter(|&id| world.has(id, property::POSITION) && world.has(id, property::SIZE))
        .collect()
}

struct GroupState {
    id: u32,
    position: Vec2,
    size: Vec2,
    rotation: Option<Rotation>,
}

fn snapshot_group(world: &World, ids: &[u32]) -> Vec<GroupState> {
    ids.iter()
        .map(|&id| GroupState {
            id,
            position: world
                .vec2(id, property::POSITION)
                .expect("selected_group filtered"),
            size: world
                .vec2(id, property::SIZE)
                .expect("selected_group filtered"),
            rotation: world.rotation(id, property::ROTATION),
        })
        .collect()
}

fn restore_group(world: &mut World, snapshot: &[GroupState]) {
    for s in snapshot {
        world.set_vec2(s.id, property::POSITION, s.position);
        world.set_vec2(s.id, property::SIZE, s.size);
        match s.rotation {
            Some(r) => world.set_rotation(s.id, property::ROTATION, r),
            None => world.clear_property(s.id, property::ROTATION),
        }
    }
}

/// «shift»/«turn», требование 15, 170: whether any object of `group` now overlaps an object
/// picked out by `blocked_by` that isn't itself part of the group and carries no delete request —
/// touching at an edge doesn't count (`Rect::overlap` is already strict). Built through
/// `SpatialGrid` (the same structure stage 5's collision search uses) rather than a pairwise scan
/// of every world object — «Нефункциональные требования»: a shift of a four-cube group must not
/// slow down on a scene with thousands of objects.
fn group_overlaps_blocker(
    world: &World,
    group: &[u32],
    blocked_by: &Selector,
    deleted: &[u32],
    grid: &mut SpatialGrid,
) -> bool {
    let mut objects: Vec<(u32, Footprint)> = group
        .iter()
        .filter_map(|&id| footprint_of(world, id).map(|f| (id, f)))
        .collect();
    let group_len = objects.len();
    for id in world.ids() {
        if deleted.contains(&id) || group.contains(&id) {
            continue;
        }
        if selector_matches(blocked_by, world, id)
            && let Some(f) = footprint_of(world, id)
        {
            objects.push((id, f));
        }
    }
    if objects.len() == group_len {
        return false;
    }
    overlapping_pairs(&objects, grid)
        .into_iter()
        .any(|(a, b)| group.contains(&a) != group.contains(&b))
}

/// Пары мест, что строго пересекаются: сначала по охватывающим прямоугольникам через
/// `SpatialGrid` (как искала прежняя плоская сцена), затем — только когда среди мест есть
/// повёрнутое — точно, по разделяющим осям. Без поворотов ответ тот же, что давал один `Rect::overlap`.
fn overlapping_pairs(objects: &[(u32, Footprint)], grid: &mut SpatialGrid) -> Vec<(u32, u32)> {
    let boxes: Vec<(u32, Rect)> = objects.iter().map(|(id, f)| (*id, f.aabb())).collect();
    let cell_size = largest_dimension(&boxes).max(1e-3);
    let mut pairs = grid.find_pairs(cell_size, &boxes);
    if objects.iter().any(|(_, f)| f.is_oriented()) {
        let place = |id: u32| {
            objects
                .iter()
                .find(|(other, _)| *other == id)
                .map(|(_, f)| *f)
        };
        pairs.retain(|&(a, b)| match (place(a), place(b)) {
            (Some(fa), Some(fb)) => fa.overlaps(&fb),
            _ => false,
        });
    }
    pairs
}

#[allow(clippy::too_many_arguments)]
fn execute_shift(
    spec: &ShiftSpec,
    world: &mut World,
    deleted: &[u32],
    moved: &mut [bool],
    grid: &mut SpatialGrid,
    outcome: &mut Option<Outcome>,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code_deletes: &mut Vec<u32>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    run_args: &[u32],
    rule: &str,
) -> Result<(), CodeError> {
    let group = selected_group(world, &spec.group, deleted);
    if group.is_empty() {
        return Ok(());
    }
    let snapshot = snapshot_group(world, &group);
    for &id in &group {
        let p = world
            .vec2(id, property::POSITION)
            .expect("group has position");
        world.set_vec2(
            id,
            property::POSITION,
            [p[0] + spec.by[0], p[1] + spec.by[1]],
        );
    }
    let blocked = spec
        .blocked_by
        .as_ref()
        .is_some_and(|sel| group_overlaps_blocker(world, &group, sel, deleted, grid));
    if blocked {
        restore_group(world, &snapshot);
        execute_common_actions(
            world,
            &spec.if_blocked,
            outcome,
            marks,
            report,
            code_deletes,
            moved,
            code,
            rng,
            run_args,
            rule,
            deleted,
            grid,
        )?;
    } else {
        for &id in &group {
            moved[id as usize] = true;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_turn(
    spec: &TurnSpec,
    world: &mut World,
    deleted: &[u32],
    moved: &mut [bool],
    grid: &mut SpatialGrid,
    outcome: &mut Option<Outcome>,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code_deletes: &mut Vec<u32>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    run_args: &[u32],
    rule: &str,
) -> Result<(), CodeError> {
    let group = selected_group(world, &spec.group, deleted);
    if group.is_empty() {
        return Ok(());
    }
    // «turn», требование 13: «around не нашёл никого — поворота нет, if_blocked не
    // выполняется»; «нашёл нескольких — берётся меньший номер».
    let Some(around) = world
        .ids()
        .filter(|id| !deleted.contains(id))
        .filter(|&id| selector_matches(&spec.around, world, id))
        .filter(|&id| world.has(id, property::POSITION) && world.has(id, property::SIZE))
        .min()
    else {
        return Ok(());
    };
    let ap = world
        .vec2(around, property::POSITION)
        .expect("filtered above");
    let asize = world.vec2(around, property::SIZE).expect("filtered above");
    let pivot = [ap[0] + asize[0] / 2.0, ap[1] + asize[1] / 2.0];
    let dir = match spec.dir {
        TurnDir::Clockwise => 1,
        TurnDir::CounterClockwise => -1,
    };

    let snapshot = snapshot_group(world, &group);
    for &id in &group {
        let p = world
            .vec2(id, property::POSITION)
            .expect("group has position");
        let s = world.vec2(id, property::SIZE).expect("group has size");
        let center = [p[0] + s[0] / 2.0, p[1] + s[1] / 2.0];
        let d = [center[0] - pivot[0], center[1] - pivot[1]];
        // «turn», требование 13: R(dx, dy) = (−dy, dx) по часовой, (dy, −dx) против.
        let rotated = if dir == 1 {
            [-d[1], d[0]]
        } else {
            [d[1], -d[0]]
        };
        let new_center = [pivot[0] + rotated[0], pivot[1] + rotated[1]];
        // «Трёхмерная сцена», требование 16: ширина и высота не меняются — объект поворачивает сама
        // `rotation`.
        let new_size = if world.three_d() { s } else { [s[1], s[0]] };
        let new_pos = [
            new_center[0] - new_size[0] / 2.0,
            new_center[1] - new_size[1] / 2.0,
        ];
        world.set_vec2(id, property::POSITION, new_pos);
        world.set_vec2(id, property::SIZE, new_size);
        let current = world
            .rotation(id, property::ROTATION)
            .unwrap_or(Rotation::from_quarters(0));
        world.set_rotation(id, property::ROTATION, current.turned(dir));
    }
    let blocked = spec
        .blocked_by
        .as_ref()
        .is_some_and(|sel| group_overlaps_blocker(world, &group, sel, deleted, grid));
    if blocked {
        restore_group(world, &snapshot);
        execute_common_actions(
            world,
            &spec.if_blocked,
            outcome,
            marks,
            report,
            code_deletes,
            moved,
            code,
            rng,
            run_args,
            rule,
            deleted,
            grid,
        )?;
    } else {
        for &id in &group {
            moved[id as usize] = true;
        }
    }
    Ok(())
}

/// «check», требование 4–5, 18: candidates are fixed up front (`for_`, no delete request,
/// ascending id), then for each in turn: still matching `for_` and `when` (an earlier candidate's
/// `do` in this same rule may have changed it) — run `do` with this object as `run`'s argument.
#[allow(clippy::too_many_arguments)]
fn apply_check_rule(
    for_: &Selector,
    when: &Option<Condition>,
    do_: &[CommonAction],
    rule_label: &str,
    world: &mut World,
    scene: &SceneConfig,
    deleted: &mut Vec<u32>,
    moved: &mut [bool],
    grid: &mut SpatialGrid,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    outcome: &mut Option<Outcome>,
) -> Result<(), CodeError> {
    let candidates: Vec<u32> = world
        .ids()
        .filter(|id| !deleted.contains(id))
        .filter(|&id| selector_matches(for_, world, id))
        .collect();
    let mut ran_for: Vec<u32> = Vec::new();
    for id in candidates {
        if deleted.contains(&id) || !selector_matches(for_, world, id) {
            continue;
        }
        let holds = match when {
            None => true,
            Some(cond) => {
                let count_fn = |of: &Selector| count_selector_excluding(world, of, deleted);
                condition_holds(cond, world, Some(id), scene, moved, &count_fn)
            }
        };
        if !holds {
            continue;
        }
        if report.is_some() {
            ran_for.push(id);
        }
        let mut code_deletes = Vec::new();
        let deleted_snapshot = deleted.clone();
        execute_common_actions(
            world,
            do_,
            outcome,
            marks,
            report,
            &mut code_deletes,
            moved,
            code,
            rng,
            &[id],
            rule_label,
            &deleted_snapshot,
            grid,
        )?;
        deleted.extend(code_deletes);
    }
    if let Some(r) = report.as_mut()
        && !ran_for.is_empty()
    {
        r.fired.push(RuleFired::Check {
            rule: rule_label.to_string(),
            objects: ran_for,
        });
    }
    Ok(())
}

/// «Ходьба», требования 22–32: one `walk` rule — every object `for_` picks out that carries
/// `walk_to` walks toward it at `walk_speed` cells/second, in obход of every live `avoid` object
/// (a scene-edge-clamped straight line without `avoid`) — see `core::pathfind`. `walk_speed` 0 or
/// less leaves the object standing, `walk_to` unchanged (требование 30). Marks `moved` and
/// reports `RuleFired::Walk`, same as `apply_move_rule`.
#[allow(clippy::too_many_arguments)]
fn apply_walk_rule(
    world: &mut World,
    for_: &Selector,
    avoid: &Option<Selector>,
    scene: &SceneConfig,
    moved: &mut [bool],
    report: &mut Option<StepReportBuilder>,
    rule_index: usize,
    walk_paths: &mut WalkCaches,
) {
    let walkers: Vec<u32> = world
        .ids()
        .filter(|&id| selector_matches(for_, world, id))
        .filter(|&id| world.has(id, property::WALK_TO))
        .collect();
    let keep: HashSet<u32> = walkers.iter().copied().collect();
    pathfind::prune(walk_paths, &keep);

    let mut moved_objects: Vec<u32> = Vec::new();
    for id in walkers {
        let (Some(center_from), Some(size), Some(target), speed) = (
            world.vec2(id, property::POSITION).and_then(|p| {
                world
                    .vec2(id, property::SIZE)
                    .map(|s| [p[0] + s[0] / 2.0, p[1] + s[1] / 2.0])
            }),
            world.vec2(id, property::SIZE),
            world.vec2(id, property::WALK_TO),
            world.number_like(id, property::WALK_SPEED).unwrap_or(0.0),
        ) else {
            continue;
        };
        if speed <= 0.0 {
            continue;
        }
        let obstacles: Vec<(u32, Footprint)> = match avoid {
            None => Vec::new(),
            Some(sel) => world
                .ids()
                .filter(|&oid| oid != id)
                .filter(|&oid| selector_matches(sel, world, oid))
                .filter_map(|oid| footprint_of(world, oid).map(|f| (oid, f)))
                .collect(),
        };
        let rotation = world
            .three_d()
            .then(|| world.rotation(id, property::ROTATION))
            .flatten();
        let budget = speed / 60.0;
        let (new_center, arrived) = pathfind::advance(
            id,
            center_from,
            size,
            rotation,
            target,
            obstacles,
            (scene.width as f64, scene.height as f64),
            budget,
            walk_paths,
        );
        if new_center != center_from {
            world.set_vec2(
                id,
                property::POSITION,
                [new_center[0] - size[0] / 2.0, new_center[1] - size[1] / 2.0],
            );
            moved[id as usize] = true;
            moved_objects.push(id);
        }
        if arrived {
            world.clear_property(id, property::WALK_TO);
        }
    }
    if let Some(r) = report.as_mut()
        && !moved_objects.is_empty()
    {
        r.fired.push(RuleFired::Walk {
            rule: format!("rules[{rule_index}]"),
            objects: moved_objects,
        });
    }
}

/// Stage 4: "move", "check" and "walk" rules, in file order — «Исполнение игры»: перемешаны, а не
/// «сначала все move, потом все check», так что `after_move_of` внутри `check` видит движения,
/// сделанные более ранними правилами того же шага. `deleted` starts out seeded with stage 3's
/// `expired` and grows with every object a `check` rule's code deletes, so a later `check`'s own
/// `fewer_than`/group selection sees it too — «Правила игры», требование 8.
#[allow(clippy::too_many_arguments)]
pub fn apply_stage4(
    rules: &[Rule],
    world: &mut World,
    scene: &SceneConfig,
    grid_hop_ready: &mut [bool],
    moved: &mut [bool],
    deleted: &mut Vec<u32>,
    grid: &mut SpatialGrid,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    outcome: &mut Option<Outcome>,
    walk_paths: &mut WalkCaches,
) -> Result<(), CodeError> {
    for (index, rule) in rules.iter().enumerate() {
        match rule {
            Rule::Move { for_ } => {
                apply_move_rule(world, for_, grid_hop_ready, moved, report, index)
            }
            Rule::Check { for_, when, do_ } => {
                let rule_label = format!("rules[{index}]");
                apply_check_rule(
                    for_,
                    when,
                    do_,
                    &rule_label,
                    world,
                    scene,
                    deleted,
                    moved,
                    grid,
                    marks,
                    report,
                    code,
                    rng,
                    outcome,
                )?;
            }
            Rule::Walk { for_, avoid } => {
                apply_walk_rule(world, for_, avoid, scene, moved, report, index, walk_paths)
            }
            _ => {}
        }
    }
    Ok(())
}

/// Stage 5: uniform grid over `collides` objects, sized to the largest of them.
pub fn find_collision_pairs(world: &World, grid: &mut SpatialGrid) -> Vec<(u32, u32)> {
    let objects: Vec<(u32, Footprint)> = world
        .ids()
        .filter(|&id| world.flag(id, property::COLLIDES))
        .filter_map(|id| footprint_of(world, id).map(|f| (id, f)))
        .collect();
    overlapping_pairs(&objects, grid)
}

fn bounce(world: &mut World, bouncer: u32, other: u32) {
    let (Some(fb), Some(fo)) = (footprint_of(world, bouncer), footprint_of(world, other)) else {
        return;
    };
    let Some(vel) = world.vec2(bouncer, property::VELOCITY) else {
        return;
    };
    match (fb, fo) {
        (Footprint::Aligned(rb), Footprint::Aligned(ro)) => {
            bounce_aligned(world, bouncer, vel, rb, ro);
        }
        _ => bounce_oriented(world, bouncer, vel, &fb, &fo),
    }
}

/// «Исполнение игры» → «Столкновения»: хотя бы один из двух повёрнут — объект выталкивается поперёк
/// той стороны любого из двух прямоугольников, где выталкивать меньше всего, со стороны своей
/// середины, и скорость отражается от этой стороны.
fn bounce_oriented(world: &mut World, bouncer: u32, vel: Vec2, own: &Footprint, other: &Footprint) {
    let Some((normal, depth)) = own.push_out(other) else {
        return;
    };
    let Some(pos) = world.vec2(bouncer, property::POSITION) else {
        return;
    };
    let along = vel[0] * normal[0] + vel[1] * normal[1];
    world.set_vec2(
        bouncer,
        property::POSITION,
        [pos[0] + normal[0] * depth, pos[1] + normal[1] * depth],
    );
    world.set_vec2(
        bouncer,
        property::VELOCITY,
        [
            vel[0] - 2.0 * along * normal[0],
            vel[1] - 2.0 * along * normal[1],
        ],
    );
}

fn bounce_aligned(world: &mut World, bouncer: u32, vel: Vec2, rb: Rect, ro: Rect) {
    if rb.overlap(&ro).is_none() {
        return;
    }
    // «Исполнение игры»: объект выталкивается наружу вплотную к краю другого — с той стороны, где
    // его середина, и по оси, где выталкивать меньше. Когда ни один объект не выступает за другой ни
    // по одной оси, сдвиг равен ширине пересечения; объект, накрывший другой, иначе остался бы в нём
    // или ушёл насквозь.
    let flush = |b_start: f64, b_len: f64, o_start: f64, o_len: f64| {
        if b_start + b_len / 2.0 < o_start + o_len / 2.0 {
            o_start - b_len
        } else {
            o_start + o_len
        }
    };
    let x = flush(rb.x, rb.w, ro.x, ro.w);
    let y = flush(rb.y, rb.h, ro.y, ro.h);
    let mut pos = [rb.x, rb.y];
    let mut new_vel = vel;
    if (x - rb.x).abs() <= (y - rb.y).abs() {
        new_vel[0] = -vel[0];
        pos[0] = x;
    } else {
        new_vel[1] = -vel[1];
        pos[1] = y;
    }
    world.set_vec2(bouncer, property::POSITION, pos);
    world.set_vec2(bouncer, property::VELOCITY, new_vel);
}

/// The number an `add`/`set` action actually applies, in the property's own internal unit
/// (seconds are already converted to steps at load time, table/multiplier values included — see
/// `data::load::parse_number_expr`) — `None` when the acting object doesn't carry `by`/`times`
/// (or it isn't a `number`), which «Правила игры», требование 11 says means the action simply
/// doesn't apply to this object.
fn resolve_number_expr(expr: &NumberExpr, world: &World, actor: u32) -> Option<f64> {
    match expr {
        NumberExpr::Const(n) => Some(*n),
        NumberExpr::Table { table, by } => {
            let v = world.number_like(actor, *by)?;
            let idx = v.floor();
            let i = if idx < 0.0 {
                0
            } else {
                (idx as usize).min(table.len().saturating_sub(1))
            };
            table.get(i).copied()
        }
        NumberExpr::Multiplier { value, times } => {
            let v = world.number_like(actor, *times)?;
            Some(value * v)
        }
    }
}

fn apply_set_value(world: &mut World, id: u32, prop: PropertyId, value: &SetValue) {
    match value {
        SetValue::Const(v) => world.set_value(id, prop, v),
        SetValue::Number(expr) => {
            if let Some(n) = resolve_number_expr(expr, world, id) {
                world.set_number_like(id, prop, n);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_collide_effect(
    world: &mut World,
    id: u32,
    effect: &CollideEffect,
    bounced: &mut [bool],
    other: u32,
    deletes: &mut Vec<u32>,
    moved: &mut [bool],
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    rule: &str,
) -> Result<(), CodeError> {
    match effect {
        CollideEffect::Bounce => {
            if !bounced[id as usize] {
                bounce(world, id, other);
                bounced[id as usize] = true;
            }
        }
        CollideEffect::Delete => {
            deletes.push(id);
            if let Some(r) = report.as_mut() {
                let name = world.text(id, property::NAME).map(str::to_string);
                r.deletes
                    .push((id, name, DeleteCause::Rule(rule.to_string())));
            }
        }
        CollideEffect::Add { prop, expr } => {
            if let Some(delta) = resolve_number_expr(expr, world, id) {
                world.add_number_like(id, *prop, delta);
            }
        }
        CollideEffect::Set { prop, value } => apply_set_value(world, id, *prop, value),
        CollideEffect::Give { prop } => world.set_flag(id, *prop, true),
        CollideEffect::Take { prop } => world.set_flag(id, *prop, false),
        CollideEffect::Run(function) => {
            run_code(
                code,
                world,
                rng,
                deletes,
                moved,
                marks,
                report,
                rule,
                function,
                &[id, other],
            )?;
        }
    }
    Ok(())
}

/// `run_args` is the "who does the function get" for this rule kind's `do` — «Код игры»: `(a, b)`
/// for `collide`, the deleted object for `delete`, the spawned object's parent (or nothing) for
/// `spawn`, the checked object for `check`. `deleted` — every object with a delete request already
/// known at the moment this list starts running, for `shift`/`turn`'s own group selection
/// («Правила игры», требование 14); `grid` is the scratch `SpatialGrid` `shift`/`turn` reuses for
/// their `blocked_by` overlap search rather than scanning every pair of world objects.
#[allow(clippy::too_many_arguments)]
fn execute_common_actions(
    world: &mut World,
    actions: &[CommonAction],
    outcome: &mut Option<Outcome>,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    deletes: &mut Vec<u32>,
    moved: &mut [bool],
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    run_args: &[u32],
    rule: &str,
    deleted: &[u32],
    grid: &mut SpatialGrid,
) -> Result<(), CodeError> {
    for action in actions {
        match action {
            CommonAction::EndGame(o) => {
                if outcome.is_none() {
                    *outcome = Some(*o);
                }
            }
            CommonAction::Add { prop, expr } => {
                for id in world.ids().collect::<Vec<_>>() {
                    if !world.has(id, *prop) {
                        continue;
                    }
                    if let Some(delta) = resolve_number_expr(expr, world, id) {
                        world.add_number_like(id, *prop, delta);
                    }
                }
            }
            // «Правила игры», требование 10: пишет всем текущим носителям свойства.
            CommonAction::SetAll { prop, value } => {
                for id in world.ids().collect::<Vec<_>>() {
                    if world.has(id, *prop) {
                        apply_set_value(world, id, *prop, value);
                    }
                }
            }
            // требование 10: снимает признак со всех текущих носителей.
            CommonAction::TakeAll { prop } => {
                for id in world.ids().collect::<Vec<_>>() {
                    if world.has(id, *prop) {
                        world.set_flag(id, *prop, false);
                    }
                }
            }
            // требование 10: выдаёт признак всем по отбору, независимо от того, был ли он уже.
            CommonAction::GiveWhere { prop, selector } => {
                for id in world.ids().collect::<Vec<_>>() {
                    if selector_matches(selector, world, id) {
                        world.set_flag(id, *prop, true);
                    }
                }
            }
            // «Звук»: правило кладёт заявку — поднимает отметку и тут же о ней
            // забывает; звучит ли что-то на самом деле, шаг не спрашивает никогда.
            CommonAction::PlaySound(id) => marks.raise(*id),
            CommonAction::Run(function) => {
                run_code(
                    code, world, rng, deletes, moved, marks, report, rule, function, run_args,
                )?;
            }
            CommonAction::Shift(spec) => {
                execute_shift(
                    spec, world, deleted, moved, grid, outcome, marks, report, deletes, code, rng,
                    run_args, rule,
                )?;
            }
            CommonAction::Turn(spec) => {
                execute_turn(
                    spec, world, deleted, moved, grid, outcome, marks, report, deletes, code, rng,
                    run_args, rule,
                )?;
            }
        }
    }
    Ok(())
}

/// Stage 6: "collide" rules against the pairs found at stage 5. `already_deleted` — stage 3's
/// `expired`, the only delete requests that can exist before this stage runs — is `shift`/`turn`'s
/// own group-selection baseline within a `do` here (требование 14); a delete queued by an earlier
/// pair's own effects this same stage is not folded back in for a later pair's group selection —
/// a narrow, deliberate simplification, see the phase report.
#[allow(clippy::too_many_arguments)]
pub fn apply_collide_rules(
    rules: &[Rule],
    world: &mut World,
    pairs: &[(u32, u32)],
    bounced: &mut [bool],
    deletes: &mut Vec<u32>,
    moved: &mut [bool],
    outcome: &mut Option<Outcome>,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    already_deleted: &[u32],
    grid: &mut SpatialGrid,
) -> Result<(), CodeError> {
    for (index, rule) in rules.iter().enumerate() {
        let Rule::Collide {
            a,
            b,
            effects_a,
            effects_b,
            do_,
        } = rule
        else {
            continue;
        };
        let rule_label = format!("rules[{index}]");
        let mut fired_pairs: Vec<(u32, u32)> = Vec::new();
        for &(lo, hi) in pairs {
            let (obj_a, obj_b) = if selector_matches(a, world, lo) && selector_matches(b, world, hi)
            {
                (lo, hi)
            } else if selector_matches(a, world, hi) && selector_matches(b, world, lo) {
                (hi, lo)
            } else {
                continue;
            };
            if report.is_some() {
                fired_pairs.push((obj_a, obj_b));
            }
            for effect in effects_a {
                apply_collide_effect(
                    world,
                    obj_a,
                    effect,
                    bounced,
                    obj_b,
                    deletes,
                    moved,
                    marks,
                    report,
                    code,
                    rng,
                    &rule_label,
                )?;
            }
            for effect in effects_b {
                apply_collide_effect(
                    world,
                    obj_b,
                    effect,
                    bounced,
                    obj_a,
                    deletes,
                    moved,
                    marks,
                    report,
                    code,
                    rng,
                    &rule_label,
                )?;
            }
            execute_common_actions(
                world,
                do_,
                outcome,
                marks,
                report,
                deletes,
                moved,
                code,
                rng,
                &[obj_a, obj_b],
                &rule_label,
                already_deleted,
                grid,
            )?;
        }
        if let Some(r) = report.as_mut()
            && !fired_pairs.is_empty()
        {
            r.fired.push(RuleFired::Collide {
                rule: rule_label,
                pairs: fired_pairs,
            });
        }
    }
    Ok(())
}

pub struct PendingCreate {
    pub position: Vec2,
    pub props: Vec<(PropertyId, Value)>,
    /// «Редактор», требование 44: which `spawn` rule (`rules[N]`) queued this create — filled in
    /// by `queue_create_and_delete_rules`'s own caller loop, empty when no report is being built.
    pub rule: String,
}

struct PendingState {
    deleted: HashSet<u32>,
    world_occupied: Vec<(u32, Footprint)>,
    create_occupied: Vec<Footprint>,
    creates: Vec<PendingCreate>,
}

impl PendingState {
    fn matches_pending_create(selector: &Selector, create: &PendingCreate) -> bool {
        // A `false` flag is absent for `World::has` (world.rs's `Column::has`), so a pending
        // create's `false` flag must be absent for a selector here too.
        let has = |p: &PropertyId| {
            create
                .props
                .iter()
                .any(|(pp, v)| pp == p && !matches!(v, Value::Flag(false)))
        };
        selector.has.iter().all(has) && !selector.without.iter().any(has)
    }

    fn count_selector(&self, selector: &Selector, world: &World) -> u32 {
        let live = world
            .ids()
            .filter(|id| !self.deleted.contains(id))
            .filter(|&id| selector_matches(selector, world, id))
            .count();
        let pending = self
            .creates
            .iter()
            .filter(|c| Self::matches_pending_create(selector, c))
            .count();
        (live + pending) as u32
    }

    fn free_cell(&self, scene: &SceneConfig, rng: &mut Rng) -> Option<(u32, u32)> {
        let mut free = Vec::new();
        for x in 0..scene.width {
            for y in 0..scene.height {
                let cell = Footprint::flat([x as f64, y as f64], [1.0, 1.0]);
                let occupied = self.world_occupied.iter().any(|(_, f)| f.overlaps(&cell))
                    || self.create_occupied.iter().any(|f| f.overlaps(&cell));
                if !occupied {
                    free.push((x, y));
                }
            }
        }
        if free.is_empty() {
            return None;
        }
        let idx = rng.next_below(free.len() as u32) as usize;
        Some(free[idx])
    }

    fn queue_delete(&mut self, id: u32) {
        self.deleted.insert(id);
        self.world_occupied.retain(|&(oid, _)| oid != id);
    }

    fn resolve_template(
        &self,
        template: &[(PropertyId, TemplateValue)],
        parent: Option<u32>,
        world: &World,
        properties: &PropertyTable,
    ) -> Option<Vec<(PropertyId, Value)>> {
        let mut resolved = Vec::with_capacity(template.len());
        for (prop, tv) in template {
            let value = match tv {
                TemplateValue::Const(v) => v.clone(),
                TemplateValue::FromParent(parent_prop) => {
                    let parent_id = parent?;
                    world.get_value(parent_id, *parent_prop, properties.kind(*parent_prop))?
                }
            };
            resolved.push((*prop, value));
        }
        Some(resolved)
    }

    /// The `where` anchor point, before any `pick_one` cell offset — «Создание по форме»,
    /// требования 19, 21: `random_cell`'s freedom check only ever looks at this one point, never
    /// at the rest of a `pick_one` shape.
    fn where_position(
        &mut self,
        place: SpawnPlace,
        parent: Option<u32>,
        ctx: &SpawnContext,
        rng: &mut Rng,
        random_cell_exhausted: &mut bool,
    ) -> Option<Vec2> {
        match place {
            SpawnPlace::AtParent => parent.and_then(|p| ctx.pre_move_positions[p as usize]),
            SpawnPlace::RandomCell => {
                let Some((x, y)) = self.free_cell(ctx.scene, rng) else {
                    *random_cell_exhausted = true;
                    return None;
                };
                Some([x as f64, y as f64])
            }
            SpawnPlace::Cell(at) => Some(at),
        }
    }

    fn place_one(
        &mut self,
        position: Vec2,
        template: &[(PropertyId, TemplateValue)],
        parent: Option<u32>,
        ctx: &SpawnContext,
    ) -> bool {
        let Some(props) = self.resolve_template(template, parent, ctx.world, ctx.properties) else {
            return false;
        };
        let collides = props
            .iter()
            .any(|(p, v)| *p == property::COLLIDES && matches!(v, Value::Flag(true)));
        let size = props
            .iter()
            .find(|(p, _)| *p == property::SIZE)
            .and_then(|(_, v)| match v {
                Value::Vec2(s) => Some(*s),
                _ => None,
            });
        if collides {
            let size = size.unwrap_or([1.0, 1.0]);
            let footprint = if ctx.world.three_d() {
                let rotation = props.iter().find_map(|(p, v)| match (*p, v) {
                    (property::ROTATION, Value::Rotation(r)) => Some(*r),
                    _ => None,
                });
                Footprint::rotated(position, size, rotation)
            } else {
                Footprint::flat(position, size)
            };
            self.create_occupied.push(footprint);
        }
        self.creates.push(PendingCreate {
            position,
            props,
            rule: String::new(),
        });
        true
    }

    /// Layers `overrides` on top of `base`, later fields winning — «Создание по форме»,
    /// требование 20: «поля — template, поверх них поля варианта, поверх — поля клетки».
    fn layer_fields(
        base: &[(PropertyId, TemplateValue)],
        overrides: &[(PropertyId, TemplateValue)],
    ) -> Vec<(PropertyId, TemplateValue)> {
        let mut merged: Vec<(PropertyId, TemplateValue)> = base.to_vec();
        for (prop, value) in overrides {
            merged.retain(|(p, _)| p != prop);
            merged.push((*prop, value.clone()));
        }
        merged
    }

    /// One `spawn` firing: without `pick_one`, a single object at `where`'s own position
    /// (требование 22); with it, a variant is chosen by the engine's counter *before* `where`
    /// resolves `random_cell` (требование 21), then one object per cell of that variant, each at
    /// `where` + the cell's own offset, fields layered `template` → variant → cell (требование
    /// 20). Returns whether at least one object was actually queued.
    #[allow(clippy::too_many_arguments)]
    fn queue_create(
        &mut self,
        place: SpawnPlace,
        parent: Option<u32>,
        template: &[(PropertyId, TemplateValue)],
        pick_one: &Option<Vec<SpawnVariant>>,
        ctx: &SpawnContext,
        rng: &mut Rng,
        random_cell_exhausted: &mut bool,
    ) -> bool {
        match pick_one {
            None => {
                let Some(position) =
                    self.where_position(place, parent, ctx, rng, random_cell_exhausted)
                else {
                    return false;
                };
                self.place_one(position, template, parent, ctx)
            }
            Some(variants) => {
                let variant = &variants[rng.next_below(variants.len() as u32) as usize];
                let Some(anchor) =
                    self.where_position(place, parent, ctx, rng, random_cell_exhausted)
                else {
                    return false;
                };
                let variant_fields = Self::layer_fields(template, &variant.fields);
                let mut any = false;
                for cell in &variant.cells {
                    let position = [anchor[0] + cell.at[0], anchor[1] + cell.at[1]];
                    let fields = Self::layer_fields(&variant_fields, &cell.fields);
                    any |= self.place_one(position, &fields, parent, ctx);
                }
                any
            }
        }
    }
}

struct SpawnContext<'a> {
    world: &'a World,
    properties: &'a PropertyTable,
    scene: &'a SceneConfig,
    pre_move_positions: &'a [Option<Vec2>],
}

/// «Код игры»: applies whatever `delete(obj)` calls a `do`-action's code just queued to `pending`
/// too — так заявка из кода видна `fewer_than` того же прохода, так же, как заявки самих правил
/// этапа 7. `execute_common_actions` only ever appends to `code_deletes` (a plain `Vec`, matching
/// `code::Runner::run`'s own queue parameter); this drains it into `PendingState::queue_delete`,
/// which additionally keeps `world_occupied` in step for `random_cell`.
fn absorb_code_deletes(pending: &mut PendingState, code_deletes: &mut Vec<u32>) {
    for id in code_deletes.drain(..) {
        pending.queue_delete(id);
    }
}

/// A `Rule::Delete`'s own direct effect (as opposed to a `run` function it called, tagged
/// separately inside `run_code`) — queues the deletion and, while reporting, tags it with this
/// rule's label and this step's `deleted_by_rule` list, reading the object's `name` now, before
/// stage 8 actually removes it.
fn queue_delete_reporting(
    pending: &mut PendingState,
    world: &World,
    report: &mut Option<StepReportBuilder>,
    deleted_by_rule: &mut Vec<u32>,
    rule_label: &str,
    id: u32,
) {
    pending.queue_delete(id);
    if let Some(r) = report.as_mut() {
        let name = world.text(id, property::NAME).map(str::to_string);
        r.deletes
            .push((id, name, DeleteCause::Rule(rule_label.to_string())));
        deleted_by_rule.push(id);
    }
}

#[allow(clippy::too_many_arguments)]
/// Stage 7: "create" and "delete" rules, in file order, accumulating requests only; `do_`
/// actions (`end_game`, `add`, `run`, `shift`, `turn`) still run immediately, same as for
/// "collide".
pub fn queue_create_and_delete_rules(
    rules: &[Rule],
    world: &mut World,
    properties: &PropertyTable,
    scene: &SceneConfig,
    already_deleted: &[u32],
    moved: &mut [bool],
    pre_move_positions: &[Option<Vec2>],
    rng: &mut Rng,
    outcome: &mut Option<Outcome>,
    random_cell_exhausted: &mut bool,
    marks: &mut SoundMarks<'_>,
    report: &mut Option<StepReportBuilder>,
    code: &mut Option<CodeEnv<'_>>,
    grid: &mut SpatialGrid,
) -> Result<(Vec<u32>, Vec<PendingCreate>), CodeError> {
    let mut pending = PendingState {
        deleted: already_deleted.iter().copied().collect(),
        world_occupied: world
            .ids()
            .filter(|id| !already_deleted.contains(id))
            .filter(|&id| world.flag(id, property::COLLIDES))
            .filter_map(|id| footprint_of(world, id).map(|f| (id, f)))
            .collect(),
        create_occupied: Vec::new(),
        creates: Vec::new(),
    };

    for (index, rule) in rules.iter().enumerate() {
        let rule_label = format!("rules[{index}]");
        match rule {
            Rule::Delete { for_, when, do_ } => {
                let candidates: Vec<u32> = world
                    .ids()
                    .filter(|id| !pending.deleted.contains(id))
                    .filter(|&id| selector_matches(for_, world, id))
                    .collect();
                let mut deleted_by_rule: Vec<u32> = Vec::new();
                match when {
                    Condition::FewerThan { count, of } => {
                        if pending.count_selector(of, world) < *count {
                            for id in candidates {
                                queue_delete_reporting(
                                    &mut pending,
                                    world,
                                    report,
                                    &mut deleted_by_rule,
                                    &rule_label,
                                    id,
                                );
                                let mut code_deletes = Vec::new();
                                let deleted_snapshot: Vec<u32> =
                                    pending.deleted.iter().copied().collect();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
                                    report,
                                    &mut code_deletes,
                                    moved,
                                    code,
                                    rng,
                                    &[id],
                                    &rule_label,
                                    &deleted_snapshot,
                                    grid,
                                )?;
                                absorb_code_deletes(&mut pending, &mut code_deletes);
                            }
                        }
                    }
                    // «Формат игры»: условие спрашивает, сместился ли на этом шаге объект из
                    // отбора `of` — а не сам кандидат на удаление, у которого может быть свой
                    // отдельный `for`.
                    Condition::AfterMoveOf { of } => {
                        let any_moved = world
                            .ids()
                            .any(|id| moved[id as usize] && selector_matches(of, world, id));
                        if any_moved {
                            for id in candidates {
                                queue_delete_reporting(
                                    &mut pending,
                                    world,
                                    report,
                                    &mut deleted_by_rule,
                                    &rule_label,
                                    id,
                                );
                                let mut code_deletes = Vec::new();
                                let deleted_snapshot: Vec<u32> =
                                    pending.deleted.iter().copied().collect();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
                                    report,
                                    &mut code_deletes,
                                    moved,
                                    code,
                                    rng,
                                    &[id],
                                    &rule_label,
                                    &deleted_snapshot,
                                    grid,
                                )?;
                                absorb_code_deletes(&mut pending, &mut code_deletes);
                            }
                        }
                    }
                    _ => {
                        for id in candidates {
                            let deleted_snapshot: Vec<u32> =
                                pending.deleted.iter().copied().collect();
                            let count_fn = |of: &Selector| pending.count_selector(of, world);
                            if condition_holds(when, world, Some(id), scene, moved, &count_fn) {
                                queue_delete_reporting(
                                    &mut pending,
                                    world,
                                    report,
                                    &mut deleted_by_rule,
                                    &rule_label,
                                    id,
                                );
                                let mut code_deletes = Vec::new();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
                                    report,
                                    &mut code_deletes,
                                    moved,
                                    code,
                                    rng,
                                    &[id],
                                    &rule_label,
                                    &deleted_snapshot,
                                    grid,
                                )?;
                                absorb_code_deletes(&mut pending, &mut code_deletes);
                            }
                        }
                    }
                }
                if let Some(r) = report.as_mut()
                    && !deleted_by_rule.is_empty()
                {
                    r.fired.push(RuleFired::Delete {
                        rule: rule_label.clone(),
                        objects: deleted_by_rule,
                    });
                }
            }
            Rule::Spawn {
                when,
                place,
                template,
                pick_one,
                do_,
            } => {
                let ctx = SpawnContext {
                    world,
                    properties,
                    scene,
                    pre_move_positions,
                };
                let count_fn = |of: &Selector| pending.count_selector(of, world);
                match &when.parent_of {
                    None => {
                        let fires =
                            condition_holds(&when.condition, world, None, scene, moved, &count_fn);
                        let before = pending.creates.len();
                        if fires
                            && pending.queue_create(
                                *place,
                                None,
                                template,
                                pick_one,
                                &ctx,
                                rng,
                                random_cell_exhausted,
                            )
                        {
                            for c in &mut pending.creates[before..] {
                                c.rule.clone_from(&rule_label);
                            }
                            let mut code_deletes = Vec::new();
                            let deleted_snapshot: Vec<u32> =
                                pending.deleted.iter().copied().collect();
                            execute_common_actions(
                                world,
                                do_,
                                outcome,
                                marks,
                                report,
                                &mut code_deletes,
                                moved,
                                code,
                                rng,
                                &[],
                                &rule_label,
                                &deleted_snapshot,
                                grid,
                            )?;
                            absorb_code_deletes(&mut pending, &mut code_deletes);
                        }
                    }
                    Some(parent_of) => {
                        let fires =
                            condition_holds(&when.condition, world, None, scene, moved, &count_fn);
                        if !fires {
                            continue;
                        }
                        let parents: Vec<u32> = world
                            .ids()
                            .filter(|&id| {
                                moved[id as usize] && selector_matches(parent_of, world, id)
                            })
                            .collect();
                        let created: Vec<(u32, bool)> = parents
                            .into_iter()
                            .map(|parent| {
                                let before = pending.creates.len();
                                let ok = pending.queue_create(
                                    *place,
                                    Some(parent),
                                    template,
                                    pick_one,
                                    &ctx,
                                    rng,
                                    random_cell_exhausted,
                                );
                                if ok {
                                    for c in &mut pending.creates[before..] {
                                        c.rule.clone_from(&rule_label);
                                    }
                                }
                                (parent, ok)
                            })
                            .collect();
                        for (parent, ok) in created {
                            if ok {
                                let mut code_deletes = Vec::new();
                                let deleted_snapshot: Vec<u32> =
                                    pending.deleted.iter().copied().collect();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
                                    report,
                                    &mut code_deletes,
                                    moved,
                                    code,
                                    rng,
                                    &[parent],
                                    &rule_label,
                                    &deleted_snapshot,
                                    grid,
                                )?;
                                absorb_code_deletes(&mut pending, &mut code_deletes);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok((pending.deleted.into_iter().collect(), pending.creates))
}

/// Объект за сценой дальше собственного размера по осям сцены; повёрнутое место — сам повёрнутый
/// прямоугольник, а не охватывающий его.
fn outside_scene(place: &Footprint, scene: &SceneConfig) -> bool {
    let rect = place.aabb();
    match place {
        Footprint::Aligned(_) => {
            let left = rect.x + rect.w < -rect.w;
            let right = rect.x > scene.width as f64 + rect.w;
            let top = rect.y + rect.h < -rect.h;
            let bottom = rect.y > scene.height as f64 + rect.h;
            left || right || top || bottom
        }
        Footprint::Oriented(_) => place.is_apart_from(&Footprint::flat(
            [-rect.w, -rect.h],
            [
                scene.width as f64 + 2.0 * rect.w,
                scene.height as f64 + 2.0 * rect.h,
            ],
        )),
    }
}

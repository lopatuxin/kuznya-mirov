use std::collections::HashSet;

use super::code::{CodeError, Runner as CodeRunner};
use super::grid::{Rect, SpatialGrid, largest_dimension};
use super::input::{KeyAction, KeyEvent};
use super::property::{self, PropertyId, PropertyTable};
use super::rng::Rng;
use super::rules::{
    CollideEffect, CommonAction, Condition, NumberExpr, Outcome, Rule, Selector, SetValue,
    ShiftSpec, SpawnPlace, SpawnVariant, TemplateValue, TurnDir, TurnSpec,
};
use super::scene::SceneConfig;
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
        })
}

pub fn selector_matches(selector: &Selector, world: &World, id: u32) -> bool {
    world.has_all(id, &selector.has) && world.has_none(id, &selector.without)
}

fn rect_of(world: &World, id: u32) -> Option<Rect> {
    let p = world.vec2(id, property::POSITION)?;
    let s = world.vec2(id, property::SIZE)?;
    Some(Rect {
        x: p[0],
        y: p[1],
        w: s[0],
        h: s[1],
    })
}

/// Stage 2: press/release events turn into property writes named by each object's `keys` table —
/// applied in `events`' own order, not grouped by press-then-release, so a release and a press of
/// the same key landing in the same real-time gap leave the object in the state the later of the
/// two actually calls for.
pub fn apply_input(world: &mut World, events: &[KeyEvent]) {
    let mut edits: Vec<(u32, PropertyId, Value)> = Vec::new();
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
    for (id, prop, value) in edits {
        world.set_value(id, prop, &value);
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
    // «Формат игры»: объект с follow_mouse не выходит за край сцены — середина под курсором,
    // пока объект помещается целиком, дальше прижат к краю.
    let fit =
        |start: f64, len: f64, scene_len: u32| start.clamp(0.0, (scene_len as f64 - len).max(0.0));
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
            new_pos[0] = fit(cursor[0] - size[0] / 2.0, size[0], scene.width);
        }
        if axis.affects_y() {
            new_pos[1] = fit(cursor[1] - size[1] / 2.0, size[1], scene.height);
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
) {
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
        }
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
        Condition::OutsideScene => {
            id.is_some_and(|id| rect_of(world, id).is_some_and(|r| outside_scene(&r, scene)))
        }
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
    let mut objects: Vec<(u32, Rect)> = group
        .iter()
        .filter_map(|&id| rect_of(world, id).map(|r| (id, r)))
        .collect();
    let group_len = objects.len();
    for id in world.ids() {
        if deleted.contains(&id) || group.contains(&id) {
            continue;
        }
        if selector_matches(blocked_by, world, id)
            && let Some(r) = rect_of(world, id)
        {
            objects.push((id, r));
        }
    }
    if objects.len() == group_len {
        return false;
    }
    let cell_size = largest_dimension(&objects).max(1e-3);
    grid.find_pairs(cell_size, &objects)
        .into_iter()
        .any(|(a, b)| group.contains(&a) != group.contains(&b))
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
        let new_size = [s[1], s[0]];
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
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    outcome: &mut Option<Outcome>,
) -> Result<(), CodeError> {
    let candidates: Vec<u32> = world
        .ids()
        .filter(|id| !deleted.contains(id))
        .filter(|&id| selector_matches(for_, world, id))
        .collect();
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
        let mut code_deletes = Vec::new();
        let deleted_snapshot = deleted.clone();
        execute_common_actions(
            world,
            do_,
            outcome,
            marks,
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
    Ok(())
}

/// Stage 4: "move" and "check" rules, in file order — «Исполнение игры»: перемешаны, а не
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
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    outcome: &mut Option<Outcome>,
) -> Result<(), CodeError> {
    for (index, rule) in rules.iter().enumerate() {
        match rule {
            Rule::Move { for_ } => apply_move_rule(world, for_, grid_hop_ready, moved),
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
                    code,
                    rng,
                    outcome,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Stage 5: uniform grid over `collides` objects, sized to the largest of them.
pub fn find_collision_pairs(world: &World, grid: &mut SpatialGrid) -> Vec<(u32, u32)> {
    let objects: Vec<(u32, Rect)> = world
        .ids()
        .filter(|&id| world.flag(id, property::COLLIDES))
        .filter_map(|id| rect_of(world, id).map(|r| (id, r)))
        .collect();
    let cell_size = largest_dimension(&objects).max(1e-3);
    grid.find_pairs(cell_size, &objects)
}

fn bounce(world: &mut World, bouncer: u32, other: u32) {
    let (Some(rb), Some(ro)) = (rect_of(world, bouncer), rect_of(world, other)) else {
        return;
    };
    if rb.overlap(&ro).is_none() {
        return;
    }
    let Some(vel) = world.vec2(bouncer, property::VELOCITY) else {
        return;
    };
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
        CollideEffect::Delete => deletes.push(id),
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
                    code, world, rng, deletes, moved, marks, rule, function, run_args,
                )?;
            }
            CommonAction::Shift(spec) => {
                execute_shift(
                    spec, world, deleted, moved, grid, outcome, marks, deletes, code, rng,
                    run_args, rule,
                )?;
            }
            CommonAction::Turn(spec) => {
                execute_turn(
                    spec, world, deleted, moved, grid, outcome, marks, deletes, code, rng,
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
        for &(lo, hi) in pairs {
            let (obj_a, obj_b) = if selector_matches(a, world, lo) && selector_matches(b, world, hi)
            {
                (lo, hi)
            } else if selector_matches(a, world, hi) && selector_matches(b, world, lo) {
                (hi, lo)
            } else {
                continue;
            };
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
    }
    Ok(())
}

pub struct PendingCreate {
    pub position: Vec2,
    pub props: Vec<(PropertyId, Value)>,
}

struct PendingState {
    deleted: HashSet<u32>,
    world_occupied: Vec<(u32, Rect)>,
    create_occupied: Vec<Rect>,
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
                let cell = Rect {
                    x: x as f64,
                    y: y as f64,
                    w: 1.0,
                    h: 1.0,
                };
                let occupied = self
                    .world_occupied
                    .iter()
                    .any(|(_, r)| r.overlap(&cell).is_some())
                    || self
                        .create_occupied
                        .iter()
                        .any(|r| r.overlap(&cell).is_some());
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
            let (w, h) = size.map(|s| (s[0], s[1])).unwrap_or((1.0, 1.0));
            self.create_occupied.push(Rect {
                x: position[0],
                y: position[1],
                w,
                h,
            });
        }
        self.creates.push(PendingCreate { position, props });
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
    code: &mut Option<CodeEnv<'_>>,
    grid: &mut SpatialGrid,
) -> Result<(Vec<u32>, Vec<PendingCreate>), CodeError> {
    let mut pending = PendingState {
        deleted: already_deleted.iter().copied().collect(),
        world_occupied: world
            .ids()
            .filter(|id| !already_deleted.contains(id))
            .filter(|&id| world.flag(id, property::COLLIDES))
            .filter_map(|id| rect_of(world, id).map(|r| (id, r)))
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
                match when {
                    Condition::FewerThan { count, of } => {
                        if pending.count_selector(of, world) < *count {
                            for id in candidates {
                                pending.queue_delete(id);
                                let mut code_deletes = Vec::new();
                                let deleted_snapshot: Vec<u32> =
                                    pending.deleted.iter().copied().collect();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
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
                                pending.queue_delete(id);
                                let mut code_deletes = Vec::new();
                                let deleted_snapshot: Vec<u32> =
                                    pending.deleted.iter().copied().collect();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
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
                                pending.queue_delete(id);
                                let mut code_deletes = Vec::new();
                                execute_common_actions(
                                    world,
                                    do_,
                                    outcome,
                                    marks,
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
                            let mut code_deletes = Vec::new();
                            let deleted_snapshot: Vec<u32> =
                                pending.deleted.iter().copied().collect();
                            execute_common_actions(
                                world,
                                do_,
                                outcome,
                                marks,
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
                                (
                                    parent,
                                    pending.queue_create(
                                        *place,
                                        Some(parent),
                                        template,
                                        pick_one,
                                        &ctx,
                                        rng,
                                        random_cell_exhausted,
                                    ),
                                )
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

fn outside_scene(rect: &Rect, scene: &SceneConfig) -> bool {
    let left = rect.x + rect.w < -rect.w;
    let right = rect.x > scene.width as f64 + rect.w;
    let top = rect.y + rect.h < -rect.h;
    let bottom = rect.y > scene.height as f64 + rect.h;
    left || right || top || bottom
}

use std::collections::HashSet;

use super::code::{CodeError, Runner as CodeRunner};
use super::grid::{Rect, SpatialGrid, largest_dimension};
use super::input::{KeyAction, KeyEvent};
use super::property::{self, PropertyId, PropertyTable};
use super::rng::Rng;
use super::rules::{
    CollideEffect, CommonAction, Condition, Rule, Selector, SpawnCondition, SpawnPlace,
    TemplateValue,
};
use super::scene::SceneConfig;
use super::sound::SoundMarks;
use super::value::{Value, Vec2};
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

/// Stage 3: lifetime and grid-hop counters. Returns objects whose lifetime just expired.
pub fn tick_counters(world: &mut World, grid_hop_ready: &mut [bool]) -> Vec<u32> {
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
    }
    expired
}

/// Stage 4: "move" rules, in file order. Records which objects actually changed position.
pub fn apply_move_rules(
    rules: &[Rule],
    world: &mut World,
    grid_hop_ready: &mut [bool],
    moved: &mut [bool],
) {
    for rule in rules {
        let Rule::Move { for_ } = rule else { continue };
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
    let Some((ox, oy)) = rb.overlap(&ro) else {
        return;
    };
    let Some(vel) = world.vec2(bouncer, property::VELOCITY) else {
        return;
    };
    let mut pos = [rb.x, rb.y];
    let mut new_vel = vel;
    if ox <= oy {
        new_vel[0] = -vel[0];
        let bouncer_center = rb.x + rb.w / 2.0;
        let other_center = ro.x + ro.w / 2.0;
        pos[0] = if bouncer_center < other_center {
            rb.x - ox
        } else {
            rb.x + ox
        };
    } else {
        new_vel[1] = -vel[1];
        let bouncer_center = rb.y + rb.h / 2.0;
        let other_center = ro.y + ro.h / 2.0;
        pos[1] = if bouncer_center < other_center {
            rb.y - oy
        } else {
            rb.y + oy
        };
    }
    world.set_vec2(bouncer, property::POSITION, pos);
    world.set_vec2(bouncer, property::VELOCITY, new_vel);
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
        CollideEffect::Add { prop, value } => {
            if let Some(delta) = value.as_number_like() {
                world.add_number_like(id, *prop, delta);
            }
        }
        CollideEffect::Set { prop, value } => world.set_value(id, *prop, value),
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

/// Prop marked `add`-eligible from `do`: every holder of the property gets the delta, not
/// just the pair that triggered the rule. `run_args` is the "who does the function get" for this
/// rule kind's `do` — «Код игры»: `(a, b)` for `collide`, the deleted object for `delete`, the
/// spawned object's parent (or nothing) for `spawn`.
#[allow(clippy::too_many_arguments)]
fn execute_common_actions(
    world: &mut World,
    actions: &[CommonAction],
    outcome: &mut Option<super::rules::Outcome>,
    marks: &mut SoundMarks<'_>,
    deletes: &mut Vec<u32>,
    moved: &mut [bool],
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
    run_args: &[u32],
    rule: &str,
) -> Result<(), CodeError> {
    for action in actions {
        match action {
            CommonAction::EndGame(o) => {
                if outcome.is_none() {
                    *outcome = Some(*o);
                }
            }
            CommonAction::Add { prop, value } => {
                let Some(delta) = value.as_number_like() else {
                    continue;
                };
                for id in world.ids().collect::<Vec<_>>() {
                    if world.has(id, *prop) {
                        world.add_number_like(id, *prop, delta);
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
        }
    }
    Ok(())
}

/// Stage 6: "collide" rules against the pairs found at stage 5.
#[allow(clippy::too_many_arguments)]
pub fn apply_collide_rules(
    rules: &[Rule],
    world: &mut World,
    pairs: &[(u32, u32)],
    bounced: &mut [bool],
    deletes: &mut Vec<u32>,
    moved: &mut [bool],
    outcome: &mut Option<super::rules::Outcome>,
    marks: &mut SoundMarks<'_>,
    code: &mut Option<CodeEnv<'_>>,
    rng: &mut Rng,
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

    fn queue_create(
        &mut self,
        place: SpawnPlace,
        parent: Option<u32>,
        template: &[(PropertyId, TemplateValue)],
        ctx: &SpawnContext,
        rng: &mut Rng,
        random_cell_exhausted: &mut bool,
    ) -> bool {
        let Some(props) = self.resolve_template(template, parent, ctx.world, ctx.properties) else {
            return false;
        };
        let position = match place {
            SpawnPlace::AtParent => {
                let Some(Some(p)) = parent.map(|p| ctx.pre_move_positions[p as usize]) else {
                    return false;
                };
                p
            }
            SpawnPlace::RandomCell => {
                let Some((x, y)) = self.free_cell(ctx.scene, rng) else {
                    *random_cell_exhausted = true;
                    return false;
                };
                [x as f64, y as f64]
            }
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
/// actions (`end_game`, `add`, `run`) still run immediately, same as for "collide".
pub fn queue_create_and_delete_rules(
    rules: &[Rule],
    world: &mut World,
    properties: &PropertyTable,
    scene: &SceneConfig,
    already_deleted: &[u32],
    moved: &mut [bool],
    pre_move_positions: &[Option<Vec2>],
    rng: &mut Rng,
    outcome: &mut Option<super::rules::Outcome>,
    random_cell_exhausted: &mut bool,
    marks: &mut SoundMarks<'_>,
    code: &mut Option<CodeEnv<'_>>,
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
                                )?;
                                absorb_code_deletes(&mut pending, &mut code_deletes);
                            }
                        }
                    }
                    _ => {
                        for id in candidates {
                            if condition_holds_for_object(when, world, id, scene) {
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
                do_,
            } => {
                let ctx = SpawnContext {
                    world,
                    properties,
                    scene,
                    pre_move_positions,
                };
                match when {
                    SpawnCondition::FewerThan { count, of } => {
                        if pending.count_selector(of, world) < *count
                            && pending.queue_create(
                                *place,
                                None,
                                template,
                                &ctx,
                                rng,
                                random_cell_exhausted,
                            )
                        {
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
                                &[],
                                &rule_label,
                            )?;
                            absorb_code_deletes(&mut pending, &mut code_deletes);
                        }
                    }
                    SpawnCondition::AfterMoveOf { of } => {
                        let parents: Vec<u32> = world
                            .ids()
                            .filter(|&id| moved[id as usize] && selector_matches(of, world, id))
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

fn condition_holds_for_object(
    condition: &Condition,
    world: &World,
    id: u32,
    scene: &SceneConfig,
) -> bool {
    match condition {
        Condition::Compare { prop, op, value } => world
            .number_like(id, *prop)
            .is_some_and(|lhs| op.apply(lhs, *value)),
        Condition::OutsideScene => rect_of(world, id).is_some_and(|r| outside_scene(&r, scene)),
        Condition::AfterMoveOf { .. } => false, // handled globally by the caller, before this function runs
        Condition::FewerThan { .. } => false, // handled globally by the caller, before this function runs
    }
}

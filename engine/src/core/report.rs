//! «Редактор» → «Что сработало на шаге», требования 23, 44: the per-step report a play/replay
//! session collects — which rules fired and on which objects, what was created and deleted and
//! why. Built only while `Game::begin_session` is active (`Game::step`'s report accumulator is
//! `None` otherwise, so the page pays nothing for this).

use super::rules::Outcome;

/// One rule that did something this step, in file order — «Правила игры»: a rule matching nobody
/// never appears here at all.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleFired {
    Move {
        rule: String,
        objects: Vec<u32>,
    },
    Check {
        rule: String,
        objects: Vec<u32>,
    },
    Collide {
        rule: String,
        pairs: Vec<(u32, u32)>,
    },
    Delete {
        rule: String,
        objects: Vec<u32>,
    },
    Spawn {
        rule: String,
        objects: Vec<u32>,
    },
}

impl RuleFired {
    pub fn rule_label(&self) -> &str {
        match self {
            RuleFired::Move { rule, .. }
            | RuleFired::Check { rule, .. }
            | RuleFired::Collide { rule, .. }
            | RuleFired::Delete { rule, .. }
            | RuleFired::Spawn { rule, .. } => rule,
        }
    }
}

/// Parses `"rules[N]"` back into `N`, for sorting a step's fired rules into file order — they are
/// collected stage by stage (move/check, then collide, then delete/spawn), not in file order, so
/// this is the one place that restores it. A label the parser produced always matches; anything
/// else sorts last rather than panicking.
fn rule_index_of(label: &str) -> usize {
    label
        .strip_prefix("rules[")
        .and_then(|s| s.strip_suffix(']'))
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX)
}

/// Why an object was deleted this step — «Крайние случаи»: distinguishes a rule's own effect from
/// a `run` function it called, and both from an expired `lifetime`.
#[derive(Debug, Clone, PartialEq)]
pub enum DeleteCause {
    Rule(String),
    Code { function: String, rule: String },
    LifetimeExpired,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreatedObject {
    pub id: u32,
    pub name: Option<String>,
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeletedObject {
    pub id: u32,
    pub name: Option<String>,
    pub cause: DeleteCause,
}

/// The finished report for one step — `Game::last_report()`. `screen_change` is filled in by the
/// caller (only it knows screen names) via `Game::annotate_screen_change`; `outcome` is filled by
/// `Game::step` itself, straight off the same `outcome_flag` that ends the partiya.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepReport {
    pub step: u64,
    pub fired: Vec<RuleFired>,
    pub created: Vec<CreatedObject>,
    pub deleted: Vec<DeletedObject>,
    pub screen_change: Option<(String, String)>,
    pub outcome: Option<Outcome>,
}

/// The mutable accumulator threaded through one step's rule stages, the same way `code: &mut
/// Option<CodeEnv>` already threads through them — `deletes` collects `(id, cause)` as each
/// deletion is queued (an id may repeat if two sources both queue it; `finish` keeps the first);
/// `created` is filled by `Game::step` itself once stage 8 assigns real ids to `step::
/// PendingCreate`'s own `rule` label.
#[derive(Debug, Default)]
pub struct StepReportBuilder {
    pub fired: Vec<RuleFired>,
    /// `(id, name, cause)` — the name is read where the deletion is queued, while the object is
    /// still alive; reading it after `World::delete` would see none.
    pub deletes: Vec<(u32, Option<String>, DeleteCause)>,
    pub created: Vec<CreatedObject>,
    pub outcome: Option<Outcome>,
}

impl StepReportBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finalizes into a `StepReport`: dedupes `deletes` by id (first cause wins), synthesizes one
    /// `RuleFired::Spawn` per rule that created something (stage 8 only assigns real ids after
    /// every rule stage has already run, so `created` couldn't carry this earlier), and sorts
    /// `fired` back into file order (it was built stage by stage, not rule by rule).
    pub fn finish(mut self, step: u64) -> StepReport {
        let mut seen = std::collections::HashSet::new();
        let deleted = self
            .deletes
            .drain(..)
            .filter(|(id, _, _)| seen.insert(*id))
            .map(|(id, name, cause)| DeletedObject { id, name, cause })
            .collect();

        let mut spawn_order: Vec<String> = Vec::new();
        let mut spawn_objects: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
        for c in &self.created {
            spawn_objects
                .entry(c.rule.clone())
                .or_insert_with(|| {
                    spawn_order.push(c.rule.clone());
                    Vec::new()
                })
                .push(c.id);
        }
        for rule in spawn_order {
            let objects = spawn_objects.remove(&rule).unwrap_or_default();
            self.fired.push(RuleFired::Spawn { rule, objects });
        }
        self.fired.sort_by_key(|f| rule_index_of(f.rule_label()));

        StepReport {
            step,
            fired: self.fired,
            created: self.created,
            deleted,
            screen_change: None,
            outcome: self.outcome,
        }
    }
}

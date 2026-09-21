use super::property::PropertyId;
use super::value::Value;

/// Index into `files.sounds`, in declaration order — «Звук»: a `play_sound` action
/// resolves its name to one of these at load time, the same way an object reference resolves to a
/// `PropertyId`.
pub type SoundId = usize;

#[derive(Debug, Clone, Default)]
pub struct Selector {
    pub has: Vec<PropertyId>,
    pub without: Vec<PropertyId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl CompareOp {
    pub fn apply(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CompareOp::Lt => lhs < rhs,
            CompareOp::Le => lhs <= rhs,
            CompareOp::Gt => lhs > rhs,
            CompareOp::Ge => lhs >= rhs,
            CompareOp::Eq => lhs == rhs,
            CompareOp::Ne => lhs != rhs,
        }
    }
}

/// General condition (`when`), used by the "delete" rule. "move" has no condition at all,
/// and "create" uses the narrower `SpawnCondition` below.
#[derive(Debug, Clone)]
pub enum Condition {
    Compare {
        prop: PropertyId,
        op: CompareOp,
        value: f64,
    },
    OutsideScene,
    FewerThan {
        count: u32,
        of: Selector,
    },
    AfterMoveOf {
        of: Selector,
    },
}

/// "create" only ever fires from one of these two: `fewer_than` names no parent at all,
/// `after_move_of` names the mover as parent for `at_parent` / `from_parent`.
#[derive(Debug, Clone)]
pub enum SpawnCondition {
    FewerThan { count: u32, of: Selector },
    AfterMoveOf { of: Selector },
}

#[derive(Debug, Clone, Copy)]
pub enum SpawnPlace {
    AtParent,
    RandomCell,
}

#[derive(Debug, Clone)]
pub enum TemplateValue {
    Const(Value),
    FromParent(PropertyId),
}

#[derive(Debug, Clone)]
pub enum CollideEffect {
    Bounce,
    Delete,
    Add { prop: PropertyId, value: Value },
    Set { prop: PropertyId, value: Value },
    Give { prop: PropertyId },
    Take { prop: PropertyId },
    Run(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Loss,
}

#[derive(Debug, Clone)]
pub enum CommonAction {
    EndGame(Outcome),
    Add { prop: PropertyId, value: Value },
    PlaySound(SoundId),
    Run(String),
}

#[derive(Debug, Clone)]
pub enum Rule {
    Move {
        for_: Selector,
    },
    Collide {
        a: Selector,
        b: Selector,
        effects_a: Vec<CollideEffect>,
        effects_b: Vec<CollideEffect>,
        do_: Vec<CommonAction>,
    },
    Delete {
        for_: Selector,
        when: Condition,
        do_: Vec<CommonAction>,
    },
    Spawn {
        when: SpawnCondition,
        place: SpawnPlace,
        template: Vec<(PropertyId, TemplateValue)>,
        do_: Vec<CommonAction>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

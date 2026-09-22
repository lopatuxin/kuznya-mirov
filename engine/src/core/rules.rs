use super::property::PropertyId;
use super::value::{Value, Vec2};

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

/// General condition (`when`), used by "check" and "delete" — «Правила игры» → «Условия»:
/// `all`/`any`/`not` nest to any depth around the four leaves. "move" has no condition at all;
/// "spawn" wraps the same tree in `SpawnCondition` below, which additionally forbids `Compare`/
/// `OutsideScene` anywhere and tracks the one `after_move_of` allowed to supply a parent.
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
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

/// "create"'s `when` — a general `Condition` tree (`Compare`/`OutsideScene` never appear in it,
/// checked at load time), plus which selector supplies the parents when the tree turns out true:
/// `Some(of)` when exactly one `after_move_of` sits outside every `any`/`not` in the tree (its
/// `of`, unchanged by any surrounding `all`) — «Правила игры» → «Создание по форме», требование 7.
/// `None` when the tree has no such node, so a true tree fires the spawn once with no parent
/// (`at_parent`/`from_parent` are then unavailable, checked at load time too).
#[derive(Debug, Clone)]
pub struct SpawnCondition {
    pub condition: Condition,
    pub parent_of: Option<Selector>,
}

/// Where `spawn`/`shift`/`turn` place a created object or a named cell — «Создание по форме»,
/// требование 19: `AtParent`/`RandomCell` as before, `Cell` a scene cell named literally.
#[derive(Debug, Clone, Copy)]
pub enum SpawnPlace {
    AtParent,
    RandomCell,
    Cell(Vec2),
}

/// One `pick_one` variant — требование 20: a non-empty `cells` list, plus this variant's own
/// fields layered over `template` before a clique's own cell fields land on top of both.
#[derive(Debug, Clone)]
pub struct SpawnVariant {
    pub cells: Vec<SpawnCell>,
    pub fields: Vec<(PropertyId, TemplateValue)>,
}

/// One cell of a `pick_one` variant — `[x, y]` alone, or `{"at": [x, y], ...}` with fields of its
/// own layered on top of the variant's.
#[derive(Debug, Clone)]
pub struct SpawnCell {
    pub at: Vec2,
    pub fields: Vec<(PropertyId, TemplateValue)>,
}

#[derive(Debug, Clone)]
pub enum TemplateValue {
    Const(Value),
    FromParent(PropertyId),
}

/// The number an `add`/`set` action changes a property by or to — «Правила игры» → «Числа»,
/// требование 11: a plain constant, a value read off a table by another property, or a constant
/// times a multiplier property. `by`/`times` name a property of kind `number` on the very object
/// being changed; resolving finds no such property (missing, or removed at runtime) — the whole
/// action simply doesn't apply to that object, not an error.
#[derive(Debug, Clone)]
pub enum NumberExpr {
    Const(f64),
    Table { table: Vec<f64>, by: PropertyId },
    Multiplier { value: f64, times: PropertyId },
}

/// What `set` writes — a `NumberExpr` for a `number`/`time`/`timer` property (which then also
/// accepts a plain constant, via `NumberExpr::Const`), a literal `Value` for every other kind.
#[derive(Debug, Clone)]
pub enum SetValue {
    Const(Value),
    Number(NumberExpr),
}

#[derive(Debug, Clone)]
pub enum CollideEffect {
    Bounce,
    Delete,
    Add { prop: PropertyId, expr: NumberExpr },
    Set { prop: PropertyId, value: SetValue },
    Give { prop: PropertyId },
    Take { prop: PropertyId },
    Run(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Loss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnDir {
    Clockwise,
    CounterClockwise,
}

/// `["shift", {...}]` — требование 12, 15–17.
#[derive(Debug, Clone)]
pub struct ShiftSpec {
    pub group: Selector,
    pub by: Vec2,
    pub blocked_by: Option<Selector>,
    pub if_blocked: Vec<CommonAction>,
}

/// `["turn", {...}]` — требование 13, 15–17.
#[derive(Debug, Clone)]
pub struct TurnSpec {
    pub group: Selector,
    pub around: Selector,
    pub dir: TurnDir,
    pub blocked_by: Option<Selector>,
    pub if_blocked: Vec<CommonAction>,
}

#[derive(Debug, Clone)]
pub enum CommonAction {
    EndGame(Outcome),
    Add {
        prop: PropertyId,
        expr: NumberExpr,
    },
    /// «Правила игры» → требование 10: writes every current holder of `prop`.
    SetAll {
        prop: PropertyId,
        value: SetValue,
    },
    /// требование 10: clears the flag from every current holder.
    TakeAll {
        prop: PropertyId,
    },
    /// требование 10: hands the flag to every object the selector picks out, present or not.
    GiveWhere {
        prop: PropertyId,
        selector: Selector,
    },
    PlaySound(SoundId),
    Run(String),
    Shift(ShiftSpec),
    Turn(TurnSpec),
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
    /// «Правила игры» → требование 4: runs at stage 4, interleaved with `Move` in file order.
    Check {
        for_: Selector,
        when: Option<Condition>,
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
        pick_one: Option<Vec<SpawnVariant>>,
        do_: Vec<CommonAction>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

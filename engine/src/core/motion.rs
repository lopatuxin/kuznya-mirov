//! «Трёхмерная сцена» → «Модели и движения»: какое движение играет у объекта со скелетом, в какой его миг и как
//! оно переходит в другое. Время идёт шагами партии: мир стоит — модели замирают.

use super::property;
use super::runner;
use super::skeleton::{Blend, FADE_STEPS, Layer, MAX_LAYERS, Playing, Skeleton};
use super::value::Vec2;
use super::world::World;

/// Доля, меньше которой затухающее движение в позе не видно и место в смеси ему больше не нужно.
const NEGLIGIBLE: f32 = 1e-3;

/// Доля идущего движения, с которой оно считается занявшим всю позу: сумма дробей не доходит до единицы ровно.
const FULL: f32 = 0.9999;

/// Секунды движений на шаг партии.
const STEP_SECONDS: f32 = runner::STEP_SECONDS as f32;

/// Какое движение объект хочет играть: стойку, ходьбу или названное в `animation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Idle,
    Walk,
    Named(usize),
}

/// Одно из движений смеси: что играет, сколько секунд и его доля в позе.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Running {
    want: Want,
    time: f32,
    weight: f32,
}

impl Running {
    const STILL: Running = Running {
        want: Want::Idle,
        time: 0.0,
        weight: 1.0,
    };
}

/// Движение объекта: смесь из идущего движения, чья доля растёт, и затухающих, сумма долей — единица; и где
/// объект стоял на прошлом шаге. Идущее — первое.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionState {
    layers: [Running; MAX_LAYERS],
    len: usize,
    last: Option<Vec2>,
}

impl Default for MotionState {
    fn default() -> Self {
        MotionState {
            layers: [Running::STILL; MAX_LAYERS],
            len: 1,
            last: None,
        }
    }
}

impl MotionState {
    /// Состояние, в котором без перехода играет одно движение `clip` и прошло `time` секунд от его начала.
    pub fn pinned(clip: usize, time: f32) -> MotionState {
        let mut state = MotionState::default();
        state.layers[0] = Running {
            want: Want::Named(clip),
            time,
            weight: 1.0,
        };
        state
    }

    /// Поза объекта в этот миг: у объекта, что ещё не сделал ни шага, — первая поза `idle`.
    pub fn blend(&self, skeleton: &Skeleton) -> Blend {
        Blend::from_layers(self.layers[..self.len].iter().map(|running| Layer {
            playing: Self::playing(skeleton, running.want, running.time),
            weight: running.weight,
        }))
    }

    fn playing(skeleton: &Skeleton, want: Want, time: f32) -> Playing {
        let (clip, looped) = match want {
            Want::Idle => (skeleton.idle, true),
            Want::Walk => (skeleton.walk.or(skeleton.idle), true),
            Want::Named(clip) => (Some(clip), false),
        };
        Playing { clip, time, looped }
    }

    fn moved_to(&self, position: Vec2) -> bool {
        self.last.is_some_and(|last| last != position)
    }

    fn advance(&mut self, want: Want, position: Vec2) {
        for running in &mut self.layers[..self.len] {
            running.time += STEP_SECONDS;
        }
        if want != self.layers[0].want {
            self.switch_to(want);
        }
        self.fade();
        self.last = Some(position);
    }

    /// Новое движение встаёт идущим, а идущее уходит в затухающие со своей долей. Стойка и ходьба, что ещё не
    /// затухли, возвращаются со своим временем и долей: бег не рвётся шагом без сдвига. Названное движение
    /// каждый раз начинается с начала.
    fn switch_to(&mut self, want: Want) {
        let resumed = match want {
            Want::Named(_) => None,
            _ => (1..self.len).find(|&at| self.layers[at].want == want),
        };
        let running = match resumed {
            Some(at) => self.take(at),
            None => {
                if self.len == MAX_LAYERS {
                    self.drop_lightest();
                }
                Running {
                    want,
                    time: 0.0,
                    weight: 0.0,
                }
            }
        };
        self.layers.copy_within(0..self.len, 1);
        self.layers[0] = running;
        self.len += 1;
    }

    fn take(&mut self, at: usize) -> Running {
        let running = self.layers[at];
        self.layers.copy_within(at + 1..self.len, at);
        self.len -= 1;
        running
    }

    /// Самое лёгкое из движений смеси уходит, доли остальных приводятся к единице.
    fn drop_lightest(&mut self) {
        let lightest = (0..self.len)
            .min_by(|&a, &b| self.layers[a].weight.total_cmp(&self.layers[b].weight))
            .unwrap_or(0);
        self.take(lightest);
        let total: f32 = self.layers[..self.len].iter().map(|r| r.weight).sum();
        if total > 0.0 {
            for running in &mut self.layers[..self.len] {
                running.weight /= total;
            }
        }
    }

    /// Доля идущего движения растёт на шаг перехода, доли затухающих сжимаются так, чтобы сумма осталась единицей.
    fn fade(&mut self) {
        let before = self.layers[0].weight;
        let after = before + 1.0 / FADE_STEPS as f32;
        if after >= FULL || 1.0 - before <= f32::EPSILON {
            self.layers[0].weight = 1.0;
            self.len = 1;
            return;
        }
        let shrink = (1.0 - after) / (1.0 - before);
        self.layers[0].weight = after;
        for running in &mut self.layers[1..self.len] {
            running.weight *= shrink;
        }
        let mut kept = 1;
        for at in 1..self.len {
            if self.layers[at].weight >= NEGLIGIBLE {
                self.layers[kept] = self.layers[at];
                kept += 1;
            }
        }
        self.len = kept;
    }
}

/// Один шаг партии для всех объектов со скелетом: движение, названное в `animation`; без него — ходьба, пока
/// объект сдвинулся за шаг, и стойка, пока стоит. Название, которого нет в модели, движением не считается.
pub fn advance(world: &mut World) {
    for id in 0..world.slot_count() as u32 {
        if !world.is_alive(id) {
            continue;
        }
        let (Some(position), Some(named)) =
            (world.vec2(id, property::POSITION), named_motion(world, id))
        else {
            continue;
        };
        let mut state = world.motion(id);
        let want = match named {
            Some(clip) => Want::Named(clip),
            None if state.moved_to(position) => Want::Walk,
            None => Want::Idle,
        };
        state.advance(want, position);
        world.set_motion(id, state);
    }
}

/// `None` у объекта без скелетной модели; иначе — номер движения из `animation`, если оно названо и есть в модели.
fn named_motion(world: &World, id: u32) -> Option<Option<usize>> {
    let skeleton = world.model_of(id)?.skeleton.as_ref()?;
    let name = world
        .text(id, property::ANIMATION)
        .filter(|name| !name.is_empty());
    Some(name.and_then(|name| skeleton.clip_named(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::math3::IDENTITY;
    use crate::core::skeleton::{Bone, Clip, Trs};

    fn skeleton() -> Skeleton {
        let clip = |name: &str| Clip {
            name: name.to_string(),
            start: 0.0,
            length: 1.0,
            tracks: Vec::new(),
            retouch: None,
        };
        Skeleton {
            bones: vec![Bone {
                name: "root".to_string(),
                parent: None,
                rest: Trs::REST,
            }],
            joints: vec![0],
            head: IDENTITY,
            tails: vec![IDENTITY],
            clips: vec![clip("Idle"), clip("Jog"), clip("Roll")],
            idle: Some(0),
            walk: Some(1),
            legs: None,
            spine: None,
            hands: None,
        }
    }

    fn weights(blend: &Blend) -> Vec<f32> {
        blend.layers().iter().map(|layer| layer.weight).collect()
    }

    fn clips(blend: &Blend) -> Vec<Option<usize>> {
        blend
            .layers()
            .iter()
            .map(|layer| layer.playing.clip)
            .collect()
    }

    fn total(blend: &Blend) -> f32 {
        weights(blend).iter().sum()
    }

    #[test]
    fn a_fresh_object_stands_in_the_first_pose_of_idle() {
        let blend = MotionState::default().blend(&skeleton());
        let current = blend.current();
        assert_eq!(current.playing.clip, Some(0));
        assert_eq!(current.playing.time, 0.0);
        assert!(current.playing.looped && blend.layers().len() == 1);
        assert_eq!(current.weight, 1.0);
    }

    #[test]
    fn a_change_of_motion_fades_over_twelve_steps_and_the_old_one_keeps_playing() {
        let skeleton = skeleton();
        let mut state = MotionState::default();
        state.advance(Want::Idle, [0.0, 0.0]);
        state.advance(Want::Walk, [1.0, 0.0]);
        let blend = state.blend(&skeleton);
        assert_eq!(clips(&blend), [Some(1), Some(0)]);
        assert!(blend.current().weight < 0.1);
        assert!((total(&blend) - 1.0).abs() < 1e-5);
        for _ in 0..11 {
            state.advance(Want::Walk, [2.0, 0.0]);
        }
        let blend = state.blend(&skeleton);
        assert_eq!(blend.layers().len(), 1);
        assert_eq!(blend.current().weight, 1.0);
    }

    #[test]
    fn a_named_motion_plays_once_and_walk_falls_back_to_idle_without_its_own_clip() {
        let mut skeleton = skeleton();
        let mut state = MotionState::default();
        state.advance(Want::Named(2), [0.0, 0.0]);
        assert!(!state.blend(&skeleton).current().playing.looped);
        skeleton.walk = None;
        state.advance(Want::Walk, [1.0, 0.0]);
        assert_eq!(state.blend(&skeleton).current().playing.clip, Some(0));
    }

    #[test]
    fn an_object_counts_as_moved_only_when_its_place_changed_since_the_last_step() {
        let mut state = MotionState::default();
        assert!(
            !state.moved_to([1.0, 1.0]),
            "первый шаг — сравнивать не с чем"
        );
        state.advance(Want::Idle, [1.0, 1.0]);
        assert!(!state.moved_to([1.0, 1.0]));
        assert!(state.moved_to([1.0, 2.0]));
    }

    #[test]
    fn a_change_in_the_middle_of_a_fade_keeps_the_fading_motion_and_the_weights_sum_to_one() {
        let skeleton = skeleton();
        let mut state = MotionState::default();
        state.advance(Want::Idle, [0.0, 0.0]);
        for _ in 0..5 {
            state.advance(Want::Walk, [1.0, 0.0]);
        }
        let before = state.blend(&skeleton);
        assert_eq!(clips(&before), [Some(1), Some(0)]);
        state.advance(Want::Named(2), [1.0, 0.0]);
        let blend = state.blend(&skeleton);
        assert_eq!(
            clips(&blend),
            [Some(2), Some(1), Some(0)],
            "ходьба со своей долей уходит в затухающие, стойка не пропала"
        );
        let walk_before = before.current().weight;
        assert!(
            blend.layers()[1].weight > walk_before * 0.8 && blend.layers()[1].weight < walk_before,
            "доля ходьбы сжалась, а не обнулилась: {} → {}",
            walk_before,
            blend.layers()[1].weight
        );
        assert!((total(&blend) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_motion_that_returns_inside_its_fade_continues_with_its_own_time_and_weight() {
        let skeleton = skeleton();
        let mut state = MotionState::default();
        state.advance(Want::Idle, [0.0, 0.0]);
        for _ in 0..30 {
            state.advance(Want::Walk, [1.0, 0.0]);
        }
        let walked = state.blend(&skeleton).current().playing.time;
        state.advance(Want::Idle, [1.0, 0.0]);
        state.advance(Want::Walk, [1.0, 0.0]);
        let back = state.blend(&skeleton);
        assert_eq!(back.current().playing.clip, Some(1));
        assert!(
            (back.current().playing.time - (walked + 2.0 * STEP_SECONDS)).abs() < 1e-5,
            "время бега не сбросилось: было {walked}, стало {}",
            back.current().playing.time
        );
        assert_eq!(back.layers().len(), 1, "бег вернул себе всю долю");
    }

    #[test]
    fn a_named_motion_asked_again_starts_from_the_beginning_even_while_its_old_run_fades() {
        let skeleton = skeleton();
        let mut state = MotionState::default();
        for _ in 0..10 {
            state.advance(Want::Named(2), [0.0, 0.0]);
        }
        state.advance(Want::Idle, [0.0, 0.0]);
        state.advance(Want::Named(2), [0.0, 0.0]);
        let blend = state.blend(&skeleton);
        assert_eq!(blend.current().playing.clip, Some(2));
        assert!(
            blend.current().playing.time == 0.0,
            "{}",
            blend.current().playing.time
        );
    }

    #[test]
    fn past_four_motions_the_lightest_is_dropped_and_the_rest_are_renormalised() {
        let skeleton = skeleton();
        let mut state = MotionState::default();
        let sequence = [
            Want::Walk,
            Want::Named(2),
            Want::Idle,
            Want::Named(1),
            Want::Named(0),
        ];
        for want in sequence {
            state.advance(want, [0.0, 0.0]);
            let blend = state.blend(&skeleton);
            assert!(blend.layers().len() <= MAX_LAYERS);
            assert!((total(&blend) - 1.0).abs() < 1e-4, "{:?}", weights(&blend));
        }
        assert_eq!(state.blend(&skeleton).layers().len(), MAX_LAYERS);
    }
}

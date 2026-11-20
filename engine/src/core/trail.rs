//! «Интерфейс игры» → «Шкала», «След потери»: память о доле каждой шкалы, у которой есть след. Живёт рядом с
//! раскладкой интерфейса, а не в мире: это вид, а не состояние игры, в запись партии и сохранение он не входит.
//! Время следа — шаги партии, `Game::step_count`: пока мир стоит, след стоит.

use std::collections::HashMap;

use super::screens::ScreenId;

/// 0,3 с стояния следа после потери.
const HOLD_STEPS: u64 = 18;
/// 0,3 с, за которые след равномерно сжимается к новой доле.
const SHRINK_STEPS: u64 = 18;

/// Кому принадлежит шкала: миру (полоска над объектом) или экрану.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrailOwner {
    World,
    Screen(ScreenId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TrailKey {
    owner: TrailOwner,
    element: usize,
    inner: usize,
    object: u32,
}

#[derive(Debug, Clone, Copy)]
struct Trail {
    /// Поколение объекта: на месте прежнего встал новый — след начинается заново.
    generation: u32,
    /// Доля, которую шкала показывала в последний раз.
    share: f32,
    /// Правый край следа в миг последней потери; не больше `share` — следа нет.
    edge: f32,
    /// Шаг партии последней потери.
    since: u64,
    pass: u64,
}

impl Trail {
    fn right_edge(&self, steps: u64) -> f32 {
        let shrunk = steps.saturating_sub(self.since).saturating_sub(HOLD_STEPS);
        if shrunk >= SHRINK_STEPS {
            return self.share;
        }
        let left = shrunk as f32 / SHRINK_STEPS as f32;
        self.edge + (self.share - self.edge) * left
    }
}

/// Следы всех шкал игры. Один на страницу; заново собранный мир начинает его заново (`follow_world`).
#[derive(Debug, Default)]
pub struct Trails {
    epoch: u64,
    steps: u64,
    passes: u64,
    entries: HashMap<TrailKey, Trail>,
}

impl Trails {
    /// Партия началась заново: следов нет.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Мир собран заново (`Game::world_epoch` стал другим): новая партия, повтор или сцена редактора — следов нет.
    pub fn follow_world(&mut self, epoch: u64) {
        if epoch != self.epoch {
            self.epoch = epoch;
            self.entries.clear();
        }
    }

    /// Кадр одного владельца в момент `steps` шагов партии. Шкала, которую кадр не назвал, — объект
    /// пропал или у него нет `value` или `max` — след забывает.
    pub fn pass(&mut self, owner: TrailOwner, steps: u64) -> TrailPass<'_> {
        self.steps = steps;
        self.passes += 1;
        TrailPass {
            pass: self.passes,
            trails: self,
            owner,
        }
    }
}

pub struct TrailPass<'a> {
    trails: &'a mut Trails,
    owner: TrailOwner,
    pass: u64,
}

impl TrailPass<'_> {
    /// Доля шкалы `element` (`inner` — номер элемента в строке списка) у объекта `object` сейчас `share`.
    /// Возвращает правый край следа, если он виден: след лежит между `share` и этим краем.
    pub fn observe(
        &mut self,
        element: usize,
        inner: usize,
        object: u32,
        generation: u32,
        share: f32,
    ) -> Option<f32> {
        let steps = self.trails.steps;
        let key = TrailKey {
            owner: self.owner,
            element,
            inner,
            object,
        };
        let mut trail = match self.trails.entries.get(&key) {
            Some(trail) if trail.generation == generation => *trail,
            _ => Trail {
                generation,
                share,
                edge: share,
                since: steps,
                pass: self.pass,
            },
        };
        if share < trail.share {
            trail.edge = trail.right_edge(steps);
            trail.since = steps;
        } else if share > trail.share {
            trail.edge = share;
        }
        trail.share = share;
        trail.pass = self.pass;
        self.trails.entries.insert(key, trail);
        let edge = trail.right_edge(steps);
        (edge > share).then_some(edge)
    }
}

impl Drop for TrailPass<'_> {
    fn drop(&mut self) {
        let (owner, pass) = (self.owner, self.pass);
        self.trails
            .entries
            .retain(|key, trail| key.owner != owner || trail.pass == pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observe(trails: &mut Trails, steps: u64, share: f32) -> Option<f32> {
        trails
            .pass(TrailOwner::World, steps)
            .observe(0, 0, 7, 0, share)
    }

    #[test]
    fn a_new_bar_has_no_trail() {
        let mut trails = Trails::default();
        assert_eq!(observe(&mut trails, 0, 1.0), None);
        assert_eq!(observe(&mut trails, 5, 1.0), None);
    }

    #[test]
    fn a_loss_holds_for_three_tenths_of_a_second_then_shrinks_to_nothing_in_three_tenths() {
        let mut trails = Trails::default();
        observe(&mut trails, 0, 1.0);
        let after = 46.0 / 60.0;
        assert_eq!(observe(&mut trails, 10, after), Some(1.0));
        assert_eq!(
            observe(&mut trails, 28, after),
            Some(1.0),
            "0,3 с — ещё стоит"
        );
        let halfway = observe(&mut trails, 37, after).expect("сжимается");
        assert!((halfway - (after + 1.0) / 2.0).abs() < 1e-6, "{halfway}");
        assert_eq!(observe(&mut trails, 46, after), None, "0,6 с — след пропал");
        assert_eq!(observe(&mut trails, 90, after), None);
    }

    #[test]
    fn a_second_loss_keeps_the_right_edge_and_restarts_the_standing_time() {
        let mut trails = Trails::default();
        observe(&mut trails, 0, 1.0);
        assert_eq!(observe(&mut trails, 1, 46.0 / 60.0), Some(1.0));
        assert_eq!(observe(&mut trails, 13, 32.0 / 60.0), Some(1.0));
        assert_eq!(observe(&mut trails, 13 + 18, 32.0 / 60.0), Some(1.0));
        assert!(observe(&mut trails, 13 + 19, 32.0 / 60.0).expect("сжимается") < 1.0);
        assert_eq!(observe(&mut trails, 13 + 36, 32.0 / 60.0), None);
    }

    #[test]
    fn a_loss_while_the_trail_shrinks_starts_from_where_it_is_now() {
        let mut trails = Trails::default();
        observe(&mut trails, 0, 1.0);
        observe(&mut trails, 0, 0.5);
        let shrunk = observe(&mut trails, 27, 0.5).expect("сжимается");
        assert!((shrunk - 0.75).abs() < 1e-6);
        assert_eq!(observe(&mut trails, 27, 0.25), Some(shrunk));
        assert_eq!(observe(&mut trails, 27 + 18, 0.25), Some(shrunk));
    }

    #[test]
    fn a_gain_drops_the_trail_at_once() {
        let mut trails = Trails::default();
        observe(&mut trails, 0, 1.0);
        observe(&mut trails, 1, 0.5);
        assert_eq!(observe(&mut trails, 2, 0.6), None);
        assert_eq!(observe(&mut trails, 3, 0.6), None);
    }

    #[test]
    fn a_standing_world_keeps_the_trail_where_it_is() {
        let mut trails = Trails::default();
        observe(&mut trails, 100, 1.0);
        observe(&mut trails, 100, 0.5);
        for _ in 0..200 {
            assert_eq!(observe(&mut trails, 100, 0.5), Some(1.0));
        }
    }

    #[test]
    fn a_value_below_zero_trails_from_zero() {
        let mut trails = Trails::default();
        observe(&mut trails, 0, 0.4);
        assert_eq!(observe(&mut trails, 1, 0.0), Some(0.4));
    }

    #[test]
    fn every_object_and_every_element_has_its_own_trail() {
        let mut trails = Trails::default();
        {
            let mut pass = trails.pass(TrailOwner::World, 0);
            pass.observe(0, 0, 1, 0, 1.0);
            pass.observe(0, 0, 2, 0, 1.0);
            pass.observe(1, 0, 1, 0, 1.0);
        }
        let mut pass = trails.pass(TrailOwner::World, 1);
        assert_eq!(pass.observe(0, 0, 1, 0, 0.5), Some(1.0));
        assert_eq!(pass.observe(0, 0, 2, 0, 1.0), None);
        assert_eq!(pass.observe(1, 0, 1, 0, 1.0), None);
    }

    #[test]
    fn a_bar_the_frame_did_not_name_forgets_its_trail() {
        let mut trails = Trails::default();
        {
            let mut pass = trails.pass(TrailOwner::World, 0);
            pass.observe(0, 0, 1, 0, 1.0);
        }
        {
            let mut pass = trails.pass(TrailOwner::World, 1);
            assert_eq!(pass.observe(0, 0, 1, 0, 0.5), Some(1.0));
        }
        trails.pass(TrailOwner::World, 2);
        assert_eq!(
            trails.pass(TrailOwner::World, 3).observe(0, 0, 1, 0, 0.5),
            None
        );
    }

    #[test]
    fn another_owner_does_not_forget_the_trails_of_this_one() {
        let mut trails = Trails::default();
        {
            let mut pass = trails.pass(TrailOwner::Screen(0), 0);
            pass.observe(0, 0, 1, 0, 1.0);
        }
        {
            let mut pass = trails.pass(TrailOwner::Screen(0), 1);
            pass.observe(0, 0, 1, 0, 0.5);
        }
        trails.pass(TrailOwner::World, 2);
        trails.pass(TrailOwner::Screen(1), 3);
        let mut pass = trails.pass(TrailOwner::Screen(0), 4);
        assert_eq!(pass.observe(0, 0, 1, 0, 0.5), Some(1.0));
    }

    #[test]
    fn a_new_object_in_the_old_slot_starts_without_a_trail() {
        let mut trails = Trails::default();
        observe(&mut trails, 0, 1.0);
        let mut pass = trails.pass(TrailOwner::World, 1);
        assert_eq!(pass.observe(0, 0, 7, 1, 0.3), None);
    }

    #[test]
    fn a_rebuilt_world_or_a_clear_starts_over() {
        let mut trails = Trails::default();
        observe(&mut trails, 50, 1.0);
        assert_eq!(observe(&mut trails, 51, 0.5), Some(1.0));
        trails.follow_world(0);
        assert_eq!(observe(&mut trails, 52, 0.4), Some(1.0), "тот же мир");
        trails.follow_world(1);
        assert_eq!(observe(&mut trails, 52, 0.2), None, "новая партия");
        assert_eq!(observe(&mut trails, 53, 0.1), Some(0.2));
        trails.clear();
        assert_eq!(observe(&mut trails, 5, 0.05), None);
    }
}

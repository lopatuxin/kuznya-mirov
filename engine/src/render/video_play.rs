//! «Видео на объекте», требование 5 и крайние случаи: когда просить браузерный проигрыватель играть,
//! когда ставить на паузу. Решение отделено от проигрывателя, чтобы его можно было проверить без
//! браузера.

/// Что стало с последней просьбой играть. Пока браузер не ответил, новая не шлётся; после отказа
/// видео стоит, пока часы кадров не встанут и не пойдут снова.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayRequest {
    None,
    Pending,
    Refused,
}

/// Что сделать с проигрывателем на этом кадре.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerAction {
    Play,
    Pause,
    Nothing,
}

/// Кадр: идут ли часы кадров (`playing`), стоит ли проигрыватель (`paused`) и что стало с прошлой
/// просьбой играть. Возвращает, что сделать, и состояние просьбы после этого. Браузер мог
/// остановить видео сам (скрытая вкладка): при идущих часах его снова просят играть. Отказ браузера —
/// не ошибка: видео стоит на текущем кадре, а снова просят, когда часы встанут и пойдут.
pub fn next_step(playing: bool, paused: bool, request: PlayRequest) -> (PlayerAction, PlayRequest) {
    match (playing, paused) {
        (true, true) if request == PlayRequest::None => (PlayerAction::Play, PlayRequest::Pending),
        (true, _) => (PlayerAction::Nothing, request),
        (false, paused) => {
            let action = if paused {
                PlayerAction::Nothing
            } else {
                PlayerAction::Pause
            };
            let request = if request == PlayRequest::Refused {
                PlayRequest::None
            } else {
                request
            };
            (action, request)
        }
    }
}

/// Ответ браузера на просьбу играть. `refused` — отказ; просьбу, которую прервала пауза самого
/// движка (часы встали, пока браузер готовился играть), отказом не считают.
pub fn after_answer(refused: bool) -> PlayRequest {
    if refused {
        PlayRequest::Refused
    } else {
        PlayRequest::None
    }
}

/// Какой из двух проигрывателей видео панели: обычный файл или `reverse` — тот же ролик задом наперёд.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Roll {
    Forward,
    Backward,
}

/// Что проигрыватель отвечает этому кадру: место в своём файле в секундах, идёт ли переход на новое место и
/// стоит ли он.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerReading {
    pub time: f64,
    pub seeking: bool,
    pub paused: bool,
}

/// Что сделать с проигрывателями видео панели в этом кадре: сначала пауза, потом переход на место, потом
/// просьба играть. `show` — чей кадр писать в атлас; `None` — оставить тот, что показан.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PanelStep {
    pub pause: Option<Roll>,
    pub seek: Option<(Roll, f64)>,
    pub play: Option<Roll>,
    pub show: Option<Roll>,
}

/// Сколько секунд до конца считается концом: файл `reverse` может быть короче обычного на кадр.
const END_SLACK_SECONDS: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CourseState {
    /// Картинка панели — кадры этого проигрывателя: он играет или стоит в конце.
    Rolling(Roll),
    /// Этот проигрыватель переходит на нужное место; пока он не готов, картинка остаётся прежней.
    Moving(Roll),
}

/// «Картинки» → «Видео на панели»: ход видео панели — место во времени ролика, а не часы кадров. Курсор над
/// местом — обычный файл идёт вперёд с этого места, курсора нет — `reverse` идёт с места «длина минус
/// текущее»; дойдя до конца, стоит. С `rest_at` обычный файл, дойдя до конца, считается стоящим на
/// `rest_at` (`rested`), а его время уже не равно месту (`stale`), пока он не перейдёт на место. Решает, что
/// делать с двумя проигрывателями; сами проигрыватели двигает слой отрисовки.
#[derive(Debug, Clone)]
pub struct PanelCourse {
    length: f64,
    position: f64,
    state: CourseState,
    touched: bool,
    rested: bool,
    stale: bool,
}

impl PanelCourse {
    /// Ролик длиной `length` секунд; стоит на первом кадре.
    pub fn new(length: f64) -> Self {
        PanelCourse {
            length: if length.is_finite() {
                length.max(0.0)
            } else {
                0.0
            },
            position: 0.0,
            state: CourseState::Rolling(Roll::Forward),
            touched: false,
            rested: false,
            stale: false,
        }
    }

    /// Чей проигрыватель читать в `step`: тот, что показан или переходит на место.
    pub fn current(&self) -> Roll {
        match self.state {
            CourseState::Rolling(roll) | CourseState::Moving(roll) => roll,
        }
    }

    /// Кадр: `hovering` — курсор над местом наведения, `rest_at` — секунда ролика с позой последнего кадра,
    /// `reading` — ответ `current()`.
    pub fn step(
        &mut self,
        hovering: bool,
        rest_at: Option<f64>,
        reading: PlayerReading,
    ) -> PanelStep {
        self.touched = true;
        let want = if hovering {
            Roll::Forward
        } else {
            Roll::Backward
        };
        let mut step = PanelStep::default();
        match self.state {
            CourseState::Rolling(shown) => {
                if !reading.seeking && !(self.stale && shown == Roll::Forward) {
                    self.position = self.position_in(shown, reading.time);
                    if let Some(rest_at) = rest_at
                        && shown == Roll::Forward
                        && self.reached(Roll::Forward)
                    {
                        self.rested = true;
                        self.stale = true;
                        self.position = rest_at.clamp(0.0, self.length);
                    }
                }
                if !hovering {
                    self.rested = false;
                }
                if (shown != want || self.stale) && !self.reached(want) {
                    self.move_to(want, &mut step);
                    step.pause = (!reading.paused).then_some(shown);
                    return step;
                }
                step.show = (!reading.seeking).then_some(shown);
                if reading.seeking {
                    return step;
                }
                if shown == want && !self.reached(want) && reading.paused {
                    step.play = Some(shown);
                } else if shown != want && !reading.paused {
                    step.pause = Some(shown);
                }
            }
            CourseState::Moving(target) if target != want => self.move_to(want, &mut step),
            CourseState::Moving(target) => {
                if reading.seeking {
                    return step;
                }
                self.state = CourseState::Rolling(target);
                step.show = Some(target);
                step.play = (!self.reached(target)).then_some(target);
            }
        }
        step
    }

    /// Экран с панелью ушёл: видео снова стоит на первом кадре. `true` — ход успел сдвинуться и проигрыватели
    /// надо вернуть на начало; повторный вызов подряд ничего не меняет.
    pub fn reset(&mut self) -> bool {
        self.state = CourseState::Rolling(Roll::Forward);
        self.position = 0.0;
        self.rested = false;
        self.stale = false;
        std::mem::replace(&mut self.touched, false)
    }

    fn move_to(&mut self, roll: Roll, step: &mut PanelStep) {
        self.state = CourseState::Moving(roll);
        self.rested = false;
        self.stale = false;
        let time = match roll {
            Roll::Forward => self.position,
            Roll::Backward => self.length - self.position,
        };
        step.seek = Some((roll, time));
    }

    fn position_in(&self, roll: Roll, time: f64) -> f64 {
        let position = match roll {
            Roll::Forward => time,
            Roll::Backward => self.length - time,
        };
        position.clamp(0.0, self.length)
    }

    /// Ролик дошёл до конца своего хода: вперёд — до конца ролика или на `rest_at` после него, назад — до начала.
    fn reached(&self, roll: Roll) -> bool {
        match roll {
            Roll::Forward => self.rested || self.position >= self.length - END_SLACK_SECONDS,
            Roll::Backward => self.position <= END_SLACK_SECONDS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn while_the_browser_has_not_answered_play_is_asked_once() {
        let (action, request) = next_step(true, true, PlayRequest::None);
        assert_eq!(action, PlayerAction::Play);
        assert_eq!(
            next_step(true, true, request),
            (PlayerAction::Nothing, PlayRequest::Pending)
        );
    }

    #[test]
    fn a_refused_video_stands_until_the_clocks_stop_and_go_again() {
        let refused = after_answer(true);
        assert_eq!(
            next_step(true, true, refused),
            (PlayerAction::Nothing, PlayRequest::Refused),
            "часы идут — отказ помнится, просьба не повторяется"
        );
        let (action, request) = next_step(false, true, refused);
        assert_eq!(action, PlayerAction::Nothing);
        assert_eq!(request, PlayRequest::None, "часы встали — отказ забыт");
        assert_eq!(next_step(true, true, request).0, PlayerAction::Play);
    }

    #[test]
    fn a_video_the_browser_paused_itself_is_asked_to_play_again() {
        let played = after_answer(false);
        assert_eq!(next_step(true, false, played).0, PlayerAction::Nothing);
        assert_eq!(next_step(true, true, played).0, PlayerAction::Play);
    }

    #[test]
    fn stopped_clocks_pause_a_playing_video_and_leave_a_paused_one_alone() {
        assert_eq!(
            next_step(false, false, PlayRequest::None).0,
            PlayerAction::Pause
        );
        assert_eq!(
            next_step(false, true, PlayRequest::None).0,
            PlayerAction::Nothing
        );
    }
}

#[cfg(test)]
mod panel_course_tests {
    use super::*;

    const LENGTH: f64 = 7.5;
    const FRAME: f64 = 1.0 / 60.0;
    /// Сколько кадров браузер переходит на новое место.
    const SEEK_FRAMES: u32 = 3;

    struct FakePlayer {
        time: f64,
        playing: bool,
        seek: Option<(u32, f64)>,
    }

    impl FakePlayer {
        fn reading(&self) -> PlayerReading {
            PlayerReading {
                time: self.time,
                seeking: self.seek.is_some(),
                paused: !self.playing,
            }
        }

        fn advance(&mut self) {
            if let Some((left, to)) = self.seek {
                if left > 1 {
                    self.seek = Some((left - 1, to));
                } else {
                    self.seek = None;
                    self.time = to;
                }
            } else if self.playing {
                self.time = (self.time + FRAME).min(LENGTH);
                self.playing = self.time < LENGTH;
            }
        }
    }

    /// Два проигрывателя и ход видео, как их ведёт слой отрисовки; `shown` — кадр, который сейчас в атласе.
    struct Panel {
        course: PanelCourse,
        rest_at: Option<f64>,
        forward: FakePlayer,
        backward: FakePlayer,
        shown: Option<(Roll, f64)>,
        seeks: Vec<(Roll, f64)>,
    }

    impl Panel {
        fn new() -> Self {
            let player = || FakePlayer {
                time: 0.0,
                playing: false,
                seek: None,
            };
            Panel {
                course: PanelCourse::new(LENGTH),
                rest_at: None,
                forward: player(),
                backward: player(),
                shown: None,
                seeks: Vec::new(),
            }
        }

        fn resting_at(rest_at: f64) -> Self {
            Panel {
                rest_at: Some(rest_at),
                ..Panel::new()
            }
        }

        fn player(&mut self, roll: Roll) -> &mut FakePlayer {
            match roll {
                Roll::Forward => &mut self.forward,
                Roll::Backward => &mut self.backward,
            }
        }

        fn frame(&mut self, hovering: bool) -> PanelStep {
            let reading = self.player(self.course.current()).reading();
            let step = self.course.step(hovering, self.rest_at, reading);
            if let Some(roll) = step.pause {
                self.player(roll).playing = false;
            }
            if let Some((roll, time)) = step.seek {
                self.seeks.push((roll, time));
                self.player(roll).seek = Some((SEEK_FRAMES, time));
            }
            if let Some(roll) = step.play {
                self.player(roll).playing = true;
            }
            if let Some(roll) = step.show {
                self.shown = Some((roll, self.player(roll).time));
            }
            self.forward.advance();
            self.backward.advance();
            step
        }

        fn run(&mut self, seconds: f64, hovering: bool) {
            for _ in 0..(seconds / FRAME).round() as u32 {
                self.frame(hovering);
            }
        }

        /// Место во времени ролика, как оно видно на панели: кадр обычного файла или обратного.
        fn picture(&self) -> f64 {
            match self.shown.expect("кадр показан") {
                (Roll::Forward, time) => time,
                (Roll::Backward, time) => LENGTH - time,
            }
        }
    }

    #[test]
    fn a_new_course_stands_on_the_first_frame_while_the_cursor_is_away() {
        let mut panel = Panel::new();
        panel.run(1.0, false);
        assert_eq!(panel.shown, Some((Roll::Forward, 0.0)));
        assert!(!panel.forward.playing && !panel.backward.playing);
        assert!(panel.seeks.is_empty());
    }

    #[test]
    fn the_cursor_on_the_place_plays_forward_and_leaves_the_last_frame_standing() {
        let mut panel = Panel::new();
        panel.run(LENGTH + 1.0, true);
        assert_eq!(panel.shown.map(|(roll, _)| roll), Some(Roll::Forward));
        assert!((panel.picture() - LENGTH).abs() < 2.0 * FRAME);
        assert!(!panel.forward.playing);
        assert!(
            panel.seeks.is_empty(),
            "вперёд с начала обычный файл не переходит"
        );
    }

    #[test]
    fn the_cursor_leaving_at_three_seconds_plays_reverse_from_four_and_a_half() {
        let mut panel = Panel::new();
        panel.run(3.0, true);
        panel.run(2.0 * FRAME, false);
        let &(roll, time) = panel.seeks.last().expect("переход на новое место");
        assert_eq!(roll, Roll::Backward);
        assert!((time - 4.5).abs() < 0.05, "reverse начинается с {time}");
        panel.run(3.0 + SEEK_FRAMES as f64 * FRAME, false);
        assert_eq!(panel.shown.map(|(roll, _)| roll), Some(Roll::Backward));
        assert!(
            panel.picture() < 0.1,
            "через 3 с первый кадр, а не {}",
            panel.picture()
        );
    }

    #[test]
    fn the_cursor_returning_a_second_after_leaving_continues_forward_from_two() {
        let mut panel = Panel::new();
        panel.run(3.0, true);
        panel.run(1.0, false);
        panel.run(2.0 * FRAME, true);
        let &(roll, time) = panel.seeks.last().expect("переход на новое место");
        assert_eq!(roll, Roll::Forward);
        assert!((time - 2.0).abs() < 0.1, "обычный файл продолжает с {time}");
    }

    #[test]
    fn the_shown_frame_never_jumps_while_a_player_is_moving_to_the_new_place() {
        let mut panel = Panel::new();
        panel.run(3.0, true);
        let before = panel.picture();
        let mut waited = 0;
        loop {
            let step = panel.frame(false);
            if step.seek.is_some() {
                continue;
            }
            if panel.course.current() == Roll::Backward && step.show.is_some() {
                break;
            }
            assert_eq!(
                step.show, None,
                "пока проигрыватель переходит, кадр не пишется"
            );
            assert!((panel.picture() - before).abs() < 4.0 * FRAME);
            waited += 1;
            assert!(waited < 20);
        }
        assert!(waited >= 1);
        assert!((panel.picture() - before).abs() < 8.0 * FRAME);
    }

    #[test]
    fn reaching_the_beginning_stands_on_the_first_frame_and_asks_for_nothing() {
        let mut panel = Panel::new();
        panel.run(2.0, true);
        panel.run(4.0, false);
        let before = panel.seeks.len();
        panel.run(1.0, false);
        assert!(panel.picture() < 0.1);
        assert!(!panel.backward.playing);
        assert_eq!(panel.seeks.len(), before);
    }

    #[test]
    fn the_cursor_coming_back_while_standing_at_the_first_frame_plays_forward_from_the_start() {
        let mut panel = Panel::new();
        panel.run(2.0, true);
        panel.run(4.0, false);
        panel.run(0.5, true);
        let &(roll, time) = panel.seeks.last().expect("переход");
        assert_eq!((roll, time < 0.1), (Roll::Forward, true));
        assert!(panel.forward.playing && panel.picture() > 0.0);
    }

    #[test]
    fn a_cursor_that_changes_its_mind_during_a_move_does_not_start_the_wrong_player() {
        let mut panel = Panel::new();
        panel.run(3.0, true);
        panel.frame(false);
        panel.frame(true);
        panel.run(1.0, true);
        assert_eq!(panel.course.current(), Roll::Forward);
        assert!(panel.forward.playing && !panel.backward.playing);
    }

    #[test]
    fn a_paused_player_the_browser_stopped_is_asked_to_play_again() {
        let mut panel = Panel::new();
        panel.run(1.0, true);
        panel.forward.playing = false;
        let step = panel.frame(true);
        assert_eq!(step.play, Some(Roll::Forward));
    }

    #[test]
    fn after_the_end_a_video_resting_at_two_and_a_half_plays_reverse_only_from_five() {
        let mut panel = Panel::resting_at(2.5);
        panel.run(LENGTH + 1.0, true);
        assert!(
            panel.seeks.is_empty(),
            "пока курсор над местом, ничего не переходит"
        );
        panel.run(2.0 * FRAME, false);
        let &(roll, time) = panel.seeks.last().expect("переход на новое место");
        assert_eq!(roll, Roll::Backward);
        assert!((time - 5.0).abs() < 0.05, "reverse начинается с {time}");
        panel.run(1.0 + SEEK_FRAMES as f64 * FRAME, false);
        assert!(
            (panel.picture() - 1.5).abs() < 0.1,
            "через секунду ролик на 1,5 с, а не на {}",
            panel.picture()
        );
        panel.run(2.0, false);
        assert!(panel.picture() < 0.1);
        assert!(!panel.backward.playing);
        assert_eq!(panel.seeks.len(), 1);
    }

    #[test]
    fn after_the_end_a_video_resting_at_zero_stands_on_the_last_frame_when_the_cursor_leaves() {
        let mut panel = Panel::resting_at(0.0);
        panel.run(LENGTH + 1.0, true);
        panel.run(3.0, false);
        assert!(panel.seeks.is_empty(), "назад ничего не переходит");
        assert!(!panel.forward.playing && !panel.backward.playing);
        assert_eq!(panel.shown.map(|(roll, _)| roll), Some(Roll::Forward));
        assert!((panel.picture() - LENGTH).abs() < 2.0 * FRAME);
    }

    #[test]
    fn the_cursor_returning_to_a_video_resting_at_zero_plays_it_again_from_the_start() {
        let mut panel = Panel::resting_at(0.0);
        panel.run(LENGTH + 1.0, true);
        panel.run(1.0, false);
        panel.run(2.0 * FRAME, true);
        let &(roll, time) = panel.seeks.last().expect("переход на начало");
        assert_eq!((roll, time), (Roll::Forward, 0.0));
        panel.run(1.0 + SEEK_FRAMES as f64 * FRAME, true);
        assert!(panel.forward.playing);
        assert!(
            (panel.picture() - 1.0).abs() < 0.2,
            "жест идёт с начала, а ролик на {}",
            panel.picture()
        );
        panel.run(LENGTH, true);
        panel.run(1.0, false);
        assert_eq!(panel.seeks.len(), 1, "после второго конца снова стоит");
    }

    #[test]
    fn the_cursor_returning_to_a_video_resting_at_two_and_a_half_plays_forward_from_there() {
        let mut panel = Panel::resting_at(2.5);
        panel.run(LENGTH + 1.0, true);
        panel.run(2.0 * FRAME, false);
        panel.run(2.0 * FRAME, true);
        let &(roll, time) = panel.seeks.last().expect("переход на место");
        assert_eq!(roll, Roll::Forward);
        assert!(
            (time - 2.5).abs() < 0.05,
            "обычный файл переходит на {time}"
        );
        panel.run(1.0 + SEEK_FRAMES as f64 * FRAME, true);
        assert!(panel.forward.playing);
        assert!((panel.picture() - 3.5).abs() < 0.2);
    }

    #[test]
    fn the_cursor_returning_while_reverse_plays_continues_forward_from_where_it_got_to() {
        let mut panel = Panel::resting_at(2.5);
        panel.run(LENGTH + 1.0, true);
        panel.run(1.0 + SEEK_FRAMES as f64 * FRAME, false);
        panel.run(2.0 * FRAME, true);
        let &(roll, time) = panel.seeks.last().expect("переход на место");
        assert_eq!(roll, Roll::Forward);
        assert!((time - 1.5).abs() < 0.2, "обычный файл продолжает с {time}");
    }

    #[test]
    fn leaving_before_the_end_ignores_rest_at() {
        let mut panel = Panel::resting_at(2.5);
        panel.run(3.0, true);
        panel.run(2.0 * FRAME, false);
        let &(roll, time) = panel.seeks.last().expect("переход на новое место");
        assert_eq!(roll, Roll::Backward);
        assert!((time - 4.5).abs() < 0.05, "reverse начинается с {time}");
        panel.run(3.0 + SEEK_FRAMES as f64 * FRAME, false);
        assert!(panel.picture() < 0.1);
    }

    #[test]
    fn a_reset_returns_to_the_first_frame_once() {
        let mut panel = Panel::new();
        assert!(!panel.course.reset(), "нетронутому ходу сбрасывать нечего");
        panel.run(1.0, true);
        assert!(panel.course.reset());
        assert!(!panel.course.reset());
        assert_eq!(panel.course.current(), Roll::Forward);
        panel.forward.time = 0.0;
        panel.forward.playing = false;
        panel.run(0.1, false);
        assert_eq!(panel.picture(), 0.0);
    }
}

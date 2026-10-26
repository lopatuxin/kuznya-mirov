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

    /// «Видео на панели», `idle`: покой — курсора нет, а видео стоит на первом кадре или дошло до него назад.
    /// Видео, что после конца считается стоящим на `rest_at` 0, тоже в покое: его последний кадр — поза первого.
    pub fn at_rest(&self, hovering: bool) -> bool {
        !hovering && matches!(self.state, CourseState::Rolling(_)) && self.reached(Roll::Backward)
    }
}

/// Сколько секунд кадр ролика покоя проступает поверх видео по наведению или тает с него.
pub const IDLE_FADE_SECONDS: f64 = 0.25;

/// «Картинки» → «Видео на панели», `idle`: покой видео по наведению. Каждый покой — новый круг ролика покоя с
/// первого кадра, поэтому покои считаются. Вес кадра покоя поверх видео за [`IDLE_FADE_SECONDS`] растёт до
/// единицы, когда в покое ролик покоя отдал кадр, и падает до нуля, когда покоя нет: курсор пришёл посреди
/// вдоха — вдох тает поверх первого кадра шага, а не сменяется им. Покой, что начался, пока прежний кадр покоя
/// ещё не растаял, ждёт: ролик покоя стоит на том кадре, пока вес не дойдёт до нуля, и только потом идёт с
/// первого кадра, — иначе кадр посреди вдоха сменился бы первым на глазах.
#[derive(Debug, Clone, Default)]
pub struct IdleFade {
    /// Покой идёт и ролик покоя играет свой круг.
    playing: bool,
    rests: u64,
    /// В этом покое ролик покоя уже отдал кадр.
    shown: bool,
    weight: f64,
    last_now: Option<f64>,
}

impl IdleFade {
    /// Начало кадра: `resting` — видео по наведению в покое. Возвращает номер покоя — счёт открытий для круга
    /// ролика покоя: новый номер — круг с первого кадра; `None` — ролик покоя стоит.
    pub fn begin(&mut self, resting: bool) -> Option<u64> {
        if !resting {
            self.playing = false;
            self.shown = false;
        } else if !self.playing && self.weight == 0.0 {
            self.playing = true;
            self.rests += 1;
        }
        self.playing.then_some(self.rests)
    }

    /// Конец кадра: `now` — время окна в секундах, `written` — ролик покоя записал кадр в этом кадре. Возвращает
    /// вес кадра покоя.
    pub fn finish(&mut self, now: f64, written: bool) -> f64 {
        self.shown |= self.playing && written;
        let elapsed = self.last_now.map_or(0.0, |last| (now - last).max(0.0));
        self.last_now = Some(now);
        let step = elapsed / IDLE_FADE_SECONDS;
        self.weight = if self.shown {
            (self.weight + step).min(1.0)
        } else {
            (self.weight - step).max(0.0)
        };
        self.weight
    }

    /// Панели нет на экране: покоя нет и кадр покоя не виден; покой на новом экране — новый круг.
    pub fn leave(&mut self) {
        self.playing = false;
        self.shown = false;
        self.weight = 0.0;
        self.last_now = None;
    }
}

/// Насколько близко к концу файла проигрыватель считается дошедшим до последнего кадра: последний кадр ролика
/// в 24 или 30 кадрах в секунду длиннее этого, поэтому к этому мгновению он уже на экране.
const LOOP_LAST_FRAME_SECONDS: f64 = 1.0 / 60.0;
/// Насколько проигрыватель может отстать от времени окна или уйти вперёд, секунд, прежде чем его
/// переведут на то место, где ему быть: вкладка стояла скрытой, браузер притормозил видео.
const LOOP_DRIFT_SECONDS: f64 = 0.5;
/// Проигрыватель, что стоит не дальше этого от начала файла, считается стоящим на первом кадре.
const PARKED_SECONDS: f64 = 1e-3;

/// Номер проигрывателя видео по кругу: 0 — проигрыватель самого видео, 1 — второй проигрыватель того же файла.
pub type Slot = usize;

/// Что сделать с одним проигрывателем видео по кругу в этом кадре: сначала пауза, потом переход на место,
/// потом просьба играть.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerOrder {
    pub pause: bool,
    pub seek: Option<f64>,
    pub play: bool,
}

/// Что сделать с двумя проигрывателями видео по кругу в этом кадре.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LoopStep {
    pub orders: [PlayerOrder; 2],
    /// Чей кадр писать в атлас; `None` — оставить показанный.
    pub show: Option<Slot>,
}

impl LoopStep {
    /// Оба проигрывателя встают на первом кадре; в атлас ничего не пишется.
    fn parked() -> LoopStep {
        let order = PlayerOrder {
            pause: true,
            seek: Some(0.0),
            play: false,
        };
        LoopStep {
            orders: [order; 2],
            show: None,
        }
    }
}

/// «Картинки» → «Видео на панели»: ход видео по кругу — по времени окна с того мгновения, как экран открыт.
/// Экран открылся — первый кадр; за последним сразу первый: файл играют два проигрывателя по очереди. Идёт
/// один, второй стоит на первом кадре и ждёт; когда идущий дошёл до последнего кадра, второй начинает
/// играть, а прежний возвращается на начало и встаёт. Перехода на стыке нет, поэтому ни один кадр не
/// пропадает и ни один не замирает. Ход лишь следит за идущим проигрывателем по часам окна: после каждого
/// перехода принимает за истину то место, куда тот пришёл, а ушёл он от часов больше чем на
/// [`LOOP_DRIFT_SECONDS`] (вкладка стояла скрытой) — переводит его на место. Экран открыли заново (счёт
/// `opened` сдвинулся) — оба проигрывателя возвращаются на начало и круг начинается сначала.
#[derive(Debug, Clone)]
pub struct LoopCourse {
    /// Длина ролика; 0 — круга нет, проигрыватель просто играет.
    length: f64,
    /// Счёт открытий экрана, под которым ход ведёт видео; `None`, пока панели на экране нет.
    opened: Option<u64>,
    /// Какой проигрыватель играет.
    active: Slot,
    /// Пара «время окна — место идущего проигрывателя», по которой считается, где ему быть сейчас; `None`, пока
    /// проигрыватель не отдал первый кадр. На стыке пара переходит к плановому мгновению стыка, а не к тому, когда
    /// второй проигрыватель стал играть: так запуск не уводит круг от времени окна с открытия.
    anchor: Option<(f64, f64)>,
    last_now: Option<f64>,
    /// Сколько кадров подряд время окна не сдвигалось.
    repeats: u32,
    /// Был переход: опорная пара сверяется с местом, куда проигрыватель пришёл.
    resync: bool,
}

/// Кадр рисуют дважды, когда `tick` и `draw` зовут один за другим (партия в редакторе), и время окна у них
/// одно: часы стоят, только когда оно не сдвигается и третий кадр подряд.
const LOOP_CLOCK_REPEATS: u32 = 2;

impl LoopCourse {
    /// Ролик длиной `length` секунд; ролик нулевой длины или без длины не получает ни стыка, ни перехода.
    pub fn new(length: f64) -> Self {
        LoopCourse {
            length: if length.is_finite() {
                length.max(0.0)
            } else {
                0.0
            },
            opened: None,
            active: 0,
            anchor: None,
            last_now: None,
            repeats: 0,
            resync: false,
        }
    }

    /// Кадр с панелью на экране: `now` — время окна в секундах, `opened` — номер открытия экрана, `ready` —
    /// какие из двух проигрывателей отдают кадр, `readings` — их ответы. Круг начинается, когда кадр есть у
    /// обоих; потом нужен кадр только у идущего, а ждущий после возврата на начало может быть без кадра.
    pub fn step(
        &mut self,
        now: f64,
        opened: u64,
        ready: [bool; 2],
        readings: [PlayerReading; 2],
    ) -> LoopStep {
        if self.opened.is_some_and(|known| known != opened) {
            self.forget();
            self.opened = Some(opened);
            return LoopStep::parked();
        }
        self.opened = Some(opened);
        let running = self.clock_runs(now);
        let live = self.active;
        let idle = 1 - live;
        let reading = readings[live];
        let mut step = LoopStep::default();
        step.orders[idle] = park_order(readings[idle]);
        let waiting = if self.anchor.is_some() {
            !ready[live]
        } else {
            !(ready[0] && ready[1])
        };
        if !running {
            step.orders[live].pause = !reading.paused;
            step.show = (!reading.seeking).then_some(live);
        } else if waiting || reading.seeking {
            step.orders[live].play = reading.paused;
        } else {
            self.follow(now, readings, ready, &mut step);
        }
        step
    }

    /// Кадр без панели на экране: круг начнётся заново. `Some` — ход успел сдвинуться и проигрыватели надо
    /// вернуть на начало; повторный вызов подряд ничего не меняет.
    pub fn leave(&mut self) -> Option<LoopStep> {
        let touched = self.opened.is_some();
        self.forget();
        touched.then(LoopStep::parked)
    }

    fn forget(&mut self) {
        self.opened = None;
        self.anchor = None;
        self.last_now = None;
        self.repeats = 0;
        self.resync = false;
    }

    fn clock_runs(&mut self, now: f64) -> bool {
        if self.last_now == Some(now) {
            self.repeats = self.repeats.saturating_add(1);
        } else {
            self.repeats = 0;
            self.last_now = Some(now);
        }
        self.repeats < LOOP_CLOCK_REPEATS
    }

    /// Идущий проигрыватель отдаёт кадр и не переходит: показать его, вернуть на место по часам окна, а дойдя
    /// до последнего кадра, передать ход второму.
    fn follow(
        &mut self,
        now: f64,
        readings: [PlayerReading; 2],
        ready: [bool; 2],
        step: &mut LoopStep,
    ) {
        let live = self.active;
        let reading = readings[live];
        if self.resync {
            self.resync = false;
            self.anchor = Some((now, reading.time));
        }
        let (anchor_now, anchor_time) = *self.anchor.get_or_insert((now, reading.time));
        step.show = Some(live);
        if self.length <= 0.0 {
            step.orders[live].play = reading.paused;
            return;
        }
        let phase = (anchor_time + (now - anchor_now)).rem_euclid(self.length);
        if drift(reading.time, phase, self.length) > LOOP_DRIFT_SECONDS {
            step.orders[live].seek = Some(phase);
            step.show = None;
            self.resync = true;
        } else if reading.time >= self.length - LOOP_LAST_FRAME_SECONDS {
            let idle = 1 - live;
            if is_parked(readings[idle]) && ready[idle] {
                self.active = idle;
                self.anchor = Some((anchor_now + (self.length - anchor_time), 0.0));
                step.orders[idle].play = true;
            }
        } else {
            step.orders[live].play = reading.paused;
        }
    }
}

/// Проигрыватель стоит на первом кадре и ждёт.
fn is_parked(reading: PlayerReading) -> bool {
    !reading.seeking && reading.paused && reading.time <= PARKED_SECONDS
}

/// Ждущему проигрывателю — вернуться на первый кадр и встать, если он ушёл с него.
fn park_order(reading: PlayerReading) -> PlayerOrder {
    if reading.seeking || is_parked(reading) {
        return PlayerOrder::default();
    }
    PlayerOrder {
        pause: !reading.paused,
        seek: Some(0.0),
        play: false,
    }
}

/// На сколько проигрыватель на месте `time` отстал от места `phase` или ушёл вперёд по кругу длиной `length`.
fn drift(time: f64, phase: f64, length: f64) -> f64 {
    let off = (time - phase).rem_euclid(length);
    if off > length / 2.0 {
        length - off
    } else {
        off
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

#[cfg(test)]
mod loop_course_tests {
    use super::*;

    const FRAME: f64 = 1.0 / 60.0;
    /// Кадров в секунду в ролике.
    const FILE_FPS: f64 = 24.0;
    /// Сколько кадров браузер переходит на новое место.
    const SEEK_FRAMES: u32 = 3;
    /// Сколько кадров проходит от просьбы играть до первого сдвига ролика.
    const PLAY_FRAMES: u32 = 1;
    /// Сколько кадров отрисовки дольше всего может стоять один кадр ролика: его длина, просьба играть и запас.
    const LONGEST_HOLD: usize = 5;

    struct Player {
        length: f64,
        time: f64,
        playing: bool,
        ended: bool,
        warmup: u32,
        seek: Option<(u32, f64)>,
    }

    impl Player {
        fn new(length: f64) -> Player {
            Player {
                length,
                time: 0.0,
                playing: false,
                ended: false,
                warmup: 0,
                seek: None,
            }
        }

        fn reading(&self) -> PlayerReading {
            PlayerReading {
                time: self.time,
                seeking: self.seek.is_some(),
                paused: !self.playing,
            }
        }

        /// Номер кадра ролика на экране проигрывателя.
        fn picture(&self) -> u32 {
            let last = (self.length * FILE_FPS).round() as u32 - 1;
            ((self.time * FILE_FPS + 1e-6).floor() as u32).min(last)
        }

        fn advance(&mut self) {
            if let Some((left, to)) = self.seek {
                if left > 1 {
                    self.seek = Some((left - 1, to));
                } else {
                    self.seek = None;
                    self.time = to;
                }
            } else if self.playing && self.warmup > 0 {
                self.warmup -= 1;
            } else if self.playing {
                self.time += FRAME;
                if self.time >= self.length - 1e-9 {
                    self.time = self.length;
                    self.playing = false;
                    self.ended = true;
                }
            }
        }
    }

    /// Два проигрывателя и ход видео по кругу, как их ведёт слой отрисовки.
    struct Browser {
        course: LoopCourse,
        players: [Player; 2],
        now: f64,
        opened: u64,
        ready: bool,
        /// Кадр ролика на экране в каждом кадре отрисовки; `None` — картинка не писалась.
        shown: Vec<Option<u32>>,
        /// Переходы: проигрыватель и место.
        seeks: Vec<(Slot, f64)>,
        /// Сколько раз проигрыватель отправили на место, пока второй не играл.
        lone_seeks: usize,
    }

    impl Browser {
        fn new(length: f64) -> Browser {
            Browser {
                course: LoopCourse::new(length),
                players: [Player::new(length), Player::new(length)],
                now: 100.0,
                opened: 1,
                ready: true,
                shown: Vec::new(),
                seeks: Vec::new(),
                lone_seeks: 0,
            }
        }

        /// Какие проигрыватели отдают кадр: как в Chromium, проигрыватель, что переходит на новое место, кадра не
        /// отдаёт.
        fn ready(&self) -> [bool; 2] {
            self.players
                .each_ref()
                .map(|player| self.ready && player.seek.is_none())
        }

        fn order(&mut self, slot: Slot, order: PlayerOrder) {
            let player = &mut self.players[slot];
            if order.pause {
                player.playing = false;
            }
            if let Some(to) = order.seek {
                self.seeks.push((slot, to));
                player.seek = Some((SEEK_FRAMES, to));
                player.ended = false;
            }
            if order.play && self.ready && !player.playing {
                if player.ended {
                    player.ended = false;
                    player.time = 0.0;
                }
                player.playing = true;
                player.warmup = PLAY_FRAMES;
            }
        }

        fn apply(&mut self, step: LoopStep) {
            for slot in 0..2 {
                self.order(slot, step.orders[slot]);
            }
            for slot in 0..2 {
                if step.orders[slot].seek.is_some() && !self.players[1 - slot].playing {
                    self.lone_seeks += 1;
                }
            }
            let ready = self.ready();
            self.shown.push(
                step.show
                    .filter(|&slot| ready[slot])
                    .map(|slot| self.players[slot].picture()),
            );
        }

        /// Проходит кадр отрисовки: если время окна идёт, оно и проигрыватели сдвигаются на кадр.
        fn pass(&mut self, advance_clock: bool) {
            if !advance_clock {
                return;
            }
            for player in &mut self.players {
                player.advance();
            }
            self.now += FRAME;
        }

        /// Один кадр отрисовки при текущем времени окна; `advance_clock` — после кадра оно сдвигается.
        fn step(&mut self, advance_clock: bool) -> LoopStep {
            let readings = [self.players[0].reading(), self.players[1].reading()];
            let step = self
                .course
                .step(self.now, self.opened, self.ready(), readings);
            self.apply(step);
            self.pass(advance_clock);
            step
        }

        fn run(&mut self, seconds: f64) {
            for _ in 0..(seconds / FRAME).round() as u32 {
                self.step(true);
            }
        }

        /// Кадры ролика, что были на экране, по порядку, подряд идущие одинаковые склеены.
        fn pictures(&self) -> Vec<u32> {
            let mut pictures: Vec<u32> = self.shown.iter().flatten().copied().collect();
            pictures.dedup();
            pictures
        }

        /// Самая долгая череда кадров отрисовки, на которых картинка не менялась.
        fn longest_hold(&self) -> usize {
            let mut longest = 0;
            let mut hold = 0;
            let mut last = None;
            for shown in &self.shown {
                if shown.is_some() && *shown != last {
                    hold = 0;
                    last = *shown;
                } else {
                    hold += 1;
                    longest = longest.max(hold);
                }
            }
            longest
        }
    }

    #[test]
    fn the_screen_opening_shows_the_first_frame_and_the_video_goes_on_by_the_window_clock() {
        let mut browser = Browser::new(3.0);
        browser.step(true);
        assert_eq!(browser.shown[0], Some(0), "открылся — первый кадр");
        browser.run(1.0);
        let at = browser.players[0].time;
        assert!((at - 1.0).abs() < 0.1, "через секунду на секунде: {at}");
        assert!(browser.players[0].playing && !browser.players[1].playing);
    }

    #[test]
    fn every_frame_of_the_file_is_shown_once_per_loop_and_none_is_missing_on_the_seam() {
        let mut browser = Browser::new(2.5);
        browser.run(12.5);
        let pictures = browser.pictures();
        assert_eq!(pictures[0], 0);
        for pair in pictures.windows(2) {
            assert!(
                pair[1] == pair[0] + 1 || (pair[0] == 59 && pair[1] == 0),
                "кадры идут по порядку, ни один не пропал: {pair:?}"
            );
        }
        let loops = pictures.iter().filter(|&&picture| picture == 0).count();
        assert!(
            (4..=5).contains(&loops),
            "за 12,5 секунды 4–5 кругов по 2,5 с: {loops}"
        );
        assert!(
            pictures.contains(&59),
            "последний кадр ролика показан: {pictures:?}"
        );
    }

    #[test]
    fn nothing_stops_and_nothing_is_skipped_on_the_seam_of_a_two_second_loop() {
        let mut browser = Browser::new(2.4);
        browser.run(20.0);
        assert!(
            browser.shown.iter().all(Option::is_some),
            "ни одного кадра отрисовки без картинки, в том числе на стыке"
        );
        assert!(
            browser.longest_hold() <= LONGEST_HOLD,
            "ни один кадр не стоит дольше своей длины с запасом: {} кадров отрисовки",
            browser.longest_hold()
        );
        assert!(
            browser
                .players
                .iter()
                .all(|player| !player.ended || !player.playing),
            "проигрыватель, дошедший до конца, стоит"
        );
    }

    #[test]
    fn the_seam_only_returns_the_player_that_has_played_to_the_start_while_the_other_one_plays() {
        let mut browser = Browser::new(2.5);
        browser.run(20.0);
        assert!(!browser.seeks.is_empty());
        assert!(
            browser.seeks.iter().all(|(_, to)| *to == 0.0),
            "переходов на другое место нет: {:?}",
            browser.seeks
        );
        assert_eq!(
            browser.lone_seeks, 0,
            "идущий проигрыватель никуда не переводится: {:?}",
            browser.seeks
        );
        assert!(
            browser.seeks.windows(2).all(|pair| pair[0].0 != pair[1].0),
            "проигрыватели возвращаются на начало по очереди: {:?}",
            browser.seeks
        );
    }

    #[test]
    fn the_circle_stays_tied_to_the_window_time_since_the_opening_round_after_round() {
        let mut browser = Browser::new(2.5);
        for minute in 1..=5 {
            browser.run(60.0);
            let live = browser
                .players
                .iter()
                .find(|player| player.playing)
                .expect("идёт");
            let phase = (browser.now - 100.0).rem_euclid(2.5);
            let off = (live.time - phase + 1.25).rem_euclid(2.5) - 1.25;
            assert!(
                off.abs() <= 2.0 * LOOP_DRIFT_SECONDS,
                "на {minute}-й минуте круг ушёл от времени окна на {off} с"
            );
        }
        let corrections = browser.seeks.iter().filter(|(_, to)| *to > 0.0).count();
        assert!(corrections <= 5, "переводов по часам окна: {corrections}");
    }

    #[test]
    fn a_video_shorter_than_a_second_goes_round_the_same_way() {
        let mut browser = Browser::new(0.5);
        browser.run(5.0);
        let pictures = browser.pictures();
        let loops = pictures.iter().filter(|&&picture| picture == 0).count();
        assert!((9..=10).contains(&loops), "{loops}");
        for pair in pictures.windows(2) {
            assert!(
                pair[1] == pair[0] + 1 || (pair[0] == 11 && pair[1] == 0),
                "{pair:?}"
            );
        }
        assert!(browser.shown.iter().all(Option::is_some));
    }

    #[test]
    fn a_video_without_a_length_is_left_to_play_and_never_moved() {
        for length in [0.0, f64::NAN, f64::INFINITY] {
            let mut browser = Browser::new(10.0);
            browser.course = LoopCourse::new(length);
            browser.run(2.0);
            assert!(browser.seeks.is_empty(), "{length}");
            assert!(browser.shown.iter().all(Option::is_some));
            assert!(!browser.players[1].playing, "второй проигрыватель не нужен");
        }
    }

    #[test]
    fn the_circle_waits_for_the_first_frame_and_counts_from_when_the_players_have_one() {
        let mut browser = Browser::new(3.0);
        browser.ready = false;
        browser.run(2.0);
        assert!(
            browser.shown.iter().all(Option::is_none),
            "кадра нет — ничего не пишется"
        );
        assert!(browser.seeks.is_empty());
        browser.ready = true;
        browser.run(1.0);
        assert!(
            browser.seeks.is_empty(),
            "круг начался с готовности, а не с открытия: {:?}",
            browser.seeks
        );
        assert!(
            browser
                .pictures()
                .last()
                .is_some_and(|&picture| picture <= 26)
        );
    }

    #[test]
    fn a_window_clock_that_has_stopped_pauses_the_video_and_a_clock_that_goes_on_moves_it_to_its_place()
     {
        let mut browser = Browser::new(3.0);
        browser.run(1.0);
        for _ in 0..10 {
            browser.step(false);
        }
        assert!(!browser.players[0].playing, "часы окна встали — пауза");
        browser.now += 5.4;
        let resumed = browser.now;
        browser.run(0.5);
        assert!(browser.players[0].playing, "часы пошли — играет");
        let moved: Vec<f64> = browser
            .seeks
            .iter()
            .filter(|(_, to)| *to > 0.0)
            .map(|(_, to)| *to)
            .collect();
        assert_eq!(moved.len(), 1, "и перешёл туда, где ему быть по часам окна");
        let expected = (resumed - 100.0).rem_euclid(3.0);
        assert!(
            (moved[0] - expected).abs() < 0.2,
            "{moved:?} против {expected}"
        );
    }

    #[test]
    fn two_renders_of_one_frame_with_one_window_time_do_not_stop_the_video() {
        let mut browser = Browser::new(3.0);
        let mut standing = 0;
        for _ in 0..240 {
            let first = browser.step(false);
            assert!(first.show.is_some());
            let again = browser.step(true);
            assert!(again.show.is_some());
            if browser.players.iter().any(|player| player.playing) {
                standing = 0;
            } else {
                standing += 1;
                assert!(standing < 2, "видео встало");
            }
        }
        assert!(
            browser.seeks.iter().all(|(_, to)| *to == 0.0),
            "только возвраты ждущего на начало: {:?}",
            browser.seeks
        );
    }

    #[test]
    fn a_player_the_browser_held_back_is_moved_to_the_place_the_window_clock_names() {
        let mut browser = Browser::new(3.0);
        browser.run(1.0);
        browser.players[0].playing = false;
        browser.now += 21.84;
        browser.step(true);
        let moved: Vec<f64> = browser
            .seeks
            .iter()
            .filter(|(_, to)| *to > 0.0)
            .map(|(_, to)| *to)
            .collect();
        assert_eq!(moved.len(), 1);
        let expected = (1.0_f64 + 21.84).rem_euclid(3.0);
        assert!(
            (moved[0] - expected).abs() < 0.1,
            "{moved:?} против {expected}"
        );
    }

    #[test]
    fn a_screen_opened_again_sends_both_players_back_and_starts_from_the_first_frame() {
        let mut browser = Browser::new(2.5);
        browser.run(1.5);
        assert!(browser.players[0].time > 1.0);
        browser.opened += 1;
        let step = browser.step(true);
        assert_eq!(step.show, None, "кадр этого мгновения не пишется");
        assert!(
            step.orders
                .iter()
                .all(|order| order.seek == Some(0.0) && !order.play)
        );
        browser.shown.clear();
        browser.run(0.5);
        assert_eq!(
            browser.pictures().first(),
            Some(&0),
            "экран открылся заново — первый кадр"
        );
        assert!(browser.pictures().iter().all(|&picture| picture < 15));
    }

    #[test]
    fn a_screen_opened_again_while_the_second_player_plays_starts_from_the_first_frame_too() {
        let mut browser = Browser::new(2.5);
        browser.run(3.0);
        assert!(
            browser.players[1].playing,
            "после первого стыка идёт второй"
        );
        browser.opened += 3;
        browser.step(true);
        browser.shown.clear();
        browser.run(0.5);
        assert_eq!(browser.pictures().first(), Some(&0));
        assert!(browser.pictures().iter().all(|&picture| picture < 15));
    }

    #[test]
    fn a_panel_that_left_the_screen_sends_the_players_back_once_and_the_circle_starts_again() {
        let mut browser = Browser::new(3.0);
        assert!(
            browser.course.leave().is_none(),
            "нетронутому ходу сбрасывать нечего"
        );
        browser.run(1.5);
        let parked = browser.course.leave().expect("ход сдвинулся");
        assert!(parked.orders.iter().all(|order| order.seek == Some(0.0)));
        assert!(browser.course.leave().is_none());
        browser.apply(parked);
        for _ in 0..6 {
            browser.pass(true);
        }
        browser.now += 50.0;
        browser.shown.clear();
        browser.step(true);
        assert_eq!(
            browser.shown[0],
            Some(0),
            "экран открылся заново — первый кадр"
        );
        assert!(browser.seeks.iter().all(|(_, to)| *to == 0.0));
    }
}

#[cfg(test)]
mod idle_tests {
    use super::*;

    const FRAME: f64 = 1.0 / 60.0;

    fn reading(time: f64, paused: bool) -> PlayerReading {
        PlayerReading {
            time,
            seeking: false,
            paused,
        }
    }

    #[test]
    fn a_new_course_without_the_cursor_is_at_rest() {
        let course = PanelCourse::new(7.5);
        assert!(course.at_rest(false));
        assert!(!course.at_rest(true));
    }

    #[test]
    fn a_course_on_its_way_back_is_not_at_rest_until_it_reaches_the_first_frame() {
        let mut course = PanelCourse::new(7.5);
        course.step(true, None, reading(0.0, true));
        course.step(true, None, reading(3.0, false));
        course.step(false, None, reading(3.0, false));
        course.step(false, None, reading(4.5, true));
        course.step(false, None, reading(5.0, false));
        assert!(!course.at_rest(false), "назад ещё 2,5 с");
        course.step(false, None, reading(7.5, true));
        assert!(course.at_rest(false));
    }

    #[test]
    fn a_course_resting_at_zero_after_the_end_is_at_rest_when_the_cursor_leaves() {
        let mut course = PanelCourse::new(5.0);
        course.step(true, Some(0.0), reading(0.0, true));
        course.step(true, Some(0.0), reading(5.0, true));
        assert!(!course.at_rest(true));
        course.step(false, Some(0.0), reading(5.0, true));
        assert!(course.at_rest(false));
    }

    /// Кадры по 1/60 с от `from`: покой `resting`, ролик покоя пишет кадр `written`; вес после последнего.
    fn run(fade: &mut IdleFade, from: f64, seconds: f64, resting: bool, written: bool) -> f64 {
        let mut weight = 0.0;
        for k in 0..(seconds / FRAME).round() as u32 {
            fade.begin(resting);
            weight = fade.finish(from + f64::from(k) * FRAME, written);
        }
        weight
    }

    #[test]
    fn each_rest_gets_a_new_number_and_no_rest_gets_none() {
        let mut fade = IdleFade::default();
        let first = fade.begin(true).expect("покой");
        assert_eq!(fade.begin(true), Some(first), "тот же покой — тот же круг");
        assert_eq!(fade.begin(false), None);
        let second = fade.begin(true).expect("покой");
        assert_ne!(first, second, "новый покой — круг с первого кадра");
    }

    #[test]
    fn the_idle_frame_shows_through_in_a_quarter_second_once_it_has_a_frame() {
        let mut fade = IdleFade::default();
        assert_eq!(
            run(&mut fade, 0.0, 0.5, true, false),
            0.0,
            "кадра покоя ещё нет"
        );
        let halfway = run(&mut fade, 0.5, IDLE_FADE_SECONDS / 2.0, true, true);
        assert!((0.4..0.6).contains(&halfway), "на полпути вес {halfway}");
        assert_eq!(run(&mut fade, 0.7, 0.2, true, false), 1.0);
    }

    #[test]
    fn the_cursor_arriving_melts_the_idle_frame_away_in_a_quarter_second() {
        let mut fade = IdleFade::default();
        run(&mut fade, 0.0, 1.0, true, true);
        let start = 1.0;
        let mut weights = Vec::new();
        for k in 0..30 {
            fade.begin(false);
            weights.push(fade.finish(start + f64::from(k) * FRAME, true));
        }
        assert!(
            weights[0] > 0.9,
            "в первый кадр наведения кадр покоя ещё виден"
        );
        assert!(weights.windows(2).all(|pair| pair[1] <= pair[0]));
        let gone = weights.iter().position(|&w| w == 0.0).expect("растаял");
        let seconds = gone as f64 * FRAME;
        assert!(
            (seconds - IDLE_FADE_SECONDS).abs() < 2.0 * FRAME,
            "растаял за {seconds} с"
        );
    }

    #[test]
    fn a_rest_that_starts_before_the_idle_frame_melted_waits_until_it_is_gone() {
        let mut fade = IdleFade::default();
        let first = run(&mut fade, 0.0, 1.0, true, true);
        assert_eq!(first, 1.0);
        let rest = fade.begin(true);
        run(&mut fade, 1.0, 3.0 * FRAME, false, false);
        assert_eq!(
            fade.begin(true),
            None,
            "прежний кадр покоя ещё тает — ролик стоит"
        );
        let melting = fade.finish(1.0 + 4.0 * FRAME, true);
        assert!(
            melting > 0.0 && melting < 1.0,
            "кадр нового круга не принят: {melting}"
        );
        run(&mut fade, 1.0 + 5.0 * FRAME, IDLE_FADE_SECONDS, true, false);
        let next = fade.begin(true);
        assert!(
            next.is_some() && next != rest,
            "дотаял — новый круг с первого кадра"
        );
    }

    #[test]
    fn leaving_the_screen_hides_the_idle_frame_at_once() {
        let mut fade = IdleFade::default();
        run(&mut fade, 0.0, 1.0, true, true);
        fade.leave();
        fade.begin(true);
        assert_eq!(fade.finish(5.0, false), 0.0);
    }
}

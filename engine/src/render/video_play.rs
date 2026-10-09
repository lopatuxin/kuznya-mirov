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

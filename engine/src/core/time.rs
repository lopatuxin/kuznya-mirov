pub fn seconds_to_steps(seconds: f64) -> i64 {
    let steps = (seconds * 60.0).round() as i64;
    steps.max(1)
}

/// Same conversion for a delta (`["add", "<time>", число]`) rather than a duration: the
/// minimum-of-one clamp applies to durations only, so a delta keeps its sign and can be zero.
pub fn seconds_to_steps_delta(seconds: f64) -> i64 {
    (seconds * 60.0).round() as i64
}

/// «Картинки» → «Кадры»: which frame of a looping strip shows once `elapsed` steps have passed,
/// `frame_len` steps per frame — a world image counts `elapsed` in steps taken (frozen while the
/// world doesn't step), an interface image in window time already converted to the same unit
/// (60 steps/second), never frozen. Loops forever; a single-frame image always answers 0 without
/// doing the division.
pub fn frame_index(elapsed_steps: f64, frame_len_steps: i64, frame_count: u32) -> u32 {
    if frame_count <= 1 {
        return 0;
    }
    let frame_len = frame_len_steps.max(1) as f64;
    let advanced = (elapsed_steps / frame_len).floor().max(0.0);
    (advanced as u64 % frame_count as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_nearest_step() {
        assert_eq!(seconds_to_steps(0.12), 7);
        assert_eq!(seconds_to_steps(0.6), 36);
        assert_eq!(seconds_to_steps(1.0), 60);
    }

    #[test]
    fn never_goes_below_one_step() {
        assert_eq!(seconds_to_steps(0.005), 1);
        assert_eq!(seconds_to_steps(0.0), 1);
    }

    #[test]
    fn rounds_half_up() {
        // 0.5/60 = 1/120s -> 0.5 steps, rounds to nearest even/away? f64::round rounds half away from zero.
        assert_eq!(seconds_to_steps(1.0 / 120.0), 1);
    }

    #[test]
    fn delta_keeps_sign_and_can_be_zero() {
        assert_eq!(seconds_to_steps_delta(0.12), 7);
        assert_eq!(seconds_to_steps_delta(0.0), 0);
        assert_eq!(seconds_to_steps_delta(-0.5), -30);
    }

    #[test]
    fn frame_index_starts_at_zero_and_advances_by_frame_len() {
        assert_eq!(frame_index(0.0, 9, 4), 0);
        assert_eq!(frame_index(9.0, 9, 4), 1);
        assert_eq!(frame_index(17.9, 9, 4), 1);
    }

    #[test]
    fn frame_index_loops_after_the_last_frame() {
        assert_eq!(frame_index(9.0 * 4.0, 9, 4), 0);
        assert_eq!(frame_index(9.0 * 4.0 + 3.0, 9, 4), 0);
    }

    #[test]
    fn single_frame_image_always_answers_zero() {
        assert_eq!(frame_index(0.0, 1, 1), 0);
        assert_eq!(frame_index(500.0, 1, 1), 0);
    }

    #[test]
    fn frame_len_below_one_step_is_clamped_to_one_step_per_frame() {
        assert_eq!(frame_index(0.0, 0, 4), 0);
        assert_eq!(frame_index(1.0, 0, 4), 1);
    }

    #[test]
    fn world_frame_freezes_while_elapsed_does_not_change() {
        let a = frame_index(30.0, 9, 4);
        let b = frame_index(30.0, 9, 4);
        assert_eq!(a, b);
    }
}

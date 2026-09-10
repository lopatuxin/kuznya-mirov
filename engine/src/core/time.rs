pub fn seconds_to_steps(seconds: f64) -> i64 {
    let steps = (seconds * 60.0).round() as i64;
    steps.max(1)
}

/// Same conversion for a delta (`["add", "<time>", число]`) rather than a duration: the
/// minimum-of-one clamp applies to durations only, so a delta keeps its sign and can be zero.
pub fn seconds_to_steps_delta(seconds: f64) -> i64 {
    (seconds * 60.0).round() as i64
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
}

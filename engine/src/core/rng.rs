/// Deterministic pseudo-random source (splitmix64) seeded from `game.json`'s `random_seed`,
/// so a run replays identically for the same seed. `std`'s `rand` is not a dependency of this
/// crate: the engine only needs one well-mixed 64-bit stream, and splitmix64 is a few lines.
#[derive(Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`. `n == 0` returns 0.
    pub fn next_below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn next_below_stays_in_range() {
        let mut rng = Rng::new(1);
        for _ in 0..100 {
            assert!(rng.next_below(7) < 7);
        }
    }
}

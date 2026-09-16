#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let zone = u32::MAX - (u32::MAX % n);
        loop {
            let v = (self.next_u64() >> 32) as u32;
            if v < zone {
                return v % n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn zero_seed_is_not_stuck() {
        let mut a = Rng::from_seed(0);
        let first = a.next_u64();
        assert_ne!(first, 0);
        assert_ne!(first, a.next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut r = Rng::from_seed(7);
        let mut seen = [false; 6];
        for _ in 0..10_000 {
            let v = r.below(6);
            assert!(v < 6);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn below_zero_does_not_panic() {
        assert_eq!(Rng::from_seed(1).below(0), 0);
    }
}

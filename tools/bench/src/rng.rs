//! Deterministic random numbers: the same seed gives the same dataset and the same sequence of
//! benchmark queries on every machine.

/// SplitMix64: tiny, fast and good enough for synthetic data.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(mix(seed))
    }

    /// A generator for one item of a named stream. Items get the same numbers whatever order or
    /// thread they are generated in, so the dataset does not depend on how the work is split.
    pub fn for_item(seed: u64, stream: &str, item: u64) -> Self {
        let mut state = mix(seed);
        for byte in stream.bytes() {
            state = mix(state ^ u64::from(byte));
        }
        Self(mix(state ^ mix(item)))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// A number in `0..n`; 0 when `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }

    /// An index into a collection of `len` items.
    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: i64, high: i64) -> i64 {
        low + self.below(high.abs_diff(low) + 1) as i64
    }

    /// A float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Picks an index with probability proportional to its weight.
#[derive(Debug, Clone)]
pub struct Weighted {
    cumulative: Vec<f64>,
}

impl Weighted {
    pub fn new(weights: impl IntoIterator<Item = f64>) -> Self {
        let mut total = 0.0;
        let cumulative = weights
            .into_iter()
            .map(|weight| {
                total += weight.max(0.0);
                total
            })
            .collect();
        Self { cumulative }
    }

    /// Zipf's law over `n` items: item `k` (from 0) has weight `1 / (k + 1)^exponent`.
    pub fn zipf(n: usize, exponent: f64) -> Self {
        Self::new((1..=n).map(|rank| (rank as f64).powf(-exponent)))
    }

    pub fn len(&self) -> usize {
        self.cumulative.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cumulative.is_empty()
    }

    /// The share of item `index` in the total weight.
    pub fn share(&self, index: usize) -> f64 {
        let total = self.cumulative.last().copied().unwrap_or(0.0);
        let before = index
            .checked_sub(1)
            .and_then(|i| self.cumulative.get(i))
            .copied()
            .unwrap_or(0.0);
        let upto = self.cumulative.get(index).copied().unwrap_or(before);
        if total > 0.0 {
            (upto - before) / total
        } else {
            0.0
        }
    }

    pub fn sample(&self, rng: &mut Rng) -> usize {
        let total = self.cumulative.last().copied().unwrap_or(0.0);
        let point = rng.unit() * total;
        self.cumulative
            .partition_point(|&upto| upto <= point)
            .min(self.cumulative.len().saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_numbers() {
        let a: Vec<u64> = {
            let mut rng = Rng::for_item(7, "issues", 42);
            (0..5).map(|_| rng.next_u64()).collect()
        };
        let mut rng = Rng::for_item(7, "issues", 42);
        let b: Vec<u64> = (0..5).map(|_| rng.next_u64()).collect();
        assert_eq!(a, b);
        assert_ne!(
            Rng::for_item(7, "issues", 42).next_u64(),
            Rng::for_item(7, "history", 42).next_u64()
        );
    }

    #[test]
    fn ranges_hold() {
        let mut rng = Rng::new(1);
        for _ in 0..10_000 {
            assert!(rng.below(10) < 10);
            let x = rng.between(-3, 3);
            assert!((-3..=3).contains(&x));
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
        }
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn weighted_follows_weights() {
        let weighted = Weighted::new([1.0, 0.0, 3.0]);
        let mut rng = Rng::new(3);
        let mut counts = [0_u32; 3];
        for _ in 0..40_000 {
            counts[weighted.sample(&mut rng)] += 1;
        }
        assert_eq!(counts[1], 0);
        let ratio = f64::from(counts[2]) / f64::from(counts[0]);
        assert!((2.7..3.3).contains(&ratio), "{ratio}");
        assert!((weighted.share(2) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn zipf_is_skewed() {
        let zipf = Weighted::zipf(1000, 1.0);
        assert!(zipf.share(0) > 10.0 * zipf.share(99));
    }
}

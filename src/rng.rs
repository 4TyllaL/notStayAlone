/// xorshift64 — suficiente para decidir o que o mascote faz e diz.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    /// Número em `lo..hi` (hi exclusivo).
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        lo + (x % (hi - lo).max(1) as u64) as u32
    }
}

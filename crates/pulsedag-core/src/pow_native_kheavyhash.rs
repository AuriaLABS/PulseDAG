//! Native KHeavyHash; same work digest and full-rank retry semantics as
//! pinned Kaspa, without compiling its transitive workflow dependencies.
//! Do not impose the test-only 32-retry cap on production PoW.
use sha3::{
    digest::{ExtendableOutput, Update, XofReader},
    CShake256, CShake256Core, Digest, Keccak256,
};

struct Xoshiro256PlusPlus([u64; 4]);

impl Xoshiro256PlusPlus {
    fn from_seed(bytes: [u8; 32]) -> Self {
        Self(std::array::from_fn(|i| {
            u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap())
        }))
    }

    fn next(&mut self) -> u64 {
        let [a, b, c, d] = self.0;
        let word = a.wrapping_add(d).rotate_left(23).wrapping_add(a);
        let shifted = b << 17;
        self.0 = [
            a ^ d ^ b,
            b ^ c ^ a,
            c ^ a ^ shifted,
            (d ^ b).rotate_left(45),
        ];
        word
    }
}

fn matrix_rank(values: &[[u8; 64]; 64]) -> usize {
    const EPS: f64 = 1e-9;
    let mut floating = values.map(|row| row.map(f64::from));
    let mut selected = [false; 64];
    let mut rank = 0;
    for column in 0..64 {
        let pivot = (0..64).find(|&row| !selected[row] && floating[row][column].abs() > EPS);
        if let Some(pivot) = pivot {
            selected[pivot] = true;
            rank += 1;
            let divisor = floating[pivot][column];
            for cell in floating[pivot].iter_mut().skip(column + 1) {
                *cell /= divisor;
            }
            let pivot_row = floating[pivot];
            for (row_index, row) in floating.iter_mut().enumerate() {
                if row_index != pivot && row[column].abs() > EPS {
                    let factor = row[column];
                    for (cell, reference) in row
                        .iter_mut()
                        .skip(column + 1)
                        .zip(pivot_row.iter().skip(column + 1))
                    {
                        *cell -= *reference * factor;
                    }
                }
            }
        }
    }
    rank
}

fn generate_matrix(seed: [u8; 32]) -> [[u8; 64]; 64] {
    let mut rng = Xoshiro256PlusPlus::from_seed(seed);
    // Same full-rank retry semantics as the pinned Kaspa reference.
    loop {
        let mut matrix = [[0_u8; 64]; 64];
        for row in &mut matrix {
            for chunk in row.chunks_exact_mut(16) {
                let word = rng.next();
                for (i, entry) in chunk.iter_mut().enumerate() {
                    *entry = ((word >> (i * 4)) & 15) as u8;
                }
            }
        }
        if matrix_rank(&matrix) == 64 {
            return matrix;
        }
    }
}

#[derive(Clone)]
pub(crate) struct NativeKHeavyHasher {
    nonce_state: CShake256,
    heavy_state: CShake256,
    matrix: [[u8; 64]; 64],
}

impl NativeKHeavyHasher {
    pub(crate) fn new(pre_pow_bytes: &[u8]) -> Self {
        let pre_pow: [u8; 32] = Keccak256::digest(pre_pow_bytes).into();
        // Reuse finalized customization and immutable preimage state for every
        // nonce, matching the old prepared miner's cached sponge behavior.
        let mut nonce_state = CShake256::from_core(CShake256Core::new(b"ProofOfWorkHash"));
        nonce_state.update(&pre_pow);
        nonce_state.update(&[0_u8; 40]);
        let heavy_state = CShake256::from_core(CShake256Core::new(b"HeavyHash"));
        Self {
            nonce_state,
            heavy_state,
            matrix: generate_matrix(pre_pow),
        }
    }

    /// Cache matrix/prehash for repeated nonces of the same chain-bound header.
    pub(crate) fn hash_nonce(&self, nonce: u64) -> [u8; 32] {
        let mut sponge = self.nonce_state.clone();
        sponge.update(&nonce.to_le_bytes());
        let mut nonce_pow = [0_u8; 32];
        sponge.finalize_xof().read(&mut nonce_pow);
        let mut nibbles = [0_u8; 64];
        for (i, value) in nonce_pow.iter().enumerate() {
            nibbles[2 * i] = value >> 4;
            nibbles[2 * i + 1] = value & 15;
        }
        let mut matrix_xor = [0_u8; 32];
        for i in 0..32 {
            let row_sum = |row: usize| -> u32 {
                (0..64)
                    .map(|j| u32::from(self.matrix[row][j]) * u32::from(nibbles[j]))
                    .sum()
            };
            let high = ((row_sum(2 * i) >> 10) & 15) as u8;
            let low = ((row_sum(2 * i + 1) >> 10) & 15) as u8;
            matrix_xor[i] = nonce_pow[i] ^ ((high << 4) | low);
        }
        let mut sponge = self.heavy_state.clone();
        sponge.update(&matrix_xor);
        let mut digest = [0_u8; 32];
        sponge.finalize_xof().read(&mut digest);
        digest
    }
}

pub(crate) fn native_kheavyhash_digest(pre_pow_bytes: &[u8], nonce: u64) -> [u8; 32] {
    NativeKHeavyHasher::new(pre_pow_bytes).hash_nonce(nonce)
}

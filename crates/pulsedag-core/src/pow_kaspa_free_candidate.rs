//! EXPERIMENTAL, NON-AUTHORITATIVE native-v3 KHeavyHash candidate without Kaspa crates.
//!
//! Derived from the already-passing independent Rust/Python golden-vector oracle.
//! This is deliberately NOT used by PoW consensus, mining, P2P, wallet, or startup.
//! The bounded 32-retry matrix experiment MUST NOT be treated as production
//! consensus policy, security remediation, or a reason to remove kaspa-pow.
//! See #1387. Existing canonical engine and locked dependencies remain intact.

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

fn generate_matrix(seed: [u8; 32]) -> Option<[[u8; 64]; 64]> {
    let mut rng = Xoshiro256PlusPlus::from_seed(seed);
    for _ in 0..32_usize {
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
            return Some(matrix);
        }
    }
    None // fail closed if a bounded diagnostic unexpectedly lacks a full-rank matrix
}

fn cshake256(message: &[u8], domain: &[u8]) -> [u8; 32] {
    let mut engine = CShake256::from_core(CShake256Core::new(domain));
    engine.update(message);
    let mut result = [0_u8; 32];
    engine.finalize_xof().read(&mut result);
    result
}

/// Compute one **non-authoritative, bounded** candidate KHeavyHash.
///
/// Returns None if the diagnostic 32-matrix full-rank budget is exceeded.
/// The authoritative Kaspa-backed engine remains unchanged and selected.
pub fn experimental_kheavyhash_digest(seed_bytes: &[u8], nonce: u64) -> Option<[u8; 32]> {
    let pre_pow: [u8; 32] = Keccak256::digest(seed_bytes).into();
    let mut nonce_message = Vec::with_capacity(80);
    nonce_message.extend_from_slice(&pre_pow);
    nonce_message.extend_from_slice(&[0_u8; 40]);
    nonce_message.extend_from_slice(&nonce.to_le_bytes());
    let nonce_pow = cshake256(&nonce_message, b"ProofOfWorkHash");
    let matrix = generate_matrix(pre_pow)?;
    let mut nibbles = [0_u8; 64];
    for (i, value) in nonce_pow.iter().enumerate() {
        nibbles[2 * i] = value >> 4;
        nibbles[2 * i + 1] = value & 15;
    }
    let mut matrix_xor = [0_u8; 32];
    for i in 0..32 {
        let row_sum = |row: usize| -> u32 {
            (0..64)
                .map(|j| u32::from(matrix[row][j]) * u32::from(nibbles[j]))
                .sum()
        };
        let high = ((row_sum(2 * i) >> 10) & 15) as u8;
        let low = ((row_sum(2 * i + 1) >> 10) & 15) as u8;
        matrix_xor[i] = nonce_pow[i] ^ ((high << 4) | low);
    }
    Some(cshake256(&matrix_xor, b"HeavyHash"))
}


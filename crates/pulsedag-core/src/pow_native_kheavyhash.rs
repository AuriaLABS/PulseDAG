//! Native KHeavyHash; same work digest and full-rank retry semantics as
//! pinned Kaspa, without compiling its transitive workflow dependencies.
//! Do not impose the test-only 32-retry cap on production PoW.
use sha3::{Digest, Keccak256};

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

// Reference round initializations from pinned upstream pow_hashers.rs.
// They incorporate the cSHAKE256 domain name and final padding bits. The
// two Keccak-f[1600] rounds below are exactly the original byte pipeline:
// prehash||40 zero bytes||nonce and heavy matrix-xor. No hash-policy change.
const POW_INITIAL_STATE: [u64; 25] = [
    1242148031264380989,
    3008272977830772284,
    2188519011337848018,
    1992179434288343456,
    8876506674959887717,
    5399642050693751366,
    1745875063082670864,
    8605242046444978844,
    17936695144567157056,
    3343109343542796272,
    1123092876221303306,
    4963925045340115282,
    17037383077651887893,
    16629644495023626889,
    12833675776649114147,
    3784524041015224902,
    1082795874807940378,
    13952716920571277634,
    13411128033953605860,
    15060696040649351053,
    9928834659948351306,
    5237849264682708699,
    12825353012139217522,
    6706187291358897596,
    196324915476054915,
];
const HEAVY_INITIAL_STATE: [u64; 25] = [
    4239941492252378377,
    8746723911537738262,
    8796936657246353646,
    1272090201925444760,
    16654558671554924250,
    8270816933120786537,
    13907396207649043898,
    6782861118970774626,
    9239690602118867528,
    11582319943599406348,
    17596056728278508070,
    15212962468105129023,
    7812475424661425213,
    3370482334374859748,
    5690099369266491460,
    8596393687355028144,
    570094237299545110,
    9119540418498120711,
    16901969272480492857,
    13372017233735502424,
    14372891883993151831,
    5171152063242093102,
    10573107899694386186,
    6096431547456407061,
    1592359455985097269,
];

#[derive(Clone)]
pub(crate) struct NativeKHeavyHasher {
    pow_state: [u64; 25],
    matrix: [[u16; 64]; 64],
}

impl NativeKHeavyHasher {
    pub(crate) fn new(pre_pow_bytes: &[u8]) -> Self {
        let pre_pow: [u8; 32] = Keccak256::digest(pre_pow_bytes).into();
        let mut pow_state = POW_INITIAL_STATE;
        for (index, word) in pre_pow.chunks_exact(8).enumerate() {
            let bytes: [u8; 8] = word.try_into().expect("exact fixed-width prehash word");
            pow_state[index] ^= u64::from_le_bytes(bytes);
        }
        Self {
            pow_state,
            matrix: generate_matrix(pre_pow).map(|row| row.map(u16::from)),
        }
    }

    /// Reuse domain-initialized permutation state across all nonces, matching
    /// pinned Kaspa's exact 25-lane sponge and canonical little-endian hashing.
    pub(crate) fn hash_nonce(&self, nonce: u64) -> [u8; 32] {
        let mut state = self.pow_state;
        state[9] ^= nonce;
        keccak::f1600(&mut state);
        let mut nonce_pow = [0_u8; 32];
        for (word, lane) in nonce_pow.chunks_exact_mut(8).zip(state.iter()) {
            word.copy_from_slice(&lane.to_le_bytes());
        }

        let mut nibbles = [0_u8; 64];
        for (i, value) in nonce_pow.iter().enumerate() {
            nibbles[2 * i] = value >> 4;
            nibbles[2 * i + 1] = value & 15;
        }
        let mut matrix_xor = [0_u8; 32];
        for (i, output) in matrix_xor.iter_mut().enumerate() {
            let even = &self.matrix[2 * i];
            let odd = &self.matrix[2 * i + 1];
            let mut high = 0_u16;
            let mut low = 0_u16;
            for (j, &nibble) in nibbles.iter().enumerate() {
                let value = u16::from(nibble);
                high += even[j] * value;
                low += odd[j] * value;
            }
            *output = nonce_pow[i] ^ (((high >> 10) << 4) as u8 | (low >> 10) as u8);
        }

        let mut heavy_state = HEAVY_INITIAL_STATE;
        for (index, word) in matrix_xor.chunks_exact(8).enumerate() {
            let bytes: [u8; 8] = word.try_into().expect("exact fixed-width heavy input word");
            heavy_state[index] ^= u64::from_le_bytes(bytes);
        }
        keccak::f1600(&mut heavy_state);
        let mut digest = [0_u8; 32];
        for (word, lane) in digest.chunks_exact_mut(8).zip(heavy_state.iter()) {
            word.copy_from_slice(&lane.to_le_bytes());
        }
        digest
    }
}

pub(crate) fn native_kheavyhash_digest(pre_pow_bytes: &[u8], nonce: u64) -> [u8; 32] {
    NativeKHeavyHasher::new(pre_pow_bytes).hash_nonce(nonce)
}

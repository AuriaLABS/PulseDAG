//! TEST-ONLY: a Kaspa-crate-free, plain Rust+sha3 KHeavyHash reference.
//!
//! It is deliberately NOT the production PoW implementation. It uses only
//! sha3, std and PulseDAG's public v3 header types in this integration test,
//! then matches the independent Python oracle's raw, frozen stage vectors.
//! Removing kaspa-hashes/kaspa-pow is NOT authorized by these seven fixtures;
//! production v1/v2/v3 behavior, platforms, nonce-search speed, consensus
//! vectors and dependencies still need independent migration validation (#1387).

use pulsedag_core::{
    header_v3::BlockHeaderV3,
    pow_kaspa_free_candidate::experimental_kheavyhash_digest,
    pow_v3::{evaluate_v3_header_pow, v3_pow_preimage_bytes},
};
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

fn generate_matrix(seed: [u8; 32]) -> ([[u8; 64]; 64], usize) {
    let mut rng = Xoshiro256PlusPlus::from_seed(seed);
    for retry in 0..32_usize {
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
            return (matrix, retry);
        }
    }
    panic!("test-only independent matrix rank retry bound exceeded");
}

fn cshake256(message: &[u8], domain: &[u8]) -> [u8; 32] {
    let mut engine = CShake256::from_core(CShake256Core::new(domain));
    engine.update(message);
    let mut result = [0_u8; 32];
    engine.finalize_xof().read(&mut result);
    result
}

struct Stages {
    pre_pow: [u8; 32],
    nonce_pow: [u8; 32],
    retries: usize,
    matrix_xor: [u8; 32],
    heavy_hash: [u8; 32],
}

fn independent_stages(seed_bytes: &[u8], nonce: u64) -> Stages {
    let pre_pow: [u8; 32] = Keccak256::digest(seed_bytes).into();
    let mut nonce_message = Vec::with_capacity(80);
    nonce_message.extend_from_slice(&pre_pow);
    nonce_message.extend_from_slice(&[0_u8; 40]);
    nonce_message.extend_from_slice(&nonce.to_le_bytes());
    let nonce_pow = cshake256(&nonce_message, b"ProofOfWorkHash");
    let (matrix, retries) = generate_matrix(pre_pow);
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
    let heavy_hash = cshake256(&matrix_xor, b"HeavyHash");
    Stages {
        pre_pow,
        nonce_pow,
        retries,
        matrix_xor,
        heavy_hash,
    }
}

#[test]
fn pure_rust_non_kaspa_oracle_matches_seven_frozen_v3_vectors_and_current_engine() {
    // Must be usable as a test even if release code continues using Kaspa.
    // Python scripts/native_v3_kheavyhash_oracle.py independently generated
    // each frozen stage; the Rust candidate imports no Kaspa types.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/fixtures/native_v3_kheavyhash_vectors.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["schema"].as_str(),
        Some("pulsedag.dormant-v3-independent-kheavyhash-v1")
    );
    let cases = fixture["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let mut invalid_count = 0;
    for case in cases {
        let label = case["name"].as_str().unwrap();
        let chain = case["chain_id"].as_str().unwrap();
        let header: BlockHeaderV3 = serde_json::from_value(case["header"].clone()).unwrap();
        assert!(header.height > 0, "{label}: only non-genesis PoW examples");
        let seed = v3_pow_preimage_bytes(&header, chain).unwrap();
        assert_eq!(
            hex::encode(&seed),
            case["canonical_seed_hex"].as_str().unwrap()
        );
        let candidate = independent_stages(&seed, header.nonce);
        let reusable = experimental_kheavyhash_digest(&seed, header.nonce)
            .expect("frozen research matrix fits bounded candidate budget");
        assert_eq!(
            reusable, candidate.heavy_hash,
            "{label}: reusable candidate parity"
        );
        let expected = &case["stages"];
        assert_eq!(
            hex::encode(candidate.pre_pow),
            expected["pre_pow_keccak256"].as_str().unwrap(),
            "{label}: pre-pow"
        );
        assert_eq!(
            hex::encode(candidate.nonce_pow),
            expected["nonce_pow_hash"].as_str().unwrap(),
            "{label}: initial cSHAKE"
        );
        assert_eq!(
            candidate.retries as u64,
            expected["matrix_retries"].as_u64().unwrap(),
            "{label}: matrix rank retries"
        );
        assert_eq!(
            hex::encode(candidate.matrix_xor),
            expected["matrix_xor"].as_str().unwrap(),
            "{label}: matrix mixing"
        );
        assert_eq!(
            hex::encode(candidate.heavy_hash),
            expected["kheavyhash"].as_str().unwrap(),
            "{label}: final cSHAKE"
        );
        let native = evaluate_v3_header_pow(&header, chain).unwrap();
        assert_eq!(
            hex::encode(candidate.heavy_hash),
            native.pow.hash_hex,
            "{label}: existing canonical backend"
        );
        assert_eq!(
            native.pow.accepted,
            case["expected_meets_target"].as_bool().unwrap(),
            "{label}: canonical full 256-bit PoW verdict"
        );
        if !native.pow.accepted {
            invalid_count += 1;
        }
    }
    assert!(invalid_count >= 2);
}

/// Migration evidence only: this stresses nonce extremes and chain identity
/// beyond the seven frozen single-nonce fixtures. It is NOT a production PoW
/// backend, a release-parity certification, or permission to drop Kaspa.
#[test]
fn independent_backend_matches_nonce_boundaries_and_foreign_chain_for_all_frozen_seeds() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/fixtures/native_v3_kheavyhash_vectors.json"
    ))
    .unwrap();
    let cases = fixture["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 7);

    for case in cases {
        let label = case["name"].as_str().unwrap();
        let chain = case["chain_id"].as_str().unwrap();
        let mut header: BlockHeaderV3 = serde_json::from_value(case["header"].clone()).unwrap();
        let original_nonce = header.nonce;
        let seed = v3_pow_preimage_bytes(&header, chain).unwrap();
        let target = pulsedag_core::pow::target_from_bits(header.difficulty);

        // The pre-PoW seed must not accidentally incorporate the nonce.
        // The reference has no imports from kaspa-hashes / kaspa-pow.
        let nonces = std::collections::BTreeSet::from([
            0_u64,
            1,
            2,
            17,
            u64::MAX - 1,
            u64::MAX,
            original_nonce,
        ]);
        for nonce in nonces {
            header.nonce = nonce;
            assert_eq!(
                v3_pow_preimage_bytes(&header, chain).unwrap(),
                seed,
                "{label}: pre-PoW seed unexpectedly depends on nonce {nonce}"
            );
            let independent = independent_stages(&seed, nonce);
            let reusable = experimental_kheavyhash_digest(&seed, nonce)
                .expect("frozen nonce-extreme corpus fits bounded diagnostic");
            assert_eq!(
                reusable, independent.heavy_hash,
                "{label}: standalone Kaspa-free candidate differs at nonce {nonce}"
            );
            let production = evaluate_v3_header_pow(&header, chain).unwrap();
            assert_eq!(
                independent.heavy_hash, production.pow.hash,
                "{label}: KHeavyHash backend mismatch at nonce {nonce}"
            );
            assert_eq!(
                production.pow.accepted,
                independent.heavy_hash <= target,
                "{label}: full-256-bit target disagreement at nonce {nonce}"
            );
        }

        let foreign_chain = format!("{chain}-different-network");
        let foreign_seed = v3_pow_preimage_bytes(&header, &foreign_chain).unwrap();
        assert_ne!(seed, foreign_seed, "{label}: network seed must differ");
        let independent = independent_stages(&foreign_seed, header.nonce);
        assert_eq!(
            experimental_kheavyhash_digest(&foreign_seed, header.nonce),
            Some(independent.heavy_hash),
            "{label}: foreign-chain candidate parity"
        );
        let production = evaluate_v3_header_pow(&header, &foreign_chain).unwrap();
        assert_eq!(
            independent.heavy_hash, production.pow.hash,
            "{label}: foreign-chain backend mismatch"
        );
    }
}

/// Migration evidence only: the same Kaspa-free KHeavyHash primitive must
/// also agree with the *legacy seconds-based* header validation route.
/// The v1/v2 preimages and admission policies remain distinct from native v3.
#[test]
fn legacy_v1_v2_header_pow_matches_independent_backend_across_nonce_boundaries() {
    use pulsedag_core::{
        pow::{canonical_pow_engine, pow_hash, target_from_bits, PowEngine},
        protocol::{BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2},
        types::{canonical_mining_preimage_bytes, BlockHeader},
    };

    for version in [BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2] {
        for difficulty in [0x207f_ffff, 0x1f7f_ffff] {
            let mut header = BlockHeader {
                version,
                parents: vec!["22".repeat(32), "11".repeat(32)],
                timestamp: 1_800_000_000,
                difficulty,
                nonce: 0,
                merkle_root: "33".repeat(32),
                state_root: "44".repeat(32),
                blue_score: 2,
                height: 5,
            };
            let seed = canonical_mining_preimage_bytes(&header);
            let target = target_from_bits(difficulty);
            for nonce in [0_u64, 1, 17, u64::MAX] {
                header.nonce = nonce;
                assert_eq!(
                    canonical_mining_preimage_bytes(&header),
                    seed,
                    "legacy nonce must not alter the canonical pre-PoW bytes"
                );
                let independent = independent_stages(&seed, nonce);
                assert_eq!(
                    experimental_kheavyhash_digest(&seed, nonce),
                    Some(independent.heavy_hash),
                    "legacy version {version} nonce {nonce} candidate parity"
                );
                let canonical = canonical_pow_engine().evaluate_header(&header);
                let direct = canonical_pow_engine()
                    .evaluate_pre_pow_bytes_with_nonce(&seed, nonce, difficulty);
                assert_eq!(
                    independent.heavy_hash, canonical.hash,
                    "version {version}, bits {difficulty:#010x}, nonce {nonce}"
                );
                assert_eq!(canonical, direct);
                assert_eq!(canonical.hash, pow_hash(&header));
                assert_eq!(canonical.accepted, independent.heavy_hash <= target);
            }

            let mut shuffled = header.clone();
            shuffled.parents.reverse();
            assert_eq!(canonical_mining_preimage_bytes(&shuffled), seed);
            assert_eq!(pow_hash(&shuffled), pow_hash(&header));
        }
    }
}

#[test]
fn seed_xoshiro_and_matrix_rank_negative_invariants() {
    let expected_empty_keccak = "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470";
    assert_eq!(hex::encode(Keccak256::digest([])), expected_empty_keccak);
    assert_eq!(matrix_rank(&[[0; 64]; 64]), 0);
    let mut matrix = [[0_u8; 64]; 64];
    for (i, row) in matrix.iter_mut().enumerate() {
        row[i] = 1;
    }
    assert_eq!(matrix_rank(&matrix), 64);
    matrix[0] = matrix[1];
    assert_eq!(matrix_rank(&matrix), 63);
    let seed = [42_u8; 32];
    let (generated, _) = generate_matrix(seed);
    assert_eq!(matrix_rank(&generated), 64);
}

/// Non-normative exact-integer witness: nonzero determinant modulo a prime
/// proves the same integer matrix is nonsingular over the rationals.
/// A modular rank deficiency alone is NOT proof of real-number singularity.
fn modular_matrix_rank(matrix: &[[u8; 64]; 64], prime: u64) -> usize {
    fn mod_pow(mut base: u64, mut exponent: u64, modulus: u64) -> u64 {
        let mut answer = 1_u64;
        while exponent != 0 {
            if exponent & 1 != 0 {
                answer = (answer * base) % modulus;
            }
            base = (base * base) % modulus;
            exponent >>= 1;
        }
        answer
    }

    let mut work = matrix.map(|row| row.map(u64::from));
    let mut rank = 0_usize;
    for column in 0..64 {
        let pivot = (rank..64).find(|&row| work[row][column] != 0);
        let Some(pivot) = pivot else {
            continue;
        };
        work.swap(rank, pivot);
        let inverse = mod_pow(work[rank][column], prime - 2, prime);
        for value in work[rank].iter_mut().skip(column) {
            *value = (*value * inverse) % prime;
        }
        let pivot_row = work[rank];
        for row in work.iter_mut().skip(rank + 1) {
            let factor = row[column];
            if factor != 0 {
                for (entry, reference) in row.iter_mut().zip(pivot_row.iter()).skip(column) {
                    *entry = (*entry + prime - (factor * *reference) % prime) % prime;
                }
            }
        }
        rank += 1;
        if rank == 64 {
            break;
        }
    }
    rank
}

#[test]
fn frozen_kheavyhash_matrices_have_independent_exact_integer_full_rank_witnesses() {
    // Do not replace the production/Kaspa matrix rank selection on this basis.
    // This tests seven frozen seeds using two independent prime fields, with
    // no f64 in the witness. Nonzero determinants prove exact nonsingularity
    // for THESE seven matrices; they do not certify all possible seeds.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/fixtures/native_v3_kheavyhash_vectors.json"
    ))
    .unwrap();
    let cases = fixture["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 7);

    for case in cases {
        let label = case["name"].as_str().unwrap();
        let bytes = hex::decode(case["stages"]["pre_pow_keccak256"].as_str().unwrap()).unwrap();
        let seed: [u8; 32] = bytes.try_into().unwrap();
        let (matrix, retries) = generate_matrix(seed);
        assert_eq!(
            retries as u64,
            case["stages"]["matrix_retries"].as_u64().unwrap(),
            "{label}: selected matrix attempt"
        );
        for prime in [65_521_u64, 65_519_u64] {
            assert_eq!(
                modular_matrix_rank(&matrix, prime),
                64,
                "{label}: modular nonsingularity witness for prime {prime}"
            );
        }
    }

    let mut matrix = [[0_u8; 64]; 64];
    for (i, row) in matrix.iter_mut().enumerate() {
        row[i] = 1;
    }
    assert_eq!(modular_matrix_rank(&matrix, 65_521), 64);
    matrix[0] = matrix[1];
    assert_eq!(modular_matrix_rank(&matrix, 65_521), 63);
}

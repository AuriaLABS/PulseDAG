use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pulsedag_core::pow::{
    bits_from_target, mine_header, pow_accepts, pow_evaluate, pow_hash, pow_preimage_bytes, target_from_bits,
};
use pulsedag_core::types::BlockHeader;

/// Run with:
///   cargo bench -p pulsedag-core --bench pow_core
///
/// Focus: v2.2.10 kHeavyHash hashing/validation cost, target conversion,
/// canonical header adapter serialization, and mining attempt throughput.
fn fixture_header() -> BlockHeader {
    BlockHeader {
        version: 1,
        parents: vec![
            "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
        ],
        timestamp: 1_713_370_000,
        difficulty: 64,
        nonce: 7,
        merkle_root: "4e3c14f9f6f42753fe4f2ddc2b53bfcbf065a90f8f919f94f0f7a87f6ecf7cf9".to_string(),
        state_root: "8f8793fa75efec8df5a0013d70e30e954f05064fa5f0db3ed8cb563127f90c0d".to_string(),
        blue_score: 1_024,
        height: 1_024,
    }
}

fn bench_pow_core(c: &mut Criterion) {
    let header = fixture_header();
    let preimage = pow_preimage_bytes(&header);

    let mut group = c.benchmark_group("pow_v2_2_10_core");

    // Header adapter serialization (canonical preimage path).
    group.throughput(Throughput::Bytes(preimage.len() as u64));
    group.bench_function("header_adapter_serialization", |b| {
        b.iter(|| black_box(pow_preimage_bytes(black_box(&header))))
    });

    // Single kHeavyHash PoW hash cost.
    group.throughput(Throughput::Elements(1));
    group.bench_function("pow_hash_single", |b| {
        b.iter(|| black_box(pow_hash(black_box(&header))))
    });

    // Full validation path (hash + target check) as a check_pow equivalent.
    group.bench_function("check_pow_validate", |b| {
        b.iter(|| black_box(pow_accepts(black_box(&header))))
    });

    // Target conversion cost in compact->target and target->compact directions.
    for bits in [0x1d00ffffu32, 0x1e0ffff0u32, 0x1f07ffffu32] {
        group.bench_with_input(BenchmarkId::new("target_from_bits", bits), &bits, |b, input| {
            b.iter(|| black_box(target_from_bits(black_box(*input))))
        });

        let target = target_from_bits(bits);
        group.bench_with_input(BenchmarkId::new("bits_from_target", bits), &target, |b, input| {
            b.iter(|| black_box(bits_from_target(black_box(input))))
        });
    }

    // External mining-loop attempt cost using an easy target to ensure quick hit.
    let mut easy = header.clone();
    easy.difficulty = 1;
    group.bench_function("mining_loop_easy_target", |b| {
        b.iter(|| {
            let (found_header, found, attempts, hash_hex) = mine_header(black_box(easy.clone()), 128);
            black_box((found_header, found, attempts, hash_hex))
        })
    });

    group.finish();
}

fn bench_optional_miner_style(c: &mut Criterion) {
    let mut header = fixture_header();
    header.difficulty = 1;

    let mut group = c.benchmark_group("pow_v2_2_10_miner_style");
    group.throughput(Throughput::Elements(1));

    // One-thread fixed nonce range probe.
    group.bench_function("single_thread_fixed_nonce_range", |b| {
        b.iter(|| {
            let mut winning_nonce = None;
            for nonce in 0u64..10_000 {
                header.nonce = nonce;
                if pow_evaluate(&header).accepted {
                    winning_nonce = Some(nonce);
                    break;
                }
            }
            black_box(winning_nonce)
        })
    });

    // Multi-thread fixed nonce range probe (split into static shards).
    group.bench_function("multi_thread_fixed_nonce_range_4", |b| {
        b.iter(|| {
            let workers = 4u64;
            let range = 40_000u64;
            let mut found = false;
            for worker in 0..workers {
                let mut local = header.clone();
                let mut nonce = worker;
                while nonce < range {
                    local.nonce = nonce;
                    if pow_evaluate(&local).accepted {
                        found = true;
                        break;
                    }
                    nonce += workers;
                }
                if found {
                    break;
                }
            }
            black_box(found)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_pow_core, bench_optional_miner_style);
criterion_main!(benches);

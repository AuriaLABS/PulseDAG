use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pulsedag_core::pow::{pow_accepts, pow_hash_score_u64, pow_preimage_bytes};
use pulsedag_core::types::BlockHeader;

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
    let preimage_len = pow_preimage_bytes(&header).len() as u64;

    let mut group = c.benchmark_group("pow_core");
    group.throughput(Throughput::Bytes(preimage_len));
    group.bench_function("pow_preimage_bytes", |b| {
        b.iter(|| black_box(pow_preimage_bytes(black_box(&header))))
    });

    group.throughput(Throughput::Elements(1));
    group.bench_function("pow_hash_score_u64", |b| {
        b.iter(|| black_box(pow_hash_score_u64(black_box(&header))))
    });

    for difficulty in [1u32, 64u32, 512u32] {
        let mut case = header.clone();
        case.difficulty = difficulty;
        group.bench_with_input(
            BenchmarkId::new("pow_accepts", difficulty),
            &case,
            |b, input| b.iter(|| black_box(pow_accepts(black_box(input)))),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_pow_core);
criterion_main!(benches);

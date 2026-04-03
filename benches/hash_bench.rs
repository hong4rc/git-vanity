use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

// Import internals via the binary crate's public modules
// Since git-vanity is a binary, we benchmark the core logic directly

fn sha1_hash_throughput(c: &mut Criterion) {
    use sha1::{Digest, Sha1};

    // Simulate a typical commit object (~300 bytes)
    let prefix = b"commit 287\0tree 4b825dc642cb6eb9a060e54bf899d8b7c0e23674\nauthor Test <test@test.com> 1700000000 +0000\ncommitter Test <test@test.com> 1700000000 +0000\nx-nonce ";
    let suffix = b"\n\ntest commit message\n";
    let nonce = [0x80u8, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89];

    // Pre-compute prefix state (same as IncrementalHasher)
    let mut prefix_state = Sha1::new();
    prefix_state.update(prefix);

    let mut group = c.benchmark_group("sha1");
    group.throughput(Throughput::Elements(1));

    group.bench_function("incremental_hash", |b| {
        b.iter(|| {
            let mut hasher = prefix_state.clone();
            hasher.update(black_box(&nonce));
            hasher.update(suffix);
            let result: [u8; 20] = hasher.finalize().into();
            black_box(result);
        })
    });

    group.bench_function("full_hash", |b| {
        b.iter(|| {
            let mut hasher = Sha1::new();
            hasher.update(prefix);
            hasher.update(black_box(&nonce));
            hasher.update(suffix);
            let result: [u8; 20] = hasher.finalize().into();
            black_box(result);
        })
    });

    group.finish();
}

fn nonce_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("nonce");
    group.throughput(Throughput::Elements(1));

    group.bench_function("generate", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let mut nonce = [0x80u8; 10];
            nonce[0] = 0x80 | ((0u16 >> 8) as u8 & 0x7F);
            nonce[1] = 0x80 | (0u16 as u8 & 0x7F);
            let bytes = counter.to_le_bytes();
            for i in 0..8 {
                nonce[i + 2] = 0x80 | (bytes[i] & 0x7F);
            }
            counter += 1;
            black_box(nonce);
        })
    });

    group.finish();
}

fn pattern_matching(c: &mut Criterion) {
    let hash = [
        0xca, 0xfe, 0xba, 0xbe, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56,
        0x78, 0x9a, 0xbc, 0xde, 0xf0,
    ];

    let mut group = c.benchmark_group("pattern");
    group.throughput(Throughput::Elements(1));

    // Prefix match (4 nibbles)
    group.bench_function("prefix_4char", |b| {
        let nibbles: Vec<u8> = vec![0x0c, 0x0a, 0x0f, 0x0e]; // "cafe"
        b.iter(|| {
            let matched = nibbles.iter().enumerate().all(|(i, &expected)| {
                let byte = hash[i / 2];
                let actual = if i % 2 == 0 {
                    (byte >> 4) & 0x0F
                } else {
                    byte & 0x0F
                };
                actual == expected
            });
            black_box(matched);
        })
    });

    // Prefix match (6 nibbles)
    group.bench_function("prefix_6char", |b| {
        let nibbles: Vec<u8> = vec![0x0c, 0x0a, 0x0f, 0x0e, 0x0b, 0x0a]; // "cafeba"
        b.iter(|| {
            let matched = nibbles.iter().enumerate().all(|(i, &expected)| {
                let byte = hash[i / 2];
                let actual = if i % 2 == 0 {
                    (byte >> 4) & 0x0F
                } else {
                    byte & 0x0F
                };
                actual == expected
            });
            black_box(matched);
        })
    });

    // Pair match (scan all nibbles)
    group.bench_function("pair_scan", |b| {
        b.iter(|| {
            let matched = (0..20).any(|i| {
                let byte = hash[i];
                let hi = (byte >> 4) & 0x0F;
                let lo = byte & 0x0F;
                if hi == lo {
                    return true;
                }
                if i < 19 {
                    let next_hi = (hash[i + 1] >> 4) & 0x0F;
                    if lo == next_hi {
                        return true;
                    }
                }
                false
            });
            black_box(matched);
        })
    });

    group.finish();
}

fn end_to_end(c: &mut Criterion) {
    use sha1::{Digest, Sha1};

    let prefix = b"commit 287\0tree 4b825dc642cb6eb9a060e54bf899d8b7c0e23674\nauthor Test <test@test.com> 1700000000 +0000\ncommitter Test <test@test.com> 1700000000 +0000\nx-nonce ";
    let suffix = b"\n\ntest commit message\n";
    let target_nibbles: Vec<u8> = vec![0x0c, 0x0a, 0x0f, 0x0e]; // "cafe"

    let mut prefix_state = Sha1::new();
    prefix_state.update(prefix);

    let mut group = c.benchmark_group("e2e");
    group.throughput(Throughput::Elements(1));

    // Full cycle: nonce gen + hash + prefix check
    group.bench_function("nonce_hash_match", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            // Nonce generation
            let mut nonce = [0x80u8; 10];
            let bytes = counter.to_le_bytes();
            for i in 0..8 {
                nonce[i + 2] = 0x80 | (bytes[i] & 0x7F);
            }
            counter += 1;

            // Hash
            let mut hasher = prefix_state.clone();
            hasher.update(&nonce);
            hasher.update(suffix);
            let hash: [u8; 20] = hasher.finalize().into();

            // Pattern check
            let matched = target_nibbles.iter().enumerate().all(|(i, &expected)| {
                let byte = hash[i / 2];
                let actual = if i % 2 == 0 {
                    (byte >> 4) & 0x0F
                } else {
                    byte & 0x0F
                };
                actual == expected
            });
            black_box(matched);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    sha1_hash_throughput,
    nonce_generation,
    pattern_matching,
    end_to_end
);
criterion_main!(benches);

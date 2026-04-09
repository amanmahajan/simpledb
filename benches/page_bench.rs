use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rand::{Rng, RngExt, rng};
use simpledb::page::Page;

fn bench_put(c: &mut Criterion) {
    let mut group = c.benchmark_group("page_put");

    for size in [16usize, 64, 256] {
        let key = format!("k-{size}").into_bytes();
        let val = vec![42u8; size];

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let mut page = Page::new(1);
                let mut i = 0usize;

                loop {
                    let mut k = key.clone();
                    k.extend_from_slice(i.to_string().as_bytes());

                    if page.put(black_box(&k), black_box(&val)).is_err() {
                        break;
                    }
                    i += 1;
                }
                black_box(i);
            })
        });
    }

    group.finish();
}
fn bench_mixed_random_churn(c: &mut Criterion) {
    const INITIAL_TUPLES: usize = 200;
    const OP_COUNT: usize = 100_000;

    let keys: Vec<Vec<u8>> = (0..INITIAL_TUPLES)
        .map(|i| format!("rnd-{i:03}").into_bytes())
        .collect();

    let mut seed_rng = rng();
    let update_targets: Vec<usize> = (0..OP_COUNT)
        .map(|_| seed_rng.random_range(0..INITIAL_TUPLES))
        .collect();
    let delete_targets: Vec<usize> = (0..OP_COUNT)
        .map(|_| seed_rng.random_range(0..INITIAL_TUPLES))
        .collect();
    let get_targets: Vec<usize> = (0..OP_COUNT)
        .map(|_| seed_rng.random_range(0..INITIAL_TUPLES))
        .collect();

    let mut group = c.benchmark_group("page_mixed_random_churn");
    group.bench_function("insert200_update100k_delete100k_get100k", |b| {
        b.iter(|| {
            let mut page = Page::new(1234);
            page.set_dead_tuple_compact_percent(50);

            let initial_val = [1u8; 8];
            for key in &keys {
                page.put(key, &initial_val)
                    .expect("initial insert should fit in 8KB page");
            }

            for (i, &idx) in update_targets.iter().enumerate() {
                let update_val = (i as u64).to_le_bytes();
                let _ = page.put(black_box(&keys[idx]), black_box(&update_val));
            }

            for &idx in &delete_targets {
                let _ = page.remove(black_box(&keys[idx]));
            }

            let mut hits = 0usize;
            for &idx in &get_targets {
                if page.get(black_box(&keys[idx])).is_some() {
                    hits += 1;
                }
            }

            black_box((
                hits,
                page.slot_count(),
                page.free_space_bytes(),
                page.dead_tuple_bytes(),
            ));
        })
    });
    group.finish();
}

fn bench_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("page_get");

    for records in [32usize, 128, 256] {
        let mut page = Page::new(2);
        let val = vec![7u8; 32];
        let mut keys = Vec::with_capacity(records);

        for i in 0..records {
            let key = format!("key-{i:04}").into_bytes();
            if page.put(&key, &val).is_err() {
                break;
            }
            keys.push(key);
        }

        let existing = keys[keys.len() / 2].clone();
        let missing = b"key-999999".to_vec();

        group.bench_with_input(BenchmarkId::new("hit", records), &records, |b, _| {
            b.iter(|| {
                let v = page.get(black_box(&existing));
                black_box(v);
            })
        });

        group.bench_with_input(BenchmarkId::new("miss", records), &records, |b, _| {
            b.iter(|| {
                let v = page.get(black_box(&missing));
                black_box(v);
            })
        });
    }

    group.finish();
}

fn bench_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("page_remove");

    for records in [32usize, 128, 256] {
        group.bench_with_input(BenchmarkId::from_parameter(records), &records, |b, &n| {
            b.iter(|| {
                let mut page = Page::new(3);
                let val = vec![1u8; 32];
                let mut keys = Vec::with_capacity(n);

                for i in 0..n {
                    let key = format!("del-{i:04}").into_bytes();
                    if page.put(&key, &val).is_err() {
                        break;
                    }
                    keys.push(key);
                }

                for key in &keys {
                    let out = page.remove(black_box(key));
                    black_box(out);
                }
            })
        });
    }

    group.finish();
}
fn bench_compaction_thresholds(c: &mut Criterion) {
    let mut group = c.benchmark_group("page_compaction_threshold");

    for threshold in [25u8, 50u8, 75u8] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{threshold}%")),
            &threshold,
            |b, &t| {
                b.iter(|| {
                    let mut page = Page::new(99);
                    page.set_dead_tuple_compact_percent(t);
                    let payload = vec![9u8; 256];

                    for i in 0..500usize {
                        let key = format!("k-{i:04}").into_bytes();
                        if page.put(black_box(&key), black_box(&payload)).is_err() {
                            break;
                        }
                    }

                    for i in 0..350usize {
                        let key = format!("k-{i:04}").into_bytes();
                        let out = page.remove(black_box(&key));
                        black_box(out);
                    }

                    let refill_val = vec![3u8; 128];
                    let mut inserted = 0usize;
                    for i in 0..500usize {
                        let key = format!("r-{i:04}").into_bytes();
                        if page.put(black_box(&key), black_box(&refill_val)).is_err() {
                            break;
                        }
                        inserted += 1;
                    }

                    black_box((inserted, page.free_space_bytes(), page.dead_tuple_bytes()));
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    page_benches,
    bench_put,
    bench_get,
    bench_remove,
    bench_compaction_thresholds,
    bench_mixed_random_churn
);
criterion_main!(page_benches);

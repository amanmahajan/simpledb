# Benchmarks

Benchmarks are written with [Criterion](https://bheisler.github.io/criterion.rs/book/) and live in `benches/page_bench.rs`. They measure the `Page` layer directly.

## Running benchmarks

```bash
cargo bench
```

HTML reports are written to `target/criterion/`.

## Benchmark groups

### `page_put`

Measures how many key-value pairs can be inserted into a single fresh page before it fills up. Runs for value sizes of 16, 64, and 256 bytes.

```
page_put/16    — small values, many records per page
page_put/64    — moderate values
page_put/256   — larger values, fewer records per page
```

### `page_get`

Pre-fills a page with 32, 128, or 256 records, then measures lookup latency for both a key that exists (hit) and a key that does not (miss).

```
page_get/hit/32    page_get/miss/32
page_get/hit/128   page_get/miss/128
page_get/hit/256   page_get/miss/256
```

Both cases exercise the binary search over the slot array. Miss searches terminate at an insertion point without finding a tuple.

### `page_remove`

Inserts `n` records, then removes all of them in order. Measures the cost of tombstoning and slot-array compaction under sustained deletes.

```
page_remove/32
page_remove/128
page_remove/256
```

### `page_compaction_threshold`

Tests the effect of the dead-tuple compaction threshold on a write-heavy workload:

1. Insert up to 500 records (256-byte values).
2. Delete 350 of them.
3. Refill with new records (128-byte values) and count how many fit.

Runs at thresholds of 25%, 50%, and 75%.

```
page_compaction_threshold/25%
page_compaction_threshold/50%
page_compaction_threshold/75%
```

A lower threshold means more frequent compaction, recovering space sooner at the cost of more rebuild work per operation.

### `page_mixed_random_churn`

A realistic workload benchmark:

1. Insert 200 records with 8-byte values.
2. Perform 100,000 random updates.
3. Perform 100,000 random deletes.
4. Perform 100,000 random gets and count hits.

Compaction threshold is set to 50%. This benchmark captures amortized compaction costs under churn.

```
page_mixed_random_churn/insert200_update100k_delete100k_get100k
```

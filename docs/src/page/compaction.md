# Dead-Tuple Compaction

Deleting or overwriting a tuple does not free its bytes immediately. The old tuple is tombstoned in-place and its size is added to a `dead_bytes` counter in the page header. Space is reclaimed lazily by a compaction pass.

## Why lazy compaction?

An immediate in-place shift of all subsequent tuple bytes would be O(page_size) and would invalidate every slot offset. Lazy compaction amortizes this cost: many tombstones can accumulate and be cleared in a single pass.

## Threshold check

```rust
fn should_compact_dead_tuples(&self) -> bool {
    let used = self.tuple_region_used_bytes(); // PAGE_SIZE - free_end
    if used == 0 {
        return false;
    }
    (self.dead_tuple_bytes() as usize) * 100
        >= used * self.dead_tuple_compact_percent as usize
}
```

`dead_tuple_compact_percent` defaults to **75** — compaction fires when dead bytes account for 75% or more of the total tuple region. It can be tuned per page:

```rust
page.set_dead_tuple_compact_percent(50); // compact at 50%
```

## Compaction algorithm

`compact_live_tuples` rebuilds the page from scratch, writing only live tuples:

```rust
fn compact_live_tuples(&mut self) -> Result<(), &'static str> {
    let mut live = Vec::with_capacity(self.slot_count() as usize);

    for i in 0..self.slot_count() as usize {
        let off = self.read_slot(i) as usize;
        if self.read_tuple_tombstone(off) == 1 {
            continue;
        }
        live.push((
            self.read_key(off).to_vec(),
            self.read_tuple_val(off).to_vec(),
        ));
    }

    let mut compacted = Page::new(self.page_id());
    compacted.set_dead_tuple_compact_percent(self.dead_tuple_compact_percent);
    for (k, v) in live {
        compacted.put(&k, &v)?;
    }

    self.data = compacted.data; // swap the buffer
    Ok(())
}
```

After compaction `dead_bytes = 0` and `free_end` is restored to reflect only the live tuples.

## When compaction is triggered

`maybe_compact_dead_tuples` is called:

- After every `remove`.
- After every overwrite in `put` (before and after writing the new tuple, to ensure there is room).

```rust
fn maybe_compact_dead_tuples(&mut self) -> Result<(), &'static str> {
    if self.should_compact_dead_tuples() {
        self.compact_live_tuples()?;
    }
    Ok(())
}
```

## Effect on free space

After compaction the page behaves as if the tombstoned tuples never existed. All live tuples are repacked from the end of the page with no gaps, maximising usable free space.

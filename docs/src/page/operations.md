# Page Operations

All key-value operations go through a binary search on the sorted slot array. The `find_slot` helper returns either the slot index where the key was found or the index where it should be inserted.

## Binary search — `find_slot`

```rust
enum SearchResult {
    Found(usize),     // index of the existing slot
    NotFound(usize),  // index where a new slot should be inserted
}

fn find_slot(&self, key: &[u8]) -> SearchResult {
    let search_key = Key::from(key);
    let mut lo = 0usize;
    let mut hi = self.slot_count() as usize;

    while lo < hi {
        let mid = (lo + hi) / 2;
        let tuple_off = self.read_slot(mid) as usize;
        let mid_key = Tuple::from(&self.data[tuple_off..]).key();

        match mid_key.cmp(&search_key) {
            Ordering::Equal   => return SearchResult::Found(mid),
            Ordering::Less    => lo = mid + 1,
            Ordering::Greater => hi = mid,
        }
    }
    SearchResult::NotFound(lo)
}
```

Keys are compared as raw byte slices in lexicographic order via `Key<D>`'s `Ord` implementation.

## `get`

Looks up the key. Returns `None` if the key is absent or its tuple is tombstoned.

```rust
pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
    match self.find_slot(key) {
        SearchResult::Found(i) => {
            let off = self.read_slot(i) as usize;
            if self.read_tuple_tombstone(off) == 1 {
                None
            } else {
                Some(self.read_tuple_val(off))
            }
        }
        SearchResult::NotFound(_) => None,
    }
}
```

The returned slice borrows directly from the page buffer — no copy.

## `put`

Handles two cases depending on whether the key already exists.

**New key (insert):**

1. Check free space ≥ `tuple_len + SLOT_SIZE`; compact dead tuples if needed.
2. `alloc_tuple(len)` — reserve bytes at the end of the tuple region.
3. `write_tuple(off, ...)` — serialize the tombstone flag, lengths, key, and value.
4. `alloc_slot()` — increment `slot_count` and advance `free_start`.
5. Shift all slots at positions ≥ `pos` right by one.
6. Write the new slot at position `pos`.

**Existing key (overwrite):**

1. Check free space ≥ `tuple_len`; compact if needed.
2. Read the old tuple bytes (to return as the old value).
3. Tombstone the old tuple (`data[old_off] = 1`) and account for its dead bytes.
4. Allocate and write the new tuple.
5. Update the existing slot to point to the new tuple offset.
6. Trigger compaction if the threshold is crossed.

```rust
pub fn put(&mut self, key: &[u8], val: &[u8])
    -> Result<Option<Vec<u8>>, &'static str>
```

Returns `Ok(None)` on insert, `Ok(Some(old_val))` on overwrite, `Err("page full")` when there is no space even after compaction.

## `remove`

1. Tombstones the tuple at the found slot.
2. Shifts all slots after position `i` left by one (closing the gap).
3. Decrements `slot_count` and retreats `free_start`.
4. Triggers compaction if the threshold is crossed.

```rust
pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>>
```

Returns the old value, or `None` if the key was not found.

## `get_key_value_at_slot`

Iterates slots by index (useful for scanning all entries):

```rust
pub fn get_key_value_at_slot(&self, slot_idx: usize) -> Option<(&[u8], &[u8])>
```

Returns `None` for out-of-bounds indices or tombstoned tuples. Because slots are kept sorted, iterating slots 0…n gives keys in ascending order.

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

```rust
pub fn put(&mut self, key: &[u8], val: &[u8])
    -> Result<Option<Vec<u8>>, &'static str>
```

Returns `Ok(None)` on insert, `Ok(Some(old_val))` on overwrite, `Err("page full")` when there is no space even after compaction.

`put` has two completely different code paths depending on whether `find_slot` returns `Found` or `NotFound`.

---

### Case 1 — New key (insert)

#### What needs to fit

A new key needs room for **two things simultaneously**:

```
free space needed = tuple_bytes + SLOT_SIZE (2 bytes)
                    └─ the actual data ─┘   └─ the new pointer entry ─┘
```

The slot array grows from the left; the tuple region grows from the right. Both eat from the same free gap in the middle. If `free_space_bytes < tuple_len + 2`, the page is full.

#### Page state before insert (2 slots, keys "bob" and "eve")

```
byte 0                                                        byte 8191
┌────────────────────┬──────────┬──────────┬───────┬─────────────────────┐
│ Header (20 B)      │slot[0]=… │slot[1]=… │ free  │  …[eve]…  [bob]…   │
│ slot_count=2       │ (2 B)    │ (2 B)    │       │                     │
│ free_start=24      │          │          │       │                     │
│ free_end=8120      │          │          │       │                     │
└────────────────────┴──────────┴──────────┴───────┴─────────────────────┘
                     ▲                     ▲       ▲
                  byte 20               byte 24  byte 8120
                  (= HEADER_SIZE)       free_start  free_end
```

#### Inserting key "carol"

`find_slot("carol")` returns `NotFound(1)` — it belongs between slot[0]("bob") and slot[1]("eve").

**Step 1 — `alloc_tuple`**: subtract tuple length from `free_end`, return new offset.

```
free_end was 8120
tuple for "carol" = 1 + 2 + 2 + 5 + val_len bytes
new free_end      = 8120 - tuple_len
```

The tuple bytes are written at the new `free_end` offset:

```
┌────────────────────┬────────┬────────┬───────┬──────────────────────────────┐
│ Header             │slot[0] │slot[1] │ free  │  [carol tuple][eve][bob]     │
│ free_end updated   │        │        │       │  ▲                           │
└────────────────────┴────────┴────────┴───────┴──┼───────────────────────────┘
                                                   new free_end
```

**Step 2 — `alloc_slot`**: increment `slot_count` from 2 → 3, advance `free_start` from 24 → 26.

```
┌────────────────────┬────────┬────────┬────────┬──────┬───────────────────────┐
│ Header             │slot[0] │slot[1] │slot[2] │ free │  [carol][eve][bob]    │
│ slot_count=3       │        │        │(empty) │      │                       │
│ free_start=26      │        │        │        │      │                       │
└────────────────────┴────────┴────────┴────────┴──────┴───────────────────────┘
```

**Step 3 — shift slots right**: slots at positions ≥ 1 (the insertion point) shift one position to the right to open the gap.

```
Before shift:  slot[0]="bob"  slot[1]="eve"  slot[2]=empty
After shift:   slot[0]="bob"  slot[1]=empty  slot[2]="eve"
```

**Step 4 — write slot**: write the new tuple's offset into `slot[1]`.

```
slot[0] → bob tuple offset
slot[1] → carol tuple offset   ← new
slot[2] → eve tuple offset
```

Final state — slots are sorted, tuple region has all three tuples packed from the right:

```
┌────────────────────┬────────┬────────┬────────┬──────┬──────────────────────┐
│ Header             │slot[0] │slot[1] │slot[2] │ free │ [carol]  [eve] [bob] │
│ slot_count=3       │→bob    │→carol  │→eve    │      │                      │
│ free_start=26      │        │        │        │      │                      │
└────────────────────┴────────┴────────┴────────┴──────┴──────────────────────┘
```

> The tuples are **not sorted** in the tuple region — only the **slot array** is sorted. Binary search on slots is what makes lookups O(log n), not physical ordering of tuple bytes.

---

### Case 2 — Existing key (overwrite)

This is more subtle. The old tuple cannot be resized in place because tuples are variable-length and packed tightly. Instead:

1. The old tuple is **tombstoned** (its first byte is set to `1`).
2. A brand-new tuple is allocated at the current `free_end`.
3. The existing slot is updated to point to the new offset.

The old bytes remain on the page as dead space until compaction.

#### Why not just edit the old tuple in place?

If the new value is a different size, the tuple would overlap its neighbour. Slotted pages avoid this by always appending new tuples at the free end — the slot redirects the pointer, so no other slot is affected.

#### Page state before overwrite (key "bob", value "old")

```
┌────────────┬────────┬───────┬────────────────────────────────────────┐
│ Header     │slot[0] │ free  │          [bob|"old"]                   │
│ dead=0     │→7900   │       │          ▲ offset 7900                 │
└────────────┴────────┴───────┴──────────┼───────────────────────────-─┘
                                         tombstone=0 here
```

#### Overwriting "bob" with value "new-longer-value"

**Step 1 — tombstone the old tuple**: set `data[7900] = 1`, add old tuple's total length to `dead_bytes` counter.

```
offset 7900: tombstone byte set to 1
dead_bytes counter += old_tuple_total_len
```

**Step 2 — `alloc_tuple`**: carve out space for the new tuple at the new `free_end`.

```
new tuple written at (free_end - new_tuple_len)
```

**Step 3 — update slot**: `slot[0]` now points to the new offset instead of 7900.

```
┌────────────┬────────┬───────┬──────────────────────────────────────────────┐
│ Header     │slot[0] │ free  │  [bob|"new-longer-value"]  [☠ bob|"old"]    │
│ dead=N     │→7860   │       │  ▲ new offset              ▲ dead, offset 7900│
└────────────┴────────┴───────┴──┼──────────────────────────────────────────-┘
                                  7860
```

The old bytes at 7900 are now unreachable from any slot — they are invisible to `get` — but they still physically occupy space. `dead_bytes` tracks this. When `dead_bytes / tuple_region_used_bytes >= dead_tuple_compact_percent`, compaction fires and sweeps them out.

#### Why check free space before compaction, then again after?

```rust
if self.free_space_bytes() < needed {
    self.maybe_compact_dead_tuples()?;   // try to reclaim dead space
    if self.free_space_bytes() < needed { // check again — compaction may not have helped
        return Err("page full");
    }
}
```

Compaction only helps if there are enough dead bytes to reclaim. If the page is genuinely full of live data, compaction does nothing and the second check correctly returns `"page full"`.

## `remove`

```rust
pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>>
```

Returns the old value, or `None` if the key was not found.

`remove` does **not** free the tuple's bytes immediately. It tombstones the tuple in place and closes the gap in the slot array. The dead bytes are reclaimed later by compaction.

---

### What remove must do

Two separate data structures need to be updated:

1. **The tuple** (in the tuple region) — marked dead so `get` skips it.
2. **The slot array** (near the header) — the slot pointing to this tuple is physically removed and the array is closed up, keeping it gap-free and sorted.

---

### Starting state — 3 slots, keys "bob", "carol", "eve"

```
byte 0                                                              byte 8191
┌────────────┬────────┬────────┬────────┬──────┬──────────────────────────────┐
│ Header     │slot[0] │slot[1] │slot[2] │ free │  [carol]      [eve]   [bob]  │
│ slot_count=3│→bob   │→carol  │→eve    │      │  ▲ off=7950   ▲7970   ▲7990  │
│ free_start=26│      │        │        │      │                               │
│ free_end=7950│      │        │        │      │                               │
│ dead=0     │        │        │        │      │                               │
└────────────┴────────┴────────┴────────┴──────┴───────────────────────────────┘
```

### Removing key "carol" (slot index 1)

`find_slot("carol")` returns `Found(1)`.

---

**Step 1 — capture the old value**

Before touching anything, read the value bytes from the tuple at `slot[1]`'s offset and copy them into a `Vec<u8>`. This is what the function returns to the caller.

```
old_val = read_tuple_val(7950).to_vec()   // copy made here
```

---

**Step 2 — tombstone the tuple**

Set the first byte at offset 7950 to `1`. The tuple bytes still sit in the tuple region but are now invisible to any future `get` or scan.

```
data[7950] = 1    // tombstone flag

Before: [0x00 | key_len | val_len | "carol" | value...]   tombstone=0 (live)
After:  [0x01 | key_len | val_len | "carol" | value...]   tombstone=1 (dead)
```

Add the tuple's total byte length to the `dead_bytes` counter in the header:

```
dead_bytes += 1 + 2 + 2 + key_len + val_len
           = total size of the carol tuple
```

The page now looks like this — slot[1] still points to the dead tuple, but that is about to be fixed:

```
┌────────────┬────────┬────────┬────────┬──────┬──────────────────────────────┐
│ Header     │slot[0] │slot[1] │slot[2] │ free │  [☠carol]     [eve]   [bob]  │
│ dead=N     │→bob    │→carol  │→eve    │      │  tombstone=1                  │
└────────────┴────────┴────────┴────────┴──────┴───────────────────────────────┘
```

---

**Step 3 — shift slots left**

Every slot after position 1 shifts one position to the left, overwriting the removed slot and closing the gap.

```
Before:  slot[0]→bob  slot[1]→carol  slot[2]→eve
                            ↑ removing this
After:   slot[0]→bob  slot[1]→eve    slot[2]→(stale, will be ignored)
```

The loop runs: `for j in 2..3 { write_slot(j-1, read_slot(j)) }`, which copies slot[2] into slot[1].

---

**Step 4 — update header counters**

```
slot_count: 3 → 2
free_start: 26 → 24   (= HEADER_SIZE + slot_count * SLOT_SIZE = 20 + 2*2)
```

`free_start` retreats by 2 bytes — the slot array is now one entry shorter, giving back 2 bytes to the free gap. The stale value in the old slot[2] position is now inside the free region and will be overwritten by the next insert.

Final state:

```
byte 0                                                              byte 8191
┌────────────┬────────┬────────┬──────────┬──────────────────────────────────┐
│ Header     │slot[0] │slot[1] │   free   │  [☠carol]     [eve]   [bob]      │
│ slot_count=2│→bob   │→eve    │          │  ▲ dead bytes here                │
│ free_start=24│      │        │          │                                   │
│ dead=N     │        │        │          │                                   │
└────────────┴────────┴────────┴──────────┴───────────────────────────────────┘
```

---

### What does NOT happen

The carol tuple bytes are **not moved or erased**. The tuple region still physically contains:

```
[☠carol bytes][eve bytes][bob bytes]
```

The free region did not grow — `free_end` is unchanged. The only space recovered immediately is the 2-byte slot. The carol tuple's bytes remain as dead weight until compaction fires.

---

### Asymmetry with `put`

| | `put` (new key) | `remove` |
|---|---|---|
| Slot array | grows by 1 (shift right, write) | shrinks by 1 (shift left) |
| `free_start` | advances by 2 | retreats by 2 |
| `free_end` | retreats by tuple size | **unchanged** |
| Tuple bytes | written at new `free_end` | tombstoned in place |
| Space recovered | n/a | only 2 bytes (the slot), rest waits for compaction |

This asymmetry is why the page can accumulate dead bytes: inserts move `free_end` in but removes never move it back out.

## `get_key_value_at_slot`

Iterates slots by index (useful for scanning all entries):

```rust
pub fn get_key_value_at_slot(&self, slot_idx: usize) -> Option<(&[u8], &[u8])>
```

Returns `None` for out-of-bounds indices or tombstoned tuples. Because slots are kept sorted, iterating slots 0…n gives keys in ascending order.

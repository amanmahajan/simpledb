# Page Layout

A `Page` is a fixed-size 8 KB buffer (`PAGE_SIZE = 8192`). Every page starts with a 20-byte header, followed by a slot array that grows toward higher addresses, and a tuple region that grows toward lower addresses from the end of the page.

```
Byte 0                                              Byte 8191
┌──────────────┬────────────────────┬──────┬───────────────────────┐
│  Header      │   Slot array       │ free │  Tuple region         │
│  (20 bytes)  │  (2 bytes/slot)    │      │  (variable-length     │
│              │  ──────────────►   │      │   tuples)  ◄────────  │
└──────────────┴────────────────────┴──────┴───────────────────────┘
               ▲                    ▲      ▲
           free_start begins    free_start free_end
           at HEADER_SIZE       advances   retreats
```

## Header fields

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `magic` | Sentinel `0x504E5554` ("PNUT") — identifies valid pages |
| 4 | 4 | `page_id` | Unique page identifier |
| 8 | 2 | `slot_count` | Number of live slots |
| 10 | 2 | `free_start` | Byte offset right after the slot array |
| 12 | 2 | `free_end` | Byte offset right before the tuple region |
| 14 | 2 | `dead_bytes` | Bytes occupied by tombstoned tuples |
| 16 | 4 | `next_leaf_page_id` | Sibling pointer for the leaf linked list (`0` = none) |

`free_space_bytes = free_end - free_start` is the usable gap between the slot array and the tuple region.

## Initializing a page

```rust
pub fn new(page_id: u32) -> Self {
    let mut p = Page {
        data: [0u8; PAGE_SIZE],
        dead_tuple_compact_percent: Self::DEFAULT_DEAD_TUPLE_COMPACT_PERCENT,
    };

    write_u32(&mut p.data, HDR_MAGIC_OFF, PAGE_MAGIC);       // 0x504E5554
    write_u32(&mut p.data, HDR_PAGE_ID_OFF, page_id);
    write_u16(&mut p.data, HDR_SLOT_CNT_OFF, 0);
    write_u16(&mut p.data, HDR_FREE_START_OFF, HEADER_SIZE as u16); // 20
    write_u16(&mut p.data, HDR_FREE_END_OFF, PAGE_SIZE as u16);     // 8192
    write_u16(&mut p.data, HDR_DEAD_BYTES_OFF, 0);
    write_u32(&mut p.data, HDR_NEXT_LEAF_PAGE_ID_OFF, 0);
    p
}
```

On a fresh page `free_start = 20` and `free_end = 8192`, giving 8172 bytes of free space.

## Slot array

Each slot is a `u16` little-endian byte offset pointing to a tuple in the tuple region. Slots are stored in **sorted key order** — binary search is used for all lookups. Slot `i` lives at byte `20 + i * 2` in the page.

When a new key is inserted, all slots at positions ≥ the insertion point are shifted right by one slot to open a gap, then the new slot is written.

## Tuple region

Tuples are written from the end of the page toward the header. `alloc_tuple(len)` subtracts `len` from `free_end` and returns the new offset. Each tuple is self-describing:

```
┌──────────┬─────────┬─────────┬──────────┬──────────┐
│ tombstone│ key_len │ val_len │   key    │   value  │
│  (1 B)   │ (2 B LE)│ (2 B LE)│          │          │
└──────────┴─────────┴─────────┴──────────┴──────────┘
 offset+0   offset+1  offset+3  offset+5   offset+5+key_len
```

Total tuple size = `1 + 2 + 2 + key_len + val_len`.

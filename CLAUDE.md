# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build               # compile
cargo test                # run all tests
cargo test <test_name>    # run a single test by name
cargo test -p simpledb -- btree::leaf_test  # run tests in a specific module
cargo bench               # run Criterion benchmarks (outputs HTML to target/criterion/)
cargo clippy              # lint
```

## Architecture

This is a Rust 2024 database internals project ("Peanut DB"). The layers from bottom to top:

**Page** (`src/page/page.rs`) — the fundamental storage unit. An 8KB slotted page with:
- A 20-byte header (magic `0x504E5554`, page_id, slot_count, free_start, free_end, dead_bytes, next_leaf_page_id)
- A slot array (sorted `u16` offsets, grows down from the header)
- A tuple region (variable-length records, grows up from the end)
- Tuple format: `tombstone(u8) | key_len(u16 LE) | val_len(u16 LE) | key | val`
- Dead-tuple compaction triggered when dead bytes exceed a configurable % threshold (default 75%)
- Core API: `put`, `get`, `remove` — all binary-search through the sorted slot array

**Btree types** (`src/btree/`):
- `key.rs` — `Key<D>` newtype with lexicographic `Ord` via `AsRef<[u8]>`
- `tuple.rs` — `Tuple<A>` zero-copy view over the same wire format as Page tuples; `TupleBuilder` constructs owned tuples
- `leaf.rs` — `LeafPage` (owned) and `LeafPageMut<'a>` (borrowed) wrappers around `Page`; `insert_or_split` handles leaf-level splits and maintains sibling pointer chains via `next_leaf_page_id`
- `internal.rs` — `InternalPage` wraps `Page`; stores separator keys with right-child `u32` page IDs as values; leftmost child pointer reuses `next_leaf_page_id` header field; `find_child` does binary search to route lookups
- `tree.rs` — `BTree` struct (stub: holds `root_page_id` + `Pager`, no methods implemented yet)

**Pager** (`src/pager/pager.rs`) — pure in-memory `HashMap<u32, Page>` with auto-increment IDs. No disk I/O yet.

## Key conventions

- All multi-byte integers are little-endian.
- `zerocopy` v0.8: use associated-function form `Ref::into_ref(r)`, not method form `r.into_ref()`.
- `InternalPage` repurposes the `next_leaf_page_id` header field for the leftmost child pointer — internal nodes have no leaf siblings to link.
- Compaction rebuilds the page from scratch (`Page::new` + re-insert live tuples) rather than in-place defragmentation.

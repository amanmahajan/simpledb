# Roadmap

`simpledb` is a work-in-progress. The storage primitives are solid; the higher-level B-tree and durability layers are next.

## B-tree operations

The storage primitives are complete:

- ✅ `LeafPage` / `LeafPageMut` with `insert_or_split` (copy-up)
- ✅ `InternalPage` / `InternalPageMut` with `insert_or_split` (push-up)

`btree/tree.rs` holds only a stub:

```rust
pub struct BTree {
    root_page_id: u32,
    pager: Pager,
}
```

What needs to be added:

- `BTree::insert` — walk the tree, find the correct leaf, call `insert_or_split`, and propagate the separator key up if a split occurred.
- `BTree::get` — traverse internal nodes to reach the right leaf.
- `BTree::remove` — delete from a leaf; handle underflow (merge or redistribute).
- `BTree::range_scan` — start at the left-most matching leaf and follow sibling pointers.

## Disk I/O

The `Pager` currently stores pages in a `HashMap`. To make the database persistent:

- Replace the `HashMap` with a buffer pool that maps page IDs to positions in a file.
- Implement page eviction (e.g. clock or LRU) to bound memory usage.
- Read 8 KB pages from disk on cache miss; write dirty pages on eviction or checkpoint.

## Write-ahead log (WAL)

For crash recovery:

- Log every modification before applying it to the page.
- On startup, replay the log to bring pages to a consistent state.
- Implement checkpointing to truncate the log.

## Concurrency

- Page-level latches (read/write locks) for multi-reader, single-writer access.
- B-tree latch-coupling (crabbing) for concurrent tree traversal.

## Query layer

- A simple SQL-like parser and planner on top of the B-tree.
- Support for table schemas, typed columns, and basic predicates.

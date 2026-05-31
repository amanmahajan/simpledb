# Roadmap

`simpledb` is a work-in-progress. The storage and B-tree layers are complete; durability and concurrency are next.

## B-tree operations

As of the `BTree` milestone, all core tree operations are complete.

- ✅ `LeafPage` / `LeafPageMut` with `insert_or_split` (copy-up)
- ✅ `InternalPage` / `InternalPageMut` with `insert_or_split` (push-up)
- ✅ `BTree::insert` — traverses internal nodes, inserts at the correct leaf, cascades splits up to a new root when needed
- ✅ `BTree::get` — traverses internal nodes to reach the right leaf and returns the value
- ✅ `BTree::remove` — deletes from the leaf (no rebalancing yet — see below)
- ✅ `BTree::scan` — lazy `Scan` iterator that starts at a given key and follows the leaf sibling chain

Remaining B-tree work:

- `BTree::remove` rebalancing — when a leaf drops below half-full after a delete, merge it with a sibling or redistribute entries. Currently leaves are left sparse.

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

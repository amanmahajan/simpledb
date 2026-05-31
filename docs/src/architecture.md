# Architecture Overview

## Module dependency graph

```
┌──────────────────────────────────────┐
│               BTree                  │  (btree/tree.rs)
│  root_page_id: u32                   │
│  height: u32                         │
│  pager: Pager                        │
│  insert / get / remove / scan        │
└────────────────┬─────────────────────┘
                 │ uses
                 ▼
┌──────────────────────────────────────┐
│    LeafPage / LeafPageMut            │  (btree/leaf.rs)
│  insert_or_split                     │
│  entries / get / remove              │
└────────────────┬─────────────────────┘
                 │ wraps
                 ▼
┌──────────────────────────────────────┐
│               Page                   │  (page/page.rs)
│  put / get / remove                  │
│  slot array + tuple region           │
│  dead-tuple compaction               │
└──────────────────────────────────────┘

┌──────────────────────────────────────┐
│              Pager                   │  (pager/pager.rs)
│  HashMap<u32, Page>                  │
│  alloc_page_id / new_page            │
└──────────────────────────────────────┘

┌──────────────────────────────────────┐
│         Tuple / TupleBuilder         │  (btree/tuple.rs)
│         Key<D>                       │  (btree/key.rs)
│         byte utils                   │  (utils/byte.rs)
└──────────────────────────────────────┘
```

## Data flow: inserting a key-value pair

```
caller
  │
  │  BTree::insert(key, val)
  ▼
BTree
  │  find_leaf(key)          ← descend height-1 internal levels
  │  alloc_page_id()         ← reserve ID for potential right page
  ▼
LeafPageMut::insert_or_split(key, val, new_page_id)
  │  Page::put(key, val)
  ▼
Page
  │  find_slot(key)          ← binary search on slot array
  │  alloc_tuple(len)        ← claim bytes from tuple region
  │  write_tuple(off, ...)   ← serialize tombstone+key_len+val_len+key+val
  │  alloc_slot / shift      ← insert u16 offset into sorted slot array
  ▼
  [page bytes updated in-place]

if insert fit → Ok(None) → BTree::insert returns Ok(())

if "page full"
  ▼
LeafPageMut::insert_or_split
  │  collect all entries + new entry, sort
  │  split at midpoint → left page (in-place), right page (new)
  │  wire sibling pointers (left.next = right, right.next = old_next)
  ▼
  LeafSplit { separator_key, right_page }
  │
  ▼
BTree::propagate_split(path, sep_key, right_child)
  │  for each internal node on path (bottom-up):
  │    InternalPageMut::insert_or_split(sep, child, new_id)
  │    if no split → done
  │    if split → push separator to grandparent
  │  if root split → create new root, height += 1
  ▼
  Ok(())
```

## Key design principles

**Fixed-size pages.** Every page is exactly 8 KB, matching typical OS and SSD page sizes. This makes future disk I/O straightforward.

**Sorted slot array.** Slots are `u16` byte offsets kept in key order. Binary search over slots gives O(log n) lookup without a separate index structure.

**Tombstone deletes.** Deleted and overwritten tuples are marked with a flag byte instead of being removed immediately. Space is reclaimed lazily by a compaction pass.

**Zero-copy tuple views.** `Tuple<A>` is generic over any `AsRef<[u8]>`, so tuple data can be read directly from page memory without copying.

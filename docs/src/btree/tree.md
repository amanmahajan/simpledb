# The B-Tree

## What problem does it solve?

You have hundreds of thousands of key-value pairs. You need to find any one of them in microseconds. You need to insert new ones without rebuilding everything. You need to scan a range of them in sorted order.

A flat array of key-value pairs lets you scan cheaply but requires rewriting half the array on every insert. A hash table gives you O(1) lookup but destroys sorted order. A B+ tree threads the needle: O(log n) insert and lookup, *and* O(k) range scans, where k is the number of results.

`BTree` in `btree/tree.rs` is the top of the stack. It owns the `Pager`, knows the root, and ties everything together — leaf inserts, split cascades, sibling-chain scans.

---

## Think of a multi-storey filing room

Imagine a library with physical file drawers. The drawers themselves (leaf pages) hold the actual folders. On the ground floor there's a wall of index cards (an internal page) that says *"surnames A–M: go to room 3; N–Z: go to room 7."* If you have many rooms, you might need a floor directory that points to the right index-card wall.

The `BTree` is the librarian who knows how many floors there are, which floor is the index and which floor holds the actual folders, and who handles what happens when a drawer is full and needs a second one.

---

## The struct

```rust
pub struct BTree {
    root_page_id: u32,
    height: u32,
    pager: Pager,
}
```

Three fields. That's it.

**`root_page_id`** is the page ID of the topmost node. Everything starts here. When the tree is empty it's `0` (a sentinel — page IDs start at 1).

**`height`** describes the tree's shape:

| `height` | Meaning |
|----------|---------|
| `0` | Empty — no pages allocated yet |
| `1` | Root is a leaf page; no internal nodes at all |
| `2` | Root is an internal page; its children are leaf pages |
| `3` | Root → internal → internal → leaves |
| … | Each level adds one layer of internal routing |

Knowing `height` is critical. It tells `find_leaf` exactly how many internal levels to traverse before it reaches a leaf. Without it you'd need to store a "is this page a leaf?" flag inside every page header.

**`pager`** owns every page in existence. All 8KB of them live in a `HashMap<u32, Page>`. The B-tree borrows pages from the pager, modifies them, and stores new ones.

---

## Inserting a key-value pair

### The happy path (no split)

You call `tree.insert(b"alice", b"data")`. Here is what happens, step by step:

1. If the tree is empty (`height == 0`), create a fresh leaf page in the pager, record it as the root, set `height = 1`.

2. Call `find_leaf("alice")` to walk down to the right leaf page. For a freshly bootstrapped tree this returns the root page immediately.

3. Ask the leaf to insert the entry. The page has room → insert succeeds → done.

```
After inserting "alice", "bob", "carol" into a fresh tree (height=1):

root_page_id = 1
height = 1

Page 1 (leaf, also the root)
┌─────────────────────────┐
│ alice → data_a          │
│ bob   → data_b          │
│ carol → data_c          │
└─────────────────────────┘
next_leaf = None
```

### When the leaf is full — the first split

Eventually you fill the leaf. The next insert causes `insert_or_split` to return a `LeafSplit` (see [Leaf Splits](split.md)).

The split gives you two things: a `separator_key` and a new `right_page`. You must now store the right page in the pager and record the separator somewhere in an internal node. If no parent exists (height was 1, root was a leaf), you create a brand-new internal root and the tree grows one level.

```
Before (inserting "dave" into a full leaf):

root_page_id = 1, height = 1

Page 1 (leaf, full)
┌─────────────────────────┐
│ alice → data_a          │
│ bob   → data_b          │  ← "dave" won't fit
│ carol → data_c          │
│ eve   → data_e          │
└─────────────────────────┘


After (leaf splits at "carol"; new internal root created):

root_page_id = 3, height = 2

Page 3 (new internal root)
leftmost=1, [carol→2]
      │           │
      ▼           ▼
Page 1 (leaf)   Page 2 (new leaf)
┌──────────┐    ┌──────────────┐
│ alice    │    │ carol        │
│ bob      │    │ dave         │──► None
└──────────┘    │ eve          │
    ──►Page 2   └──────────────┘
```

The separator `"carol"` lives in the new root. The old root page (1) keeps its left half. The new page (2) gets the right half. Height becomes 2.

---

## `find_leaf` — navigating to the right page

```rust
fn find_leaf(&self, key: &[u8]) -> (u32, Vec<u32>) {
    let mut path = Vec::new();
    let mut current = self.root_page_id;
    for _ in 1..self.height {       // iterate once per internal level
        path.push(current);
        let page = self.pager.get(current).unwrap().clone();
        let node = InternalPage::from_page(page);
        current = node.find_child(key);
    }
    (current, path)
}
```

`for _ in 1..self.height` runs exactly `height - 1` times — once per internal level. When `height == 1`, the loop is empty and `current` stays at the root (which is already the leaf). When `height == 3` the loop runs twice, descending through two levels of internal nodes.

The `path` vector records every internal page visited, from root to the immediate parent of the leaf. This path is exactly what `propagate_split` needs to walk back up if a split occurs.

---

## `propagate_split` — the cascade

This is the heart of insert. When a leaf splits, its parent internal node must record the new separator. But the parent might be full too. And *its* parent might be full. The cascade climbs until it finds a node with room, or creates a new root.

```rust
fn propagate_split(
    &mut self,
    mut path: Vec<u32>,   // internal nodes from root down to leaf's parent
    mut sep_key: Vec<u8>,
    mut right_child: u32,
) -> Result<(), &'static str>
```

The loop pops from the back of `path` — bottom up, from the leaf's parent toward the root:

```rust
while let Some(parent_id) = path.pop() {
    let new_right_id = self.pager.alloc_page_id();
    let split = {
        let node = InternalPageMut::new(self.pager.get_mut(parent_id).unwrap());
        node.insert_or_split(&sep_key, right_child, new_right_id)?
    };
    match split {
        None => return Ok(()),          // separator fit, cascade stops here
        Some(s) => {                    // parent also split, keep climbing
            self.pager.insert_page(s.right_page.into_page());
            sep_key = s.separator_key;
            right_child = new_right_id;
        }
    }
}
// Path is exhausted → root itself split
```

If the loop empties `path` without stopping, the root page has split. Now you need a new root:

```rust
let new_root_id = self.pager.new_page();
let page = self.pager.get_mut(new_root_id).unwrap();
let mut root = InternalPageMut::new(page);
root.set_leftmost_child(old_root_id);   // left half of old root
root.insert(&sep_key, right_child)?;    // right half of old root
self.root_page_id = new_root_id;
self.height += 1;
```

This is the **only moment the tree ever grows taller** via a split cascade. Inserting a key can never decrease height; only merges can do that (not yet implemented).

### Cascade example with a 3-level tree

```
Initial state (height=3):

      [root: eve→pg5]
       │           │
       ▼           ▼
[bob→pg2]       [henry→pg7]
  │      │          │      │
pg1    pg2        pg6    pg7
[a,b] [c,d,e]  [e,f,g] [h,i,j]  ← all leaves full

Insert "carol":
  Step 1: leaf pg2 splits → separator="d", right=pg8
  Step 2: parent [bob→pg2] gets "d"→pg8 → that page is ALSO full → splits
          separator="d" pushed to root
  Step 3: root gets "d"→new_internal → root is full → root splits
          new root created, height → 4
```

In practice this cascade is rare. An 8KB page holds hundreds of small entries or ~5 of 1500-byte values. Internal nodes fill far slower than leaves.

---

## `get` — finding a value

```rust
pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
    if self.height == 0 {
        return None;
    }
    let (leaf_id, _) = self.find_leaf(key);
    self.pager.get(leaf_id)?.get(key)
}
```

Walk to the leaf, ask the page for the key. The returned `&[u8]` borrows directly from the page's memory inside the pager — no copy.

---

## `remove` — deleting a key

```rust
pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
    if self.height == 0 {
        return None;
    }
    let (leaf_id, _) = self.find_leaf(key);
    let page = self.pager.get_mut(leaf_id)?;
    LeafPageMut::new(page).remove(key)
}
```

Walk to the leaf, tombstone the entry, return the old value. The page handles compaction lazily — see [Dead-Tuple Compaction](../page/compaction.md).

**No rebalancing.** When a leaf drops below half-full after a remove, nothing special happens. The page stays sparse. Merging and redistribution are a future milestone.

---

## `scan` — reading a range in sorted order

A range scan is the feature that justifies the B+ tree's entire existence. The idea is elegant:

1. Walk down the tree to find the leaf where the scan *starts*.
2. Read that leaf from the first matching key to the last key on the page.
3. Follow `next_leaf_page_id` to the right sibling. Repeat until done.

You never need to climb back up the tree. The leaf sibling chain is a sorted linked list that covers every key in the database.

```rust
pub fn scan(&self, start: Option<&[u8]>) -> Scan<'_>
```

`start = None` means "scan everything from the beginning." `start = Some(key)` starts at the first key ≥ `key`.

```
Scan from "bob" through a 3-page tree:

Page 1           Page 2           Page 3
┌──────────┐     ┌──────────┐     ┌──────────┐
│ alice    │     │ carol    │ ──► │ henry    │
│ bob      │ ──► │ dave     │     │ iris     │
│           │     │ eve      │     │ zara     │──► None
└──────────┘     └──────────┘     └──────────┘

scan(Some("bob")) starts at page 1, skips "alice",
returns "bob" → then follows next→ page 2, returns "carol", "dave", "eve"
→ follows next → page 3, returns "henry", "iris", "zara" → None, done.
```

### The `Scan` iterator

```rust
pub struct Scan<'a> {
    pager: &'a Pager,
    current_page_id: Option<u32>,
    slot_idx: usize,
    skip_before: Option<Vec<u8>>,
}
```

`Scan` is a lazy iterator — it only reads one slot per call to `next()`. The `'a` lifetime ties it to the `BTree`'s pager, so the key and value slices it hands back are zero-copy references directly into page memory.

`skip_before` carries the start key. On the very first leaf, the iterator checks each key against it and skips keys that come before the start. Once it finds the first key ≥ start, it clears `skip_before` and never checks again — all subsequent leaves are entirely in range.

`current_page_id = None` is the termination sentinel. When a leaf has no right sibling, `next_leaf_page_id()` returns `None`, `current_page_id` becomes `None`, and the next `next()` call returns `None`.

Tombstoned slots — entries deleted after the scan started — are silently skipped because `get_key_value_at_slot` returns `None` for tombstones.

---

## Height evolution over time

```
Start (empty):         height=0, root_page_id=0 (sentinel)

First insert:          height=1
  [root leaf: a]

First leaf split:      height=2
  [root internal: c→]
     │         │
  [a,b]     [c,d,e]

Root internal splits:  height=3
  [new root: …→]
     │             │
  [int1:c→]     [int2:h→]
   │      │      │      │
 [a,b]  [c,d] [e,f,g] [h,i]
```

Height only grows when a split reaches the root. It never shrinks (no merge is implemented yet).

---

## Method reference

### `BTree`

| Method | What it does |
|--------|-------------|
| `new()` | Empty tree — no pages allocated |
| `insert(key, val)` | Insert or update; cascades splits up to a new root if needed |
| `get(key)` | Walk to the leaf, return value bytes or `None` |
| `remove(key)` | Walk to the leaf, tombstone the entry, return old value or `None` |
| `scan(start)` | Return a lazy `Scan` iterator from `start` (or the beginning) |

### `Scan<'a>`

| Method | What it does |
|--------|-------------|
| `next()` | Return the next `(key, value)` pair in sorted order, or `None` |

`Scan` implements `Iterator<Item = (&'a [u8], &'a [u8])>`. Every standard iterator adaptor (`map`, `filter`, `take`, `collect`, …) works on it.

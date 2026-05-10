# Leaf Pages

## What is a leaf page, and why does it exist?

A B+ tree is built to answer one question fast: *"given a key, find its value."*

The tree has two kinds of nodes:

- **Internal nodes** — routers. They hold keys and pointers to children. No actual data.
- **Leaf nodes** — the real thing. They hold the actual key-value pairs.

Every single piece of data you ever stored lives in a leaf page. Internal pages are just signposts pointing you toward the right leaf. If you removed every internal page, your data would still be intact — just harder to find.

In `simpledb` a leaf page is a thin wrapper around a `Page` (the slotted 8KB buffer you already know). The wrapper adds two B-tree-specific ideas: a **sibling pointer** for range scans, and **split logic** for when the page fills up.

---

## The two types

```rust
pub struct LeafPage {       // owns the page
    page: Page,
}

pub struct LeafPageMut<'a> { // borrows the page mutably
    page: &'a mut Page,
}
```

Why two types instead of one?

When the B-tree is running, pages live inside the `Pager` (a HashMap of page IDs → Pages). The pager *owns* all pages. To mutate a page, you borrow it from the pager with `get_mut()`, which gives you a `&mut Page`. You can't also *own* that page — Rust won't let you have both.

So the split is:

| Type | When you use it | Owns the page? |
|------|----------------|----------------|
| `LeafPage` | Reading, or just created a page not yet in pager | Yes |
| `LeafPageMut<'a>` | Mutating a page that lives in the pager | No, borrows it |

`LeafPage` is used in split results — the new right page is handed back to the caller as an owned value, ready to be inserted into the pager.

`LeafPageMut` is used when the tree walks down to a leaf to insert or delete — the page already lives in the pager and you just borrow it temporarily.

---

## What a leaf page looks like

A leaf page IS a `Page`. That means it has the same slotted layout:

```
byte 0                                                        byte 8191
┌──────────────────────┬──────────────────────┬──────┬───────────────────┐
│ Header (20 B)        │ Slot array           │ free │  Tuple region     │
│ magic                │ [off0][off1][off2]   │      │  [val2][val1][val0]│
│ page_id              │  sorted by key ───►  │      │  ◄─── grows left  │
│ slot_count           │                      │      │                   │
│ free_start           │                      │      │                   │
│ free_end             │                      │      │                   │
│ dead_bytes           │                      │      │                   │
│ next_leaf_page_id ●──┼──────────────────────┼──────┼───► sibling leaf  │
└──────────────────────┴──────────────────────┴──────┴───────────────────┘
```

The only field that a leaf page uses *differently* from a plain `Page` is `next_leaf_page_id`. For a plain `Page` it's just a header field. For a leaf page it's the **sibling pointer** — the chain that links all leaves together left to right.

---

## `entries()` — and why tombstones matter here

Both `LeafPage` and `LeafPageMut` have an `entries()` method:

```rust
pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::with_capacity(self.slot_count() as usize);
    for i in 0..self.slot_count() as usize {
        if let Some((k, v)) = self.key_val_at(i) {  // None if tombstoned
            out.push((k.to_vec(), v.to_vec()));
        }
    }
    out
}
```

`key_val_at` returns `None` for tombstoned tuples. This filter is **critical** in split logic.

When a page is full and needs to split, the code calls `self.entries()` to gather all the data it will redistribute. If it accidentally included tombstoned entries, it would write ghost keys into the new pages — keys that `get` would skip but that would occupy space and corrupt the sorted order.

Dead entries stay physically on the page but are invisible to `entries()`, and therefore invisible to any split.

---

## The sibling chain

All leaf pages form a singly-linked list ordered by key:

```
Page 2             Page 5             Page 9
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│ alice        │   │ eve          │   │ paul         │
│ bob          │──►│ henry        │──►│ zara         │──► None
│ carol        │   │ iris         │   │              │
└──────────────┘   └──────────────┘   └──────────────┘
next=5             next=9             next=0 (None)
```

The pointer is stored in 4 bytes at offset 16 of the page header. `0` means no next sibling.

**Why this chain exists:**

To do a range scan (`WHERE key BETWEEN 'bob' AND 'paul'`), you:
1. Walk the tree from the root to find the leaf containing `'bob'`.
2. Start reading from there.
3. Follow `next_leaf_page_id` pointers until you pass `'paul'`.

Without this chain you'd have to go back up to the root and walk down again for every leaf — expensive. With the chain it's a simple pointer follow.

---

## Method reference

### On `LeafPage` (read-only)

| Method | What it does |
|--------|-------------|
| `new(page_id)` | Allocates a fresh empty leaf |
| `from_page(p)` | Wraps an existing `Page` as a leaf |
| `into_page(self)` | Unwraps back to `Page` (e.g. before inserting into pager) |
| `get(key)` | Binary search → value bytes, or `None` |
| `slot_count()` | Number of live entries |
| `key_val_at(i)` | Key and value at slot index `i`, or `None` if tombstoned |
| `entries()` | All live entries as owned `Vec<(Vec<u8>, Vec<u8>)>` |
| `page_id()` | The page's ID |
| `next_leaf_page_id()` | The right sibling's page ID, or `None` |

### Additional on `LeafPageMut` (read + write)

| Method | What it does |
|--------|-------------|
| `insert(key, val)` | Delegates to `Page::put` |
| `remove(key)` | Delegates to `Page::remove` |
| `set_next_leaf_page_id(next)` | Wires or updates the sibling pointer |
| `insert_or_split(key, val, new_id)` | Insert, splitting the page if full — see [Leaf Splits](split.md) |

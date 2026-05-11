# Leaf Splits

## The problem

A leaf page is 8KB. You can only fit so many key-value pairs on it. Eventually it fills up and a new insert returns `Err("page full")`.

You can't just "make the page bigger" — pages are fixed-size. You have to split the one full page into two half-full pages, then tell the parent internal node about the new page so it can route traffic correctly.

This is `insert_or_split`.

---

## The contract

```rust
pub fn insert_or_split(
    &mut self,
    key: &[u8],
    val: &[u8],
    new_right_page_id: u32,
) -> Result<Option<LeafSplit>, &'static str>
```

- Returns `Ok(None)` → insert fit, nothing else to do.
- Returns `Ok(Some(split))` → page was full, split happened, caller must handle the split result.
- Returns `Err(e)` → something genuinely broken (not just full).

The caller must supply `new_right_page_id` upfront — a fresh page ID already allocated from the pager. The split function doesn't talk to the pager itself; it just uses the ID to label the new right page.

```rust
pub struct LeafSplit {
    pub separator_key: Vec<u8>,  // what to insert into the parent
    pub right_page: LeafPage,    // the new right sibling (caller stores in pager)
}
```

---

## Step-by-step walkthrough

### Starting state

Page 3 has 4 entries and is completely full. You want to insert `"dave"`.

```
Page 3 (full)                           Page 7
┌─────────────────┐                     ┌─────────────────┐
│ alice → "data1" │                     │ zara  → "data5" │
│ bob   → "data2" │ ──next──────────►   │                 │
│ carol → "data3" │                     └─────────────────┘
│ eve   → "data4" │
└─────────────────┘
```

Call: `leaf_mut.insert_or_split(b"dave", b"data_dave", new_page_id=6)`

---

### Step 1 — Try the normal insert first

```rust
match self.insert(key, val) {
    Ok(_) => return Ok(None),   // fits, done
    Err("page full") => { ... } // continue to split
}
```

The insert fails with `"page full"`. Now the split path begins.

---

### Step 2 — Collect all entries including the new one

```rust
let mut all = self.entries();               // live entries from current page
all.push((key.to_vec(), val.to_vec()));     // add the new entry
all.sort_by(|a, b| a.0.cmp(&b.0));         // sort by key
```

`entries()` only returns **live** (non-tombstoned) entries. After adding `"dave"` and sorting:

```
all = [
    ("alice", "data1"),
    ("bob",   "data2"),
    ("carol", "data3"),
    ("dave",  "data_dave"),  ← newly added
    ("eve",   "data4"),
]
```

---

### Step 3 — Find the split point

```rust
let mid = all.len() / 2;  // 5 / 2 = 2
```

```
index:  0        1       2        3        4
all = [alice,  bob,   carol,  dave,   eve ]
                       ▲
                      mid=2

left half  = all[..2]  = [alice, bob]
right half = all[2..]  = [carol, dave, eve]
```

---

### Step 4 — The separator key

```rust
let separator_key = right_entries[0].0.clone();  // "carol"
```

The separator key is the **first key of the right page**. This is the key that will be inserted into the parent internal node. It acts as a fence: *"if your key >= 'carol', go to the right page."*

**Critical rule — the separator key is ALSO kept in the right leaf.**

This is what "copy up" means. The key `"carol"` goes both to the parent AND stays as the first entry of the right leaf. This is different from internal node splits where the middle key is removed from both children. Leaves must keep their keys because the leaf chain is the only place the actual data lives.

```
           Parent (after caller updates it)
           ┌──────────────────┐
           │ ... | carol→pg6 | ... │
           └──────────────────┘
                      │
          ┌───────────┘ points to right leaf
          ▼
Page 6: [carol, dave, eve]   ← "carol" is here too
```

---

### Step 5 — Rebuild both pages from scratch

```rust
let old_next = self.page.next_leaf_page_id();  // save Page 3's old next (Page 7)

let mut new_left  = Page::new(self.page.page_id());   // same ID as original (page 3)
let mut new_right = Page::new(new_right_page_id);     // fresh page (page 6)

for (k, v) in left_entries  { new_left.put(k, v)?;  }
for (k, v) in right_entries { new_right.put(k, v)?; }
```

Both pages are created clean with `Page::new`, then entries are written in sorted order. This also resets any dead bytes — the new pages are compact from birth.

---

### Step 6 — Wire the sibling pointers

```rust
new_left.set_next_leaf_page_id(Some(new_right_page_id));  // left → new right
new_right.set_next_leaf_page_id(old_next);                // new right → old next
```

Before split:
```
Page 3 ──────────────────────────────────────────► Page 7
```

After split:
```
Page 3 ──────────► Page 6 ──────────────────────► Page 7
[alice, bob]        [carol, dave, eve]              [zara]
```

`old_next` is saved before rebuilding the pages because `Page::new` zeroes out the header, and the original page's next pointer would be lost. If you forgot `old_next`, the chain would be severed at Page 7.

---

### Step 7 — Replace the original page in memory

```rust
*self.page = new_left;
```

`self.page` is a `&mut Page` pointing to the page that lives inside the pager. This line overwrites the pager's copy of page 3 with the new left contents — in place, with no heap allocation or pager call needed.

---

### Step 8 — Return the split result

```rust
Ok(Some(LeafSplit {
    separator_key,                              // "carol"
    right_page: LeafPage::from_page(new_right), // page 6, owned
}))
```

The caller (the `BTree`) must now:
1. Store `right_page` in the pager under page ID 6.
2. Insert `("carol", page_id=6)` into the parent internal node.

If there is no parent yet (the root was a leaf and just split), the `BTree` must create a new root internal node.

---

## Full before / after picture

```
BEFORE (page 3 is full, inserting "dave"):

        [parent internal node]
              │
              ▼
Page 3: [alice, bob, carol, eve] ──next──► Page 7: [zara]


AFTER split (new_right_page_id = 6):

        [parent internal node]
          │             │
          ▼             ▼
Page 3: [alice, bob]   Page 6: [carol, dave, eve] ──next──► Page 7: [zara]
     ──next──►

Parent now has separator "carol" pointing to page 6.
```

---

## What the caller does with the separator key

`insert_or_split` hands back a `LeafSplit` and stops there. It has no idea what the rest of the tree looks like. The caller — the `BTree` — is responsible for what happens next.

There are two cases.

---

### Case 1 — A parent internal node exists

The caller takes the separator key and the new right page ID and inserts them into the parent:

```rust
parent.insert(separator_key, right_page.page_id())
```

This adds one new routing entry to the parent. The parent now knows: *"if key >= 'carol', go to page 6."*

```
BEFORE split:

        [parent: bob→pg3]
                │
                ▼
        pg3: [alice, bob, carol, eve]   (full)


AFTER split (separator="carol", new right=pg6):

        [parent: bob→pg3, carol→pg6]
                │                │
                ▼                ▼
     pg3: [alice, bob]     pg6: [carol, dave, eve]
```

The parent gained one entry. The leaf level gained one page. The tree is wider but not taller.

---

### Case 2 — There is no parent (the leaf was the root)

A tree starts as a single leaf page that is also the root. When that leaf splits for the first time, there is no parent to insert into. The `BTree` must **create a new root internal node**:

```rust
let new_root = InternalPage::new(new_root_page_id, left_page_id);
new_root.insert(separator_key, right_page_id);
```

- `leftmost_child` of the new root = left page ID
- One separator entry = `(separator_key, right_page_id)`

```
BEFORE (single leaf, also the root):

   root=pg3: [alice, bob, carol, eve]   (full, about to split)


AFTER (tree grows one level taller):

   root=pg9 (new internal node)
   leftmost=pg3   carol→pg6
        │               │
        ▼               ▼
   pg3: [alice, bob]   pg6: [carol, dave, eve]
```

This is the **only moment the tree ever grows taller**. Every other split just makes the tree wider by adding a new leaf and a new entry somewhere in an existing internal node.

---

### What if the parent internal node is also full?

When the caller tries `parent.insert(...)` and the parent returns `"page full"`, the parent internal node must split too — producing its own separator key pushed up to *its* parent. This cascades upward until either a non-full ancestor is found or the root itself splits and a new root is created.

```
Cascade example:

   Level 2 (root): split → new root created, tree grows taller
        │
   Level 1 (internal): split → separator pushed up to level 2
        │
   Level 0 (leaf): split → separator pushed up to level 1
```

In practice, cascades are rare. A page holds hundreds of entries, so internal nodes fill up much slower than leaves.

---

## Split invariants

After every split, all of these must hold:

| Invariant | Why it matters |
|-----------|---------------|
| `separator_key == right_page.entries()[0].0` | Parent uses this key for routing — it must exactly match the first key of the right leaf |
| Every left key < every right key | The whole B+ tree property; violated keys break binary search at every level above |
| No key is lost or duplicated | Correctness — every inserted entry must be findable after the split |
| `right.next == old left.next` | The leaf chain must remain intact for range scans |
| `left.next == new_right_page_id` | Left must point to its new right sibling |
| Neither page is empty | A split into one full page and one empty page would immediately re-trigger another split |

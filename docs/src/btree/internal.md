# Internal Pages

## What is an internal page?

An internal page does not hold your data. It holds **directions to your data**.

Think of a building directory in a lobby: it doesn't contain any offices, it just tells you *"accounting is on floor 3, engineering is on floor 5."* The directory is the internal page. The actual offices are the leaf pages.

Every key stored in an internal page is a **separator** — a boundary marker that says: *"everything to my left is smaller than me, everything to my right is greater than or equal to me."*

When you search for a key, you read the internal page, pick the right direction, and follow the pointer. You never stop at an internal page — you keep going until you reach a leaf.

---

## The layout

An internal page is also backed by the same slotted `Page` structure. Each slot stores a separator key mapped to a **child page ID** (a 4-byte `u32`):

```rust
pub struct InternalPage {
    page: Page,
}
```

Each entry in the slot array looks like:

```
slot[i] → [ separator_key | child_page_id (4 bytes LE) ]
```

The slot array is sorted by separator key, just like a leaf.

There is one special case: the **leftmost child**. For every internal node with `n` separator keys, there are `n+1` children. The leftmost child (for keys smaller than the first separator) has no key to pair with. It is stored separately in the `next_leaf_page_id` header field, which is repurposed for internal pages.

```
Internal page layout (logical):

  leftmost_child | sep[0]→child[0] | sep[1]→child[1] | sep[2]→child[2]
       │                │                  │                  │
       ▼                ▼                  ▼                  ▼
  keys < sep[0]   sep[0]<=k<sep[1]  sep[1]<=k<sep[2]    k>=sep[2]
```

In memory (the actual `Page`):

```
┌──────────────────────┬───────────────────────────┬──────┬────────────────────┐
│ Header               │ Slot array                │ free │ Tuple region       │
│ next_leaf_page_id=L  │ [sep0→A][sep1→B][sep2→C]  │      │ [C_id][B_id][A_id] │
│ (repurposed as       │  sorted by key            │      │ (4 bytes each)     │
│  leftmost child L)   │                           │      │                    │
└──────────────────────┴───────────────────────────┴──────┴────────────────────┘
```

---

## The fencepost problem

Why is there always one more child than separators?

Picture 3 fence panels between 4 fence posts:

```
post   panel   post   panel   post   panel   post
 |─────────────|─────────────|─────────────|
```

Separators are the panels (they divide space). Children are the posts (they ARE the spaces). With `n` separators you always get `n+1` children.

```
n=3 separators:  "bob"       "eve"       "zara"
                  │           │           │
   [keys<bob]  [bob..eve)  [eve..zara)  [keys>=zara]
       ▲           ▲           ▲            ▲
   leftmost    child[0]    child[1]      child[2]
```

The leftmost child is the extra "post" on the far left that no separator owns.

---

## `find_child` — how routing works

This is the most important method. Given a search key, return which child to follow.

```rust
pub fn find_child(&self, key: &[u8]) -> u32 {
    let n = self.slot_count() as usize;

    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if self.key_at(mid) <= key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }

    if lo == 0 {
        self.leftmost_child()
    } else {
        self.child_at(lo - 1)
    }
}
```

The binary search finds the **rightmost separator that is ≤ the search key**. After the loop, `lo` equals the count of separators that are ≤ the search key.

Concrete example — separators: `["bob", "eve", "zara"]`, leftmost=L, children=[A, B, C]:

| Search key | Separators ≤ key | `lo` | Child returned |
|------------|-----------------|------|----------------|
| `"alice"`  | none            | 0    | leftmost (L)   |
| `"bob"`    | `"bob"`         | 1    | child[0] (A)   |
| `"carol"`  | `"bob"`         | 1    | child[0] (A)   |
| `"eve"`    | `"bob"`, `"eve"`| 2    | child[1] (B)   |
| `"zara"`   | all 3           | 3    | child[2] (C)   |
| `"zzz"`    | all 3           | 3    | child[2] (C)   |

Notice `"carol"` → child A, because `"bob" <= "carol" < "eve"`. Carol lives in the range owned by separator `"bob"`.

---

## The two types

Like `LeafPage` / `LeafPageMut`, internal nodes come in two flavours:

```rust
pub struct InternalPage {        // owns the page — read-only
    page: Page,
}

pub struct InternalPageMut<'a> { // borrows the page mutably — can insert and split
    page: &'a mut Page,
}
```

`InternalPage` is used when the pager hands you an owned `Page` and you only need to read or route. `InternalPageMut` is used when you need to write: inserting a separator after a child splits, or splitting the internal node itself.

---

## `insert` — adding a new routing entry

When a leaf below this node splits, the split produces a `separator_key` and a new right page. The parent internal node must record this:

```rust
pub fn insert(&mut self, key: &[u8], right_child: u32) -> Result<Option<Vec<u8>>, &'static str> {
    self.page.put(key, &right_child.to_le_bytes())
}
```

The child page ID is stored as the value (4 raw bytes, little-endian). The separator key is the key. The underlying `Page::put` keeps the slot array sorted automatically.

---

## Internal node split — "push up"

An internal page also has a fixed size. When `insert_separator` returns `"page full"`, the internal page itself must split. This works differently from a leaf split.

### The rule: the middle key is pushed up, not copied up

In a leaf split, the middle key is **copied** to the parent — it also stays in the right leaf (because leaves need all their keys for scans).

In an internal split, the middle key is **pushed** to the parent — it is **removed** from both children. It only lives in the parent.

Why? Internal nodes are routing tables, not data stores. You only need enough keys to point traffic in the right direction. Once a key has been promoted to the parent, the children on each side of it already know their bounds — they don't need to store the key themselves.

### Step-by-step example

Internal page before (leftmost=L, 5 entries, page is full):

```
L | bob→A | eve→B | henry→C | paul→D | zara→E
```

Insert separator `("kate", F)` → page full → split.

**Step 1 — collect all entries, add new one, sort:**

```
all = [(bob,A), (eve,B), (henry,C), (kate,F), (paul,D), (zara,E)]
       idx 0     idx 1    idx 2      idx 3      idx 4     idx 5
```

**Step 2 — find the midpoint:**

```
mid = 6 / 2 = 3
```

**Step 3 — identify the three parts:**

```
left  = all[..3]  = [(bob,A), (eve,B), (henry,C)]
pushed_up         = all[3]   = (kate, F)          ← goes to parent, removed from here
right = all[4..]  = [(paul,D), (zara,E)]
```

**Step 4 — build left page (same page ID, same leftmost child):**

```
Left internal page:
  leftmost=L | bob→A | eve→B | henry→C
```

**Step 5 — build right page (new page ID, leftmost = pushed-up entry's child):**

The pushed-up entry was `(kate, F)`. The key `"kate"` goes to the parent. Its child `F` becomes the leftmost child of the right page — because `F` is the page for keys `"kate" <= k < "paul"`.

```
Right internal page:
  leftmost=F | paul→D | zara→E
```

**Step 6 — return the result:**

```rust
InternalSplit {
    separator_key: "kate",     // insert into parent
    right_page: /* right */,   // store in pager
}
```

### Full before / after

```
BEFORE (inserting kate→F into full internal node):

        [grandparent]
              │
              ▼
  L | bob→A | eve→B | henry→C | paul→D | zara→E


AFTER (new_right_page_id = 9):

           [grandparent]
           now has "kate"→9 inserted
                │           │
                ▼           ▼
  L|bob→A|eve→B|henry→C    F|paul→D|zara→E
  (original page, rebuilt)  (new page 9)

  "kate" is GONE from both children.
  It lives only in the grandparent.
```

---

## Leaf split vs internal split — side by side

| | Leaf split | Internal split |
|---|---|---|
| Where does the middle key go? | **Copied up** — stays in right leaf + sent to parent | **Pushed up** — only in parent, gone from children |
| Right child's leftmost entry | The first entry of the right half | The **child pointer** from the middle entry |
| Why the difference? | Leaves need all keys for range scan via sibling chain | Internal nodes only route — parent has the key now |

---

## Method reference

### `InternalPage` (read-only)

| Method | What it does |
|--------|-------------|
| `new(page_id, leftmost_child)` | Fresh internal page with a starting leftmost child |
| `from_page(p)` | Wrap an existing `Page` as an internal node |
| `into_page(self)` | Unwrap back to `Page` |
| `page_id()` | This node's page ID |
| `slot_count()` | Number of separator entries |
| `leftmost_child()` | Child page ID for keys < first separator |
| `key_at(i)` | Separator key at slot index `i` |
| `child_at(i)` | Right child page ID for separator at index `i` |
| `find_child(key)` | Binary search → which child page ID to follow |
| `entries()` | All `(separator_key, right_child)` pairs as a `Vec` |

### `InternalPageMut<'a>` (mutable)

All `InternalPage` read methods are available, plus:

| Method | What it does |
|--------|-------------|
| `new(page)` | Borrow a `&mut Page` as a mutable internal node |
| `set_leftmost_child(id)` | Update the leftmost child pointer |
| `insert(key, child)` | Add a routing entry; returns `Err("page full")` if no space |
| `insert_or_split(key, child, new_id)` | Insert with split if needed; returns `Option<InternalSplit>` |

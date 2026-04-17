# Leaf Splits

When a leaf page is full, inserting a new key-value pair requires splitting the page into two. `LeafPageMut::insert_or_split` handles this transparently — the caller only needs to check whether a `LeafSplit` was returned.

## Algorithm

```rust
pub fn insert_or_split(
    &mut self,
    key: &[u8],
    val: &[u8],
    new_right_page_id: u32,
) -> Result<Option<LeafSplit>, &'static str>
```

1. **Try a normal insert.** If it succeeds, return `Ok(None)` — no split needed.
2. **On "page full"**, collect *all* current entries plus the new `(key, val)` into a `Vec`.
3. **Sort** the combined list by key.
4. **Split at the midpoint**: left half stays in the current page, right half goes to a new page with `new_right_page_id`.
5. **Separator key** = first key of the right page (`right_entries[0].0`).
6. **Rebuild both pages** from scratch using `Page::new` + `put` calls.
7. **Wire sibling pointers**:
   - `left.next = new_right_page_id`
   - `right.next = old_next` (preserves the existing chain)
8. Overwrite `*self.page` with the new left page.
9. Return `Ok(Some(LeafSplit { separator_key, right_page }))`.

## Before and after

```
Before split (page 1 is full, inserting k04):

Page 1: [k00, k01, k02, k03]  ──next──►  Page 5

After split (new_right_page_id = 2):

Page 1: [k00, k01]  ──next──►  Page 2: [k02, k03, k04]  ──next──►  Page 5
                                ▲
                          separator_key = k02
```

The caller receives the separator key and the right page. If there is a parent internal node, the separator key and a pointer to page 2 must be inserted there.

## Code

```rust
Err("page full") => {
    let mut all = self.entries();
    all.push((key.to_vec(), val.to_vec()));
    all.sort_by(|a, b| a.0.cmp(&b.0));

    let mid = all.len() / 2;
    let left_entries  = &all[..mid];
    let right_entries = &all[mid..];

    let separator_key = right_entries[0].0.clone();
    let old_next = self.page.next_leaf_page_id();

    let mut new_left  = Page::new(self.page.page_id());
    let mut new_right = Page::new(new_right_page_id);

    for (k, v) in left_entries  { new_left.put(k, v)?;  }
    for (k, v) in right_entries { new_right.put(k, v)?; }

    new_left.set_next_leaf_page_id(Some(new_right_page_id));
    new_right.set_next_leaf_page_id(old_next);

    *self.page = new_left;

    Ok(Some(LeafSplit { separator_key, right_page: LeafPage::from_page(new_right) }))
}
```

## Split invariants (enforced by tests)

- The right page is never empty.
- `separator_key == right_page.entries()[0].0` — the separator is exactly the first key of the right page.
- Every key in the left page is strictly less than every key in the right page.
- No key-value pair is lost or duplicated.
- The existing sibling chain is preserved: `right.next = old_next`.

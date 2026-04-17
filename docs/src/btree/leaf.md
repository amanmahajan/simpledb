# Leaf Pages

A B-tree leaf page holds sorted key-value pairs. In `simpledb` the leaf layer is a thin wrapper around `Page` that adds the B-tree-specific concept of a sibling pointer and the split operation.

## Types

### `LeafPage` — owned leaf

`LeafPage` owns a `Page` and exposes read-only operations.

```rust
pub struct LeafPage {
    page: Page,
}
```

Key methods:

```rust
impl LeafPage {
    pub fn new(page_id: u32) -> Self { ... }
    pub fn from_page(p: Page) -> Self { ... }
    pub fn into_page(self) -> Page { ... }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> { ... }
    pub fn slot_count(&self) -> u16 { ... }
    pub fn key_val_at(&self, slot_id: usize) -> Option<(&[u8], &[u8])> { ... }
    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> { ... }

    pub fn page_id(&self) -> u32 { ... }
    pub fn next_leaf_page_id(&self) -> Option<u32> { ... }
}
```

### `LeafPageMut<'a>` — borrowed mutable leaf

`LeafPageMut<'a>` holds a `&'a mut Page` for mutation. This lifetime-based design lets the `BTree` own the pages through the `Pager` while giving the leaf layer a temporary mutable view.

```rust
pub struct LeafPageMut<'a> {
    page: &'a mut Page,
}
```

Additional write methods:

```rust
impl<'a> LeafPageMut<'a> {
    pub fn insert(&mut self, key: &[u8], val: &[u8])
        -> Result<Option<Vec<u8>>, &'static str> { ... }
    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> { ... }
    pub fn set_next_leaf_page_id(&mut self, next: Option<u32>) { ... }
    pub fn insert_or_split(...) -> Result<Option<LeafSplit>, &'static str> { ... }
}
```

### `LeafSplit`

Returned when `insert_or_split` decides the page must split:

```rust
pub struct LeafSplit {
    pub separator_key: Vec<u8>,  // first key of the right page
    pub right_page: LeafPage,    // the newly created right leaf
}
```

The caller (eventually the `BTree`) is responsible for storing the `right_page` in the pager and inserting `separator_key` into the parent internal node.

## Sibling pointer chain

Leaf pages form a singly-linked list via `next_leaf_page_id`. This enables efficient forward range scans without traversing the tree from the root.

```
Page 1  ──next──►  Page 3  ──next──►  Page 5  ──next──►  None
[k00..k02]         [k03..k05]         [k06..k08]
```

The pointer is stored in the page header at offset 16 (4 bytes). `0` means no next sibling.

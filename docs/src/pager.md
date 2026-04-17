# The Pager

The `Pager` manages the collection of pages. It assigns page IDs, allocates new pages, and provides mutable or shared access to existing ones.

## Current implementation

The pager is entirely in-memory, backed by a `HashMap`:

```rust
pub struct Pager {
    pages: HashMap<u32, Page>,
    next_page_id: u32,
}
```

Page IDs start at `1` and increment monotonically. ID `0` is reserved as a sentinel meaning "no page" (used by the `next_leaf_page_id` sibling pointer).

## API

```rust
impl Pager {
    pub fn new() -> Self { ... }

    /// Allocate a fresh page ID without creating a page.
    pub fn alloc_page_id(&mut self) -> u32 { ... }

    /// Allocate an ID, create an empty page, store it, and return the ID.
    pub fn new_page(&mut self) -> u32 { ... }

    /// Store a page that was created externally (e.g. after a leaf split).
    pub fn insert_page(&mut self, page: Page) { ... }

    pub fn get(&self, page_id: u32) -> Option<&Page> { ... }
    pub fn get_mut(&mut self, page_id: u32) -> Option<&mut Page> { ... }
    pub fn contains(&self, page_id: u32) -> bool { ... }
}
```

### Usage pattern for leaf splits

A split produces a `LeafSplit` containing a new right `LeafPage`. The caller inserts it into the pager:

```rust
let split = leaf.insert_or_split(key, val, pager.alloc_page_id())?;
if let Some(s) = split {
    pager.insert_page(s.right_page.into_page());
    // then insert separator_key into the parent internal node
}
```

## Limitations

The current pager is a development scaffold:

- **No disk I/O.** All pages live in process memory and are lost on exit.
- **No eviction.** Pages are never freed; memory grows without bound.
- **No write-ahead log.** There is no crash recovery.

Future work will replace the `HashMap` with a buffer pool that reads and writes 8 KB pages to a file, with a WAL for durability.

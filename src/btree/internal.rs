use crate::page::Page;

/// A B+ tree internal (branch) node.
///
/// Each slot stores a separator key mapped to its right child page ID (4-byte LE u32).
/// The leftmost child pointer (for keys < first separator) lives in the page's
/// `next_leaf_page_id` header field — internal nodes never need a leaf sibling pointer.
///
/// Logical layout:
///   leftmost_child | sep[0]→child[0] | sep[1]→child[1] | ...
///
/// Routing rule for search key `k`:
///   k < sep[0]          → leftmost_child
///   sep[i] <= k < sep[i+1]  → child[i]
///   k >= sep[last]      → child[last]

pub struct InternalPage {
    page: Page,
}

pub struct  InternalSplit {
    pub separator_key: Vec<u8>,
    pub right_page: InternalPage,
}

impl InternalPage {
    pub fn new(page_id:u32, leftmost_child:u32) -> Self {
        let mut page = Page::new(page_id);
        page.set_next_leaf_page_id(Some(leftmost_child));
        Self{page}
    }

    pub fn from_page(p: Page) -> Self {
        Self { page: p }
    }

    pub fn into_page(self) -> Page {
        self.page
    }

    pub fn page_id(&self) -> u32 {
        self.page.page_id()
    }

    pub fn slot_count(&self) -> u16 {
        self.page.slot_count()
    }

    pub fn free_space_bytes(&self) -> usize {
        self.page.free_space_bytes()
    }
    
    pub fn leftmost_child(&self) -> u32 {
        self.page.next_leaf_page_id().unwrap_or(0)
    }

    /// Returns the separator key at index `i`.
    pub fn key_at(&self, i: usize) -> &[u8] {
        let off = self.page.read_slot(i) as usize;
        self.page.read_key(off)
    }

    /// Returns the right child page ID for separator at index `i`.
    pub fn child_at(&self, i: usize) -> u32 {
        let off = self.page.read_slot(i) as usize;
        let bytes = self.page.read_tuple_val(off);
        u32::from_le_bytes(bytes.try_into().unwrap())
    }

    /// Returns the child page ID to follow when searching for `key`.
    pub fn find_child(&self, key: &[u8]) -> u32 {
        let n = self.slot_count() as usize;
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo+ hi)/2;
            if self.key_at(mid) <= key {
                lo = mid+1;
            } else {
                hi = mid;
            }
        }
        // lo = count of separators <= key
        if lo == 0 {
            self.leftmost_child()
        } else {
            self.child_at(lo - 1)
        }
    }

    fn entries(&self) -> Vec<(Vec<u8>, u32)> {
        (0..self.slot_count() as usize)
            .map(|i| {
                let off = self.page.read_slot(i) as usize;
                let key = self.page.read_key(off).to_vec();
                let bytes = self.page.read_tuple_val(off);
                let child = u32::from_le_bytes(bytes.try_into().unwrap());
                (key, child)
            })
            .collect()
    }
    
    
    
}

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

    pub fn entries(&self) -> Vec<(Vec<u8>, u32)> {
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

pub struct InternalPageMut<'a> {
    page: &'a mut Page,
}

impl<'a> InternalPageMut<'a> {
    pub fn new(page: &'a mut Page) -> Self {
        Self { page }
    }

    pub fn page(&self) -> &Page {
        self.page
    }

    pub fn page_mut(&mut self) -> &mut Page {
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

    pub fn set_leftmost_child(&mut self, child_id: u32) {
        self.page.set_next_leaf_page_id(Some(child_id));
    }

    pub fn key_at(&self, i: usize) -> &[u8] {
        let off = self.page.read_slot(i) as usize;
        self.page.read_key(off)
    }

    pub fn child_at(&self, i: usize) -> u32 {
        let off = self.page.read_slot(i) as usize;
        let bytes = self.page.read_tuple_val(off);
        u32::from_le_bytes(bytes.try_into().unwrap())
    }

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

    pub fn entries(&self) -> Vec<(Vec<u8>, u32)> {
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

    pub fn insert(&mut self, sep_key: &[u8], right_child: u32) -> Result<Option<Vec<u8>>, &'static str> {
        self.page.put(sep_key, &right_child.to_le_bytes())
    }

    pub fn insert_or_split(
        &mut self,
        sep_key: &[u8],
        right_child: u32,
        new_right_page_id: u32,
    ) -> Result<Option<InternalSplit>, &'static str> {
        match self.insert(sep_key, right_child) {
            Ok(_) => Ok(None),
            Err("page full") => {
                let mut all = self.entries();
                all.push((sep_key.to_vec(), right_child));
                all.sort_by(|a, b| a.0.cmp(&b.0));

                let mid = all.len() / 2;
                let separator_key = all[mid].0.clone();
                let right_leftmost = all[mid].1;
                let old_leftmost = self.leftmost_child();

                let mut new_left = Page::new(self.page.page_id());
                new_left.set_next_leaf_page_id(Some(old_leftmost));
                for (k, c) in &all[..mid] {
                    new_left.put(k, &c.to_le_bytes())?;
                }

                let mut new_right = Page::new(new_right_page_id);
                new_right.set_next_leaf_page_id(Some(right_leftmost));
                for (k, c) in &all[mid + 1..] {
                    new_right.put(k, &c.to_le_bytes())?;
                }

                *self.page = new_left;

                Ok(Some(InternalSplit {
                    separator_key,
                    right_page: InternalPage::from_page(new_right),
                }))
            }
            Err(e) => Err(e),
        }
    }
}

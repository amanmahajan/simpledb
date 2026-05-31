use crate::btree::internal::{InternalPage, InternalPageMut, InternalSplit};
use crate::btree::leaf::{LeafPageMut, LeafSplit};
use crate::pager::pager::Pager;

pub struct BTree {
    root_page_id: u32,
    height: u32,
    pager: Pager,
}

/// Forward iterator over (key, value) pairs in sorted key order.
///
/// Follows the leaf sibling chain. Skips tombstoned slots silently.
pub struct Scan<'a> {
    pager: &'a Pager,
    current_page_id: Option<u32>,
    slot_idx: usize,
    skip_before: Option<Vec<u8>>,
}

impl<'a> Iterator for Scan<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let page_id = self.current_page_id?;
            let page = self.pager.get(page_id)?;

            if self.slot_idx < page.slot_count() as usize {
                self.slot_idx += 1;
                let Some((k, v)) = page.get_key_value_at_slot(self.slot_idx - 1) else {
                    continue;
                };
                // On the first leaf, skip entries before the requested start key.
                let before_start = self.skip_before.as_deref().map_or(false, |s| k < s);
                if before_start {
                    continue;
                }
                self.skip_before = None;
                return Some((k, v));
            } else {
                let next_id = page.next_leaf_page_id();
                self.current_page_id = next_id;
                self.slot_idx = 0;
            }
        }
    }
}

impl Default for BTree {
    fn default() -> Self {
        Self::new()
    }
}

impl BTree {
    pub fn new() -> Self {
        Self {
            root_page_id: 0,
            height: 0,
            pager: Pager::new(),
        }
    }

    /// Descends to the leaf page that should contain `key`.
    /// Returns (leaf_page_id, path_of_internal_page_ids from root down to the parent).
    fn find_leaf(&self, key: &[u8]) -> (u32, Vec<u32>) {
        let mut path = Vec::new();
        let mut current = self.root_page_id;
        for _ in 1..self.height {
            path.push(current);
            let page = self.pager.get(current).unwrap().clone();
            let node = InternalPage::from_page(page);
            current = node.find_child(key);
        }
        (current, path)
    }

    /// Returns the page_id of the leftmost leaf (the first page in scan order).
    fn find_leftmost_leaf(&self) -> Option<u32> {
        if self.height == 0 {
            return None;
        }
        let mut current = self.root_page_id;
        for _ in 1..self.height {
            let page = self.pager.get(current).unwrap().clone();
            let node = InternalPage::from_page(page);
            current = node.leftmost_child();
        }
        Some(current)
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        if self.height == 0 {
            return None;
        }
        let (leaf_id, _) = self.find_leaf(key);
        self.pager.get(leaf_id)?.get(key)
    }

    pub fn insert(&mut self, key: &[u8], val: &[u8]) -> Result<(), &'static str> {
        if self.height == 0 {
            self.root_page_id = self.pager.new_page();
            self.height = 1;
        }

        let (leaf_id, path) = self.find_leaf(key);
        let new_right_leaf_id = self.pager.alloc_page_id();

        let split = {
            let page = self.pager.get_mut(leaf_id).unwrap();
            let mut leaf = LeafPageMut::new(page);
            leaf.insert_or_split(key, val, new_right_leaf_id)?
        };

        let Some(LeafSplit { separator_key, right_page }) = split else {
            return Ok(());
        };

        self.pager.insert_page(right_page.into_page());
        self.propagate_split(path, separator_key, new_right_leaf_id)
    }

    /// Walks the recorded path bottom-up, inserting the split separator at each level.
    /// If the root itself splits, a new root is created and height increments.
    fn propagate_split(
        &mut self,
        mut path: Vec<u32>,
        mut sep_key: Vec<u8>,
        mut right_child: u32,
    ) -> Result<(), &'static str> {
        while let Some(parent_id) = path.pop() {
            let new_right_id = self.pager.alloc_page_id();
            let split = {
                let page = self.pager.get_mut(parent_id).unwrap();
                let mut node = InternalPageMut::new(page);
                node.insert_or_split(&sep_key, right_child, new_right_id)?
            };

            let Some(InternalSplit { separator_key, right_page }) = split else {
                return Ok(());
            };

            self.pager.insert_page(right_page.into_page());
            sep_key = separator_key;
            right_child = new_right_id;
        }

        // The split bubbled all the way to the root — grow the tree by one level.
        let new_root_id = self.pager.new_page();
        let old_root_id = self.root_page_id;
        {
            let page = self.pager.get_mut(new_root_id).unwrap();
            let mut root = InternalPageMut::new(page);
            root.set_leftmost_child(old_root_id);
            root.insert(&sep_key, right_child)?;
        }
        self.root_page_id = new_root_id;
        self.height += 1;

        Ok(())
    }

    /// Removes `key` and returns its old value, or `None` if not found.
    /// No rebalancing — underflowing pages are left sparse.
    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        if self.height == 0 {
            return None;
        }
        let (leaf_id, _) = self.find_leaf(key);
        let page = self.pager.get_mut(leaf_id)?;
        LeafPageMut::new(page).remove(key)
    }

    /// Returns a forward iterator over all (key, value) pairs with key >= `start`
    /// (or all pairs when `start` is `None`), in sorted key order.
    pub fn scan(&self, start: Option<&[u8]>) -> Scan<'_> {
        if self.height == 0 {
            return Scan {
                pager: &self.pager,
                current_page_id: None,
                slot_idx: 0,
                skip_before: None,
            };
        }
        let (first_leaf_id, skip_before) = match start {
            Some(key) => {
                let (leaf_id, _) = self.find_leaf(key);
                (Some(leaf_id), Some(key.to_vec()))
            }
            None => (self.find_leftmost_leaf(), None),
        };
        Scan {
            pager: &self.pager,
            current_page_id: first_leaf_id,
            slot_idx: 0,
            skip_before,
        }
    }
}

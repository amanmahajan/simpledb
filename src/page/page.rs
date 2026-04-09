// page.rs
use crate::utils::byte::*;
use crate::btree::tuple::{Tuple, TupleBuilder};
use crate::btree::key::Key;

/// Peanut DB - 8KB slotted page (InnoDB-style 2-byte record directory).
///
/// Layout:
/// [ Header (fixed) ]
/// [ Slot array: u16 offsets (sorted by key), grows downward ]
/// [ Free space ]
/// [ Tuple region: variable-length tuples, grows upward from bottom ]
///
/// Endianness: Little-endian for all integer fields

pub const PAGE_SIZE: usize = 8 * 1024; //8kb
pub const HEADER_SIZE: usize = 16;

//page_magic is a sentinel value stored in every page header that identifies:
// “This block of bytes is a Peanut DB page, and it uses this page format.”
pub const PAGE_MAGIC: u32 = 0x504E5554; // 'PNUT'

// Header offsets
const HDR_MAGIC_OFF: usize = 0;
const HDR_PAGE_ID_OFF: usize = 4;
const HDR_SLOT_CNT_OFF: usize = 8;
const HDR_FREE_START_OFF: usize = 10;
const HDR_FREE_END_OFF: usize = 12;
const HDR_DEAD_BYTES_OFF: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchResult {
    Found(usize),
    NotFound(usize), // insertion position
}

#[derive(Debug, Clone)]
pub struct Page {
    data: [u8; PAGE_SIZE],
    dead_tuple_compact_percent: u8,
}

impl Page {
    pub const SLOT_SIZE: usize = 2;
    pub const DEFAULT_DEAD_TUPLE_COMPACT_PERCENT: u8 = 75;

    // -----------------------------
    // Construction
    // -----------------------------
    /// Creates a fresh, empty page. Writes the page magic, page ID, and initializes free-space pointers.
    pub fn new(page_id: u32) -> Self {
        let mut p = Page {
            data: [0u8; PAGE_SIZE],
            dead_tuple_compact_percent: Self::DEFAULT_DEAD_TUPLE_COMPACT_PERCENT,
        };

        write_u32(&mut p.data, HDR_MAGIC_OFF, PAGE_MAGIC);
        write_u32(&mut p.data, HDR_PAGE_ID_OFF, page_id);
        write_u16(&mut p.data, HDR_SLOT_CNT_OFF, 0);
        write_u16(&mut p.data, HDR_FREE_START_OFF, HEADER_SIZE as u16);
        write_u16(&mut p.data, HDR_FREE_END_OFF, PAGE_SIZE as u16);
        write_u16(&mut p.data, HDR_DEAD_BYTES_OFF, 0);

        p
    }

    /// Sets compaction threshold percentage for dead tuple bytes.
    pub fn set_dead_tuple_compact_percent(&mut self, percent: u8) {
        self.dead_tuple_compact_percent = percent.clamp(1, 100);
    }

    // -----------------------------
    // Header access
    // -----------------------------
    /// Returns how many slots (records) are currently tracked in this page.
    pub fn slot_count(&self) -> u16 {
        read_u16(&self.data, HDR_SLOT_CNT_OFF)
    }

    /// Returns the byte offset where free space starts (right after the slot array).
    pub fn free_start(&self) -> u16 {
        read_u16(&self.data, HDR_FREE_START_OFF)
    }

    /// Returns the byte offset where free space ends (right before tuple bytes).
    pub fn free_end(&self) -> u16 {
        read_u16(&self.data, HDR_FREE_END_OFF)
    }

    /// Returns bytes currently occupied by dead tuples.
    pub fn dead_tuple_bytes(&self) -> u16 {
        read_u16(&self.data, HDR_DEAD_BYTES_OFF)
    }

    fn set_dead_tuple_bytes(&mut self, dead_bytes: u16) {
        write_u16(&mut self.data, HDR_DEAD_BYTES_OFF, dead_bytes);
    }

    fn page_id(&self) -> u32 {
        read_u32(&self.data, HDR_PAGE_ID_OFF)
    }

    /// Returns free bytes available on this page (`free_end - free_start`).
    pub fn free_space_bytes(&self) -> usize {
        self.free_end() as usize - self.free_start() as usize
    }

    fn tuple_region_used_bytes(&self) -> usize {
        PAGE_SIZE - self.free_end() as usize
    }

    fn tuple_total_len(&self, off: usize) -> usize {
        Self::TUP_HDR_SIZE + self.read_tuple_key_len(off) + self.read_tuple_val_len(off)
    }

    fn add_dead_tuple_bytes(&mut self, dead_len: usize) {
        let current = self.dead_tuple_bytes() as usize;
        let next = current.saturating_add(dead_len).min(u16::MAX as usize) as u16;
        self.set_dead_tuple_bytes(next);
    }

    fn should_compact_dead_tuples(&self) -> bool {
        let used = self.tuple_region_used_bytes();
        if used == 0 {
            return false;
        }
        (self.dead_tuple_bytes() as usize) * 100
            >= used * self.dead_tuple_compact_percent as usize
    }

    fn compact_live_tuples(&mut self) -> Result<(), &'static str> {
        let mut live = Vec::with_capacity(self.slot_count() as usize);

        for i in 0..self.slot_count() as usize {
            let off = self.read_slot(i) as usize;
            if self.read_tuple_tombstone(off) == 1 {
                continue;
            }
            live.push((self.read_key(off).to_vec(), self.read_tuple_val(off).to_vec()));
        }

        let mut compacted = Page::new(self.page_id());
        compacted.set_dead_tuple_compact_percent(self.dead_tuple_compact_percent);
        for (k, v) in live {
            compacted.put(&k, &v)?;
        }

        self.data = compacted.data;
        Ok(())
    }

    fn maybe_compact_dead_tuples(&mut self) -> Result<(), &'static str> {
        if self.should_compact_dead_tuples() {
            self.compact_live_tuples()?;
        }
        Ok(())
    }

    /// Performs basic sanity checks: valid page magic and valid free-space bounds.
    pub fn validate_basic(&self) -> Result<(), &'static str> {
        if read_u32(&self.data, HDR_MAGIC_OFF) != PAGE_MAGIC {
            return Err("bad magic");
        }
        if self.free_start() > self.free_end() {
            return Err("free_start > free_end");
        }
        if self.dead_tuple_bytes() as usize > self.tuple_region_used_bytes() {
            return Err("dead_tuple_bytes > tuple_region_used_bytes");
        }
        Ok(())
    }

    // -----------------------------
    // Slot array (u16 offsets)
    // -----------------------------
    /// Computes the byte offset of slot index `i` within the page.
    fn slot_byte_off(i: usize) -> usize {
        HEADER_SIZE + i * Self::SLOT_SIZE
    }

    /// Reads slot `i` and returns the tuple byte offset that slot points to.
    pub fn read_slot(&self, i: usize) -> u16 {
        read_u16(&self.data, Self::slot_byte_off(i))
    }

    /// Writes a tuple's byte offset into slot `i`. This is how a slot "points" to a tuple.
    pub fn write_slot(&mut self, i: usize, tuple_off: u16) {
        write_u16(&mut self.data, Self::slot_byte_off(i), tuple_off);
    }

    /// Allocates one new slot, updates header counters, and returns the new slot index.
    pub fn alloc_slot(&mut self) -> Result<usize, &'static str> {
        let cnt = self.slot_count() as usize;
        let new_cnt = cnt + 1;
        let new_free_start = HEADER_SIZE + new_cnt * Self::SLOT_SIZE;

        if new_free_start > self.free_end() as usize {
            return Err("no space for slot");
        }

        write_u16(&mut self.data, HDR_SLOT_CNT_OFF, new_cnt as u16);
        write_u16(&mut self.data, HDR_FREE_START_OFF, new_free_start as u16);

        Ok(cnt)
    }
    // -----------------------------
    // Tuple format (self-describing)
    // -----------------------------
    //
    // Tuple bytes:
    //   tombstone: u8   (0 = live, 1 = deleted)
    //   key_len:  u16
    //   val_len:  u16
    //   key:      [u8; key_len]
    //   val:      [u8; val_len]
    //
    // total_len = 1 + 2 + 2 + key_len + val_len2;

    /// Returns total tuple bytes needed for given key/value lengths.
    pub fn tuple_len(key_len: usize, val_len: usize) -> usize {
        Self::TUP_HDR_SIZE + key_len + val_len
    }

    /// Reserves `len` bytes from the tuple region (growing from the page end) and returns start offset.
    pub fn alloc_tuple(&mut self, len: usize) -> Result<u16, &'static str> {
        let fe = self.free_end() as usize;
        let fs = self.free_start() as usize;

        if len > fe - fs {
            return Err("no space for tuple");
        }

        let new_fe = fe - len;
        write_u16(&mut self.data, HDR_FREE_END_OFF, new_fe as u16);
        Ok(new_fe as u16)
    }

    /// Writes one tuple at `off`: tombstone flag, key length, value length, key bytes, and value bytes.
    pub fn write_tuple(&mut self, off: u16, tombstone: u8, key: &[u8], val: &[u8]) {
        let o = off as usize;
        self.data[o] = tombstone;
        write_u16(&mut self.data, o + 1, key.len() as u16);
        write_u16(&mut self.data, o + 3, val.len() as u16);

        let ks = o + Self::TUP_HDR_SIZE;
        self.data[ks..ks + key.len()].copy_from_slice(key);

        let vs = ks + key.len();
        self.data[vs..vs + val.len()].copy_from_slice(val);
    }

    /// Returns a slice to the key bytes of the tuple at `off`.
    pub fn read_key<'a>(&'a self, off: usize) -> &'a [u8] {
        let klen = read_u16(&self.data, off + 1) as usize;
        let start = off + Self::TUP_HDR_SIZE;
        &self.data[start..start + klen]
    }

    /// Returns the full raw page bytes (useful for I/O).
    pub fn as_bytes(&self) -> &[u8; PAGE_SIZE] {
        &self.data
    }

    // Tuple header size: tombstone (1) + key_len (2) + val_len (2)
    const TUP_HDR_SIZE: usize = 1 + 2 + 2;

    #[inline]
    /// Reads the tombstone flag at tuple offset `off` (`0` live, `1` deleted).
    pub fn read_tuple_tombstone(&self, off: usize) -> u8 {
        self.data[off]
    }

    #[inline]
    /// Reads key length from tuple header at `off`.
    pub fn read_tuple_key_len(&self, off: usize) -> usize {
        read_u16(&self.data, off + 1) as usize
    }

    #[inline]
    /// Reads value length from tuple header at `off`.
    pub fn read_tuple_val_len(&self, off: usize) -> usize {
        read_u16(&self.data, off + 3) as usize
    }

    /// Returns a slice to the value bytes of the tuple at `off`.
    pub fn read_tuple_val<'a>(&'a self, off: usize) -> &'a [u8] {
        let klen = self.read_tuple_key_len(off);
        let vlen = self.read_tuple_val_len(off);
        let start = off + Self::TUP_HDR_SIZE + klen;
        &self.data[start..start + vlen]
    }

    /////////// The REAL PAGE APIS/////////////////////////////////////
    /// Binary-searches sorted slots for `key`.
    /// Returns `Found(i)` if present, otherwise `NotFound(insertion_pos)`.
    fn find_slot(&self, key: &[u8]) -> SearchResult {
        let search_key = Key::from(key);

        let mut lo = 0usize;
        let mut hi = self.slot_count() as usize;

        while lo < hi {
            let mid = (lo + hi) / 2;
            let tuple_off = self.read_slot(mid) as usize;

            let tuple = Tuple::from(&self.data[tuple_off..]);
            let mid_key = tuple.key();

            match mid_key.cmp(&search_key) {
                std::cmp::Ordering::Equal => return SearchResult::Found(mid),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }

        SearchResult::NotFound(lo)
    }
    
    

    /// Looks up `key` and returns value bytes if found and not tombstoned.
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        match self.find_slot(key) {
            SearchResult::Found(i) => {
                let off = self.read_slot(i) as usize;
                if self.read_tuple_tombstone(off) == 1 {
                    None
                } else {
                    Some(self.read_tuple_val(off))
                }
            }
            SearchResult::NotFound(_) => None,
        }
    }

 /**   Inserts or updates a key-value pair.
    • If key already exists: tombstones the old tuple, writes a new tuple with the new value, updates the slot to point to the new tuple. Returns the old value.
    • If key is new: allocates space for the tuple, writes it, allocates a new slot, and shifts existing slots right to insert the new slot at the correct sorted position. Returns None.
    • Fails with "page full" if there isn't enough free space.*/
    pub fn put(&mut self, key: &[u8], val: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        // Step 1: Build the tuple off-page
        let new_tuple = TupleBuilder::new()
            .flags(0) // live
            .key(key)
            .value(val)
            .build();

        let tuple_bytes = new_tuple.as_bytes();

        match self.find_slot(key) {
            // ==========================
            // Case 1: overwrite existing
            // ==========================
            SearchResult::Found(i) => {
                let needed = tuple_bytes.len();
                if self.free_space_bytes() < needed {
                    self.maybe_compact_dead_tuples()?;
                    if self.free_space_bytes() < needed {
                        return Err("page full");
                    }
                }
                let old_off = self.read_slot(i) as usize;

                // Read old tuple (zero-copy view)
                let old_tuple = Tuple::from(&self.data[old_off..]);

                // Capture old value
                let old_val = old_tuple.bytes().to_vec();

                // Tombstone old tuple (flag byte)
                self.data[old_off] = 1;
                self.add_dead_tuple_bytes(self.tuple_total_len(old_off));

                // Allocate space for new tuple
                let new_off = self.alloc_tuple(tuple_bytes.len())? as usize;

                // Copy new tuple bytes into page
                self.data[new_off..new_off + tuple_bytes.len()]
                    .copy_from_slice(tuple_bytes);

                // Update slot to point to new tuple
                self.write_slot(i, new_off as u16);
                self.maybe_compact_dead_tuples()?;

                Ok(Some(old_val))
            }

            // ==========================
            // Case 2: insert new key
            // ==========================
            SearchResult::NotFound(pos) => {
                let needed = tuple_bytes.len() + Self::SLOT_SIZE;
                if self.free_space_bytes() < needed {
                    self.maybe_compact_dead_tuples()?;
                    if self.free_space_bytes() < needed {
                        return Err("page full");
                    }
                }
                // Allocate tuple space
                let new_off = self.alloc_tuple(tuple_bytes.len())? as usize;

                // Copy tuple bytes
                self.data[new_off..new_off + tuple_bytes.len()]
                    .copy_from_slice(tuple_bytes);

                // Allocate slot
                let slot_idx = self.alloc_slot()?;

                // Shift slots right to make space
                for i in (pos..slot_idx).rev() {
                    let v = self.read_slot(i);
                    self.write_slot(i + 1, v);
                }

                // Insert new slot
                self.write_slot(pos, new_off as u16);

                Ok(None)
            }
        }
    }
   /** Deletes a key-value pair.
    • Tombstones the tuple (marks it deleted in-place).
    • Shifts all slots left to fill the gap left by the removed slot.
    • Updates the slot count and free_start in the header.
    • Returns the old value, or None if the key wasn't found.*/
    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        match self.find_slot(key) {
            SearchResult::Found(i) => {
                let off = self.read_slot(i) as usize;

                // capture old value
                let old_val = self.read_tuple_val(off).to_vec();

                // tombstone tuple
                self.data[off] = 1;
                self.add_dead_tuple_bytes(self.tuple_total_len(off));

                // remove slot by shifting left
                let cnt = self.slot_count() as usize;
                for j in i + 1..cnt {
                    let v = self.read_slot(j);
                    self.write_slot(j - 1, v);
                }

                // update header
                let new_cnt = cnt - 1;
                write_u16(&mut self.data, HDR_SLOT_CNT_OFF, new_cnt as u16);
                write_u16(
                    &mut self.data,
                    HDR_FREE_START_OFF,
                    (HEADER_SIZE + new_cnt * Self::SLOT_SIZE) as u16,
                );
                let _ = self.maybe_compact_dead_tuples();

                Some(old_val)
            }
            SearchResult::NotFound(_) => None,
        }
    }

    pub fn get_key_value_at_slot(&self, slot_idx: usize) -> Option<(&[u8], &[u8])> {
        if slot_idx >= self.slot_count() as usize {
            return  None;
        }
        let off = self.read_slot(slot_idx) as usize;

        if self.read_tuple_tombstone(off) == 1 {
            return None;
        }
        Some((self.read_key(off), self.read_tuple_val(off)))
    }

}

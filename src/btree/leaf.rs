use crate::page::Page;

pub struct LeafPage {
    page: Page,
}

pub struct LeafPageMut<'a> {
    page: &'a mut Page,
}

pub struct LeafSplit {
    pub separator_key: Vec<u8>,
    pub right_page: LeafPage
}

impl LeafPage {
    
    pub fn new (page_id: u32) -> Self {
        Self {
            page: Page::new(page_id),
        }
    }
    
    pub fn from_page(p: Page) -> Self {
        Self {
            page: p,
        }
    }
    
    pub fn into_page(self) -> Page {
        self.page
    }
    
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.page.get(key)
    }
    
    pub fn slot_count(&self) -> u16 {
        self.page.slot_count()
    }
    
    pub fn free_space_byte(&self) -> usize {
        self.page.free_space_bytes()
    }
    pub  fn key_val_at(&self, slot_id: usize) -> Option<(&[u8],&[u8])> {
        self.page.get_key_value_at_slot(slot_id)
    } 
    
    // Helper function for splitting
    
    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::with_capacity(self.slot_count() as usize);
        for i in 0..self.slot_count() as usize {
           if let Some((k,v)) = self.key_val_at(i) {
               out.push((k.to_vec(), v.to_vec()));
           } 
        }
        out
    
    }
}

impl<'a> LeafPageMut<'a> {
    pub fn new(page: &'a mut Page) -> Self {
        Self { page }
    }

    pub fn page(&self) -> &Page {
        self.page
    }

    pub fn page_mut(&mut self) -> &mut Page {
        self.page
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.page.get(key)
    }

    pub fn insert(&mut self, key: &[u8], val: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        self.page.put(key, val)
    }

    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.page.remove(key)
    }

    pub fn slot_count(&self) -> u16 {
        self.page.slot_count()
    }

    pub fn free_space_bytes(&self) -> usize {
        self.page.free_space_bytes()
    }
    pub fn key_value_at(&self, slot_idx: usize) -> Option<(&[u8], &[u8])> {
        self.page.get_key_value_at_slot(slot_idx)
    }

    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::with_capacity(self.slot_count() as usize);

        for i in 0..self.slot_count() as usize {
            if let Some((k, v)) = self.key_value_at(i) {
                out.push((k.to_vec(), v.to_vec()));
            }
        }

        out
    }
    
    
}
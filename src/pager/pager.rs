use std::collections::HashMap;

use crate::page::page::Page;

#[derive(Debug)]
pub struct Pager {
    pages: HashMap<u32, Page>,
    next_page_id: u32,
}

impl Pager {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            next_page_id: 1,
        }
    }

    pub fn alloc_page_id(&mut self) -> u32 {
        let page_id = self.next_page_id;
        self.next_page_id += 1;
        page_id
    }

    pub fn new_page(&mut self) -> u32 {
        let page_id = self.alloc_page_id();
        let page = Page::new(page_id);
        self.pages.insert(page_id, page);
        page_id
    }

    pub fn insert_page(&mut self, page: Page) {
        let page_id = page.page_id();
        self.pages.insert(page_id, page);
    }

    pub fn get(&self, page_id: u32) -> Option<&Page> {
        self.pages.get(&page_id)
    }

    pub fn get_mut(&mut self, page_id: u32) -> Option<&mut Page> {
        self.pages.get_mut(&page_id)
    }

    pub fn contains(&self, page_id: u32) -> bool {
        self.pages.contains_key(&page_id)
    }
}

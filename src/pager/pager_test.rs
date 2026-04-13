use crate::page::Page;
use crate::pager::pager::Pager;

#[test]
fn new_page_is_stored_in_pager() {
    let mut pager = Pager::new();
    let page_id = pager.new_page();

    let page = pager.get(page_id).expect("page should exist");
    assert_eq!(page.page_id(), page_id);
}

#[test]
fn insert_existing_page() {
    let mut pager = Pager::new();

    let page = Page::new(42);
    pager.insert_page(page);

    assert!(pager.contains(42));
    assert_eq!(pager.get(42).unwrap().page_id(), 42);
}

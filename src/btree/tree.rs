use crate::pager::pager::Pager;

pub struct BTree {
    root_page_id: u32,
    pager: Pager,
}

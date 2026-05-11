use super::internal::{InternalPage, InternalPageMut};
use crate::page::Page;

fn sep_key(i: usize, len: usize) -> Vec<u8> {
    let mut k = format!("sep_{i:04}").into_bytes();
    k.resize(len, 0);
    k
}

fn split_with_total_entries(
    total_entries: usize,
    key_len: usize,
) -> (InternalPage, InternalPage, Vec<u8>) {
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(1000));
    let mut internal = InternalPageMut::new(&mut page);

    for i in 0..(total_entries - 1) {
        let k = sep_key(i, key_len);
        let child = (i + 1) as u32;
        assert!(internal.insert(&k, child).is_ok(), "failed to seed entry {i}");
    }

    let split_key = sep_key(total_entries - 1, key_len);
    let split_child = total_entries as u32;
    let split = internal
        .insert_or_split(&split_key, split_child, 2)
        .unwrap()
        .expect("expected a split");

    let left = InternalPage::from_page(internal.page().clone());
    (left, split.right_page, split.separator_key)
}

#[test]
fn set_leftmost_child_round_trip() {
    let mut page = Page::new(1);
    let mut node = InternalPageMut::new(&mut page);

    node.set_leftmost_child(42);
    assert_eq!(node.leftmost_child(), 42);

    node.set_leftmost_child(99);
    assert_eq!(node.leftmost_child(), 99);
}

#[test]
fn insert_fits_without_split() {
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(0));
    let mut node = InternalPageMut::new(&mut page);

    for i in 0..5 {
        let k = sep_key(i, 1500);
        let result = node.insert(&k, (i + 1) as u32);
        assert!(result.is_ok(), "insert {i} should succeed");
    }

    assert_eq!(node.slot_count(), 5);
}

#[test]
fn find_child_routes_to_leftmost() {
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(100));
    let mut node = InternalPageMut::new(&mut page);

    node.insert(b"m", 200).unwrap();
    node.insert(b"z", 300).unwrap();

    assert_eq!(node.find_child(b"a"), 100);
    assert_eq!(node.find_child(b"l"), 100);
}

#[test]
fn find_child_routes_to_middle_child() {
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(100));
    let mut node = InternalPageMut::new(&mut page);

    node.insert(b"m", 200).unwrap();
    node.insert(b"z", 300).unwrap();

    assert_eq!(node.find_child(b"m"), 200);
    assert_eq!(node.find_child(b"r"), 200);
}

#[test]
fn find_child_routes_to_rightmost() {
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(100));
    let mut node = InternalPageMut::new(&mut page);

    node.insert(b"m", 200).unwrap();
    node.insert(b"z", 300).unwrap();

    assert_eq!(node.find_child(b"z"), 300);
    assert_eq!(node.find_child(b"zzz"), 300);
}

#[test]
fn insert_or_split_odd_total_entries() {
    // key_len=2000: 4 entries fit, 5th triggers split
    // all=[0..4], mid=2, left=[0,1], pushed=2, right=[3,4]
    let (left, right, sep) = split_with_total_entries(5, 2000);

    assert_eq!(left.slot_count(), 2, "left should have 2 separators");
    assert_eq!(right.slot_count(), 2, "right should have 2 separators");
    assert_eq!(sep, sep_key(2, 2000), "pushed-up key should be the middle entry");
}

#[test]
fn insert_or_split_even_total_entries() {
    // key_len=1500: 5 entries fit, 6th triggers split
    // all=[0..5], mid=3, left=[0,1,2], pushed=3, right=[4,5]
    let (left, right, sep) = split_with_total_entries(6, 1500);

    assert_eq!(left.slot_count(), 3, "left should have 3 separators");
    assert_eq!(right.slot_count(), 2, "right should have 2 separators");
    assert_eq!(sep, sep_key(3, 1500), "pushed-up key should be the middle entry");
}

#[test]
fn insert_or_split_no_key_loss() {
    let total = 6;
    let key_len = 1500;

    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(1000));
    let mut internal = InternalPageMut::new(&mut page);

    let expected: Vec<(Vec<u8>, u32)> = (0..total)
        .map(|i| (sep_key(i, key_len), (i + 1) as u32))
        .collect();

    for (k, c) in expected.iter().take(total - 1) {
        internal.insert(k, *c).unwrap();
    }

    let (last_key, last_child) = expected.last().unwrap().clone();
    let split = internal
        .insert_or_split(&last_key, last_child, 2)
        .unwrap()
        .expect("expected split");

    let left = InternalPage::from_page(internal.page().clone());
    let right = split.right_page;
    let pushed_sep = split.separator_key;

    // Recover all entries: left entries + (pushed_sep → right.leftmost_child) + right entries
    let mut recovered: Vec<(Vec<u8>, u32)> = Vec::new();
    recovered.extend(left.entries());
    recovered.push((pushed_sep, right.leftmost_child()));
    recovered.extend(right.entries());

    recovered.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(recovered.len(), expected.len(), "no entry should be lost or duplicated");
    assert_eq!(recovered, expected, "all (separator, child) pairs should be preserved");
}

#[test]
fn insert_or_split_right_leftmost_child_is_correct() {
    // total=5, key_len=2000: mid=2, all[2]=(sep_key(2), child=3)
    // right.leftmost_child should be 3
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(1000));
    let mut internal = InternalPageMut::new(&mut page);

    for i in 0..4 {
        internal.insert(&sep_key(i, 2000), (i + 1) as u32).unwrap();
    }

    let split = internal
        .insert_or_split(&sep_key(4, 2000), 5, 2)
        .unwrap()
        .expect("expected split");

    assert_eq!(split.right_page.leftmost_child(), 3u32);
}

#[test]
fn insert_or_split_pushed_up_key_not_on_either_child() {
    let mut page = Page::new(1);
    page.set_next_leaf_page_id(Some(1000));
    let mut internal = InternalPageMut::new(&mut page);

    for i in 0..4 {
        internal.insert(&sep_key(i, 2000), (i + 1) as u32).unwrap();
    }

    let split = internal
        .insert_or_split(&sep_key(4, 2000), 5, 2)
        .unwrap()
        .expect("expected split");

    let pushed = &split.separator_key;
    let left = InternalPage::from_page(internal.page().clone());
    let right = split.right_page;

    let left_keys: Vec<Vec<u8>> = left.entries().into_iter().map(|(k, _)| k).collect();
    let right_keys: Vec<Vec<u8>> = right.entries().into_iter().map(|(k, _)| k).collect();

    assert!(
        !left_keys.contains(pushed),
        "pushed-up key should not appear in left page"
    );
    assert!(
        !right_keys.contains(pushed),
        "pushed-up key should not appear in right page"
    );
}

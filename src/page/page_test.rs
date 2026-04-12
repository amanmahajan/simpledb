use super::page::{Page, HEADER_SIZE, PAGE_SIZE};

#[test]
fn new_page_initializes_header_and_free_space() {
    let page = Page::new(7);

    assert_eq!(page.slot_count(), 0);
    assert_eq!(page.free_start() as usize, HEADER_SIZE + Page::SLOT_SIZE * 0);
    assert_eq!(page.free_end() as usize, PAGE_SIZE);
    assert!(page.validate_basic().is_ok());
}

#[test]
fn put_and_get_roundtrip_for_multiple_keys() {
    let mut page = Page::new(1);

    assert_eq!(page.put(b"k2", b"v2").unwrap(), None);
    assert_eq!(page.put(b"k1", b"v1").unwrap(), None);
    assert_eq!(page.put(b"k3", b"v3").unwrap(), None);

    assert_eq!(page.get(b"k1"), Some(&b"v1"[..]));
    assert_eq!(page.get(b"k2"), Some(&b"v2"[..]));
    assert_eq!(page.get(b"k3"), Some(&b"v3"[..]));
    assert_eq!(page.get(b"missing"), None);
    assert_eq!(page.slot_count(), 3);
}

#[test]
fn put_existing_key_updates_value() {
    let mut page = Page::new(2);

    assert_eq!(page.put(b"key", b"old").unwrap(), None);
    let old = page.put(b"key", b"new").unwrap();

    assert!(old.is_some());
    assert_eq!(page.get(b"key"), Some(&b"new"[..]));
    assert_eq!(page.slot_count(), 1);
}

#[test]
fn remove_existing_and_missing_keys() {
    let mut page = Page::new(3);

    page.put(b"a", b"1").unwrap();
    page.put(b"b", b"2").unwrap();
    page.put(b"c", b"3").unwrap();

    let removed = page.remove(b"b");
    assert_eq!(removed, Some(b"2".to_vec()));
    assert_eq!(page.get(b"b"), None);
    assert_eq!(page.slot_count(), 2);

    assert_eq!(page.remove(b"does-not-exist"), None);
}

#[test]
fn remove_triggers_compaction_when_dead_bytes_cross_threshold() {
    let mut page = Page::new(9);
    let big_val = vec![b'x'; 6000];

    page.put(b"victim", &big_val).unwrap();
    assert!((page.free_end() as usize) < 8 * 1024);

    let removed = page.remove(b"victim");
    assert_eq!(removed, Some(big_val));

    assert_eq!(page.slot_count(), 0);
    assert_eq!(page.dead_tuple_bytes(), 0);
    assert_eq!(page.free_end() as usize, PAGE_SIZE);
    assert_eq!(page.free_start() as usize, HEADER_SIZE);
    assert!(page.validate_basic().is_ok());
}

#[test]
fn overwrite_compacts_dead_tuples_and_keeps_latest_value() {
    let mut page = Page::new(10);
    let v1 = vec![1u8; 1000];
    let v2 = vec![2u8; 1000];
    let v3 = vec![3u8; 1000];
    let v4 = vec![4u8; 1000];

    page.put(b"k", &v1).unwrap();
    page.put(b"k", &v2).unwrap();
    page.put(b"k", &v3).unwrap();
    page.put(b"k", &v4).unwrap();

    assert_eq!(page.get(b"k"), Some(&v4[..]));
    assert_eq!(page.slot_count(), 1);
    assert_eq!(page.dead_tuple_bytes(), 0);
    assert!(page.validate_basic().is_ok());
}

#[test]
fn get_key_value_at_slot_returns_key_value_for_live_slot() {
    let mut page = Page::new(11);

    page.put(b"k2", b"v2").unwrap();
    page.put(b"k1", b"v1").unwrap();
    page.put(b"k3", b"v3").unwrap();

    assert_eq!(
        page.get_key_value_at_slot(0),
        Some((&b"k1"[..], &b"v1"[..]))
    );
    assert_eq!(
        page.get_key_value_at_slot(1),
        Some((&b"k2"[..], &b"v2"[..]))
    );
    assert_eq!(
        page.get_key_value_at_slot(2),
        Some((&b"k3"[..], &b"v3"[..]))
    );
}

#[test]
fn get_key_value_at_slot_returns_none_for_out_of_bounds_or_removed_slot() {
    let mut page = Page::new(12);

    page.put(b"a", b"1").unwrap();
    page.put(b"b", b"2").unwrap();

    assert_eq!(page.get_key_value_at_slot(2), None);

    page.remove(b"a");

    assert_eq!(page.get_key_value_at_slot(0), Some((&b"b"[..], &b"2"[..])));
    assert_eq!(page.get_key_value_at_slot(1), None);
}



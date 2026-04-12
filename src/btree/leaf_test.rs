use super::leaf::{LeafPage, LeafPageMut};
use crate::page::Page;
use std::collections::BTreeMap;

fn key(i: usize) -> Vec<u8> {
    format!("k{i:02}").into_bytes()
}

fn value(tag: u8, len: usize) -> Vec<u8> {
    vec![tag; len]
}

fn split_with_total_entries(total_entries: usize, value_len: usize) -> (LeafPage, LeafPage, Vec<u8>) {
    let mut page = Page::new(1);
    let mut leaf = LeafPageMut::new(&mut page);

    for i in 0..(total_entries - 1) {
        let k = key(i);
        let v = value(b'a' + i as u8, value_len);
        assert!(leaf.insert(&k, &v).is_ok(), "failed to seed entry {i}");
    }

    let split_key = key(total_entries - 1);
    let split_val = value(b'z', value_len);
    let split = leaf
        .insert_or_split(&split_key, &split_val, 2)
        .unwrap()
        .expect("expected insert_or_split to split");

    let left = LeafPage::from_page(leaf.page().clone());
    (left, split.right_page, split.separator_key)
}

fn page_entries(page: &LeafPage) -> Vec<(Vec<u8>, Vec<u8>)> {
    page.entries()
}

fn assert_split_invariants(
    left: &LeafPage,
    right: &LeafPage,
    separator_key: &[u8],
    expected: &[(Vec<u8>, Vec<u8>)],
) {
    let left_entries = page_entries(left);
    let right_entries = page_entries(right);
    let combined: Vec<(Vec<u8>, Vec<u8>)> = left_entries
        .iter()
        .cloned()
        .chain(right_entries.iter().cloned())
        .collect();

    assert!(!right_entries.is_empty(), "right page must contain entries");
    assert_eq!(separator_key, right_entries[0].0.as_slice());

    if let (Some((left_max, _)), Some((right_min, _))) = (left_entries.last(), right_entries.first())
    {
        assert!(
            left_max < right_min,
            "all left-side keys must be smaller than right-side keys"
        );
    }

    let actual_map: BTreeMap<Vec<u8>, Vec<u8>> = combined.iter().cloned().collect();
    let expected_map: BTreeMap<Vec<u8>, Vec<u8>> = expected.iter().cloned().collect();

    assert_eq!(
        combined.len(),
        expected.len(),
        "no key/value pair should be lost or duplicated"
    );
    assert_eq!(
        actual_map.len(),
        expected_map.len(),
        "keys should remain unique after split"
    );
    assert_eq!(
        actual_map, expected_map,
        "all keys and values should be preserved after split"
    );
}

#[test]
fn insert_or_split_preserves_invariants_for_odd_total_entries() {
    let expected: Vec<(Vec<u8>, Vec<u8>)> = (0..5)
        .map(|i| (key(i), value(if i == 4 { b'z' } else { b'a' + i as u8 }, 1700)))
        .collect();

    let (left, right, separator_key) = split_with_total_entries(5, 1700);

    assert_eq!(left.slot_count(), 2);
    assert_eq!(right.slot_count(), 3);
    assert_split_invariants(&left, &right, &separator_key, &expected);
}

#[test]
fn insert_or_split_preserves_invariants_for_even_total_entries() {
    let expected: Vec<(Vec<u8>, Vec<u8>)> = (0..6)
        .map(|i| (key(i), value(if i == 5 { b'z' } else { b'a' + i as u8 }, 1500)))
        .collect();

    let (left, right, separator_key) = split_with_total_entries(6, 1500);

    assert_eq!(left.slot_count(), 3);
    assert_eq!(right.slot_count(), 3);
    assert_split_invariants(&left, &right, &separator_key, &expected);
}

#[test]
fn insert_or_split_updates_existing_key_without_loss_or_duplication() {
    let mut page = Page::new(7);
    let mut leaf = LeafPageMut::new(&mut page);

    for i in 0..4 {
        let k = key(i);
        let v = value(b'a' + i as u8, 1700);
        leaf.insert(&k, &v).unwrap();
    }

    let updated_key = key(2);
    let updated_val = value(b'u', 1700);
    let split = leaf
        .insert_or_split(&updated_key, &updated_val, 8)
        .unwrap()
        .expect("expected update to split when page is full");

    let left = LeafPage::from_page(leaf.page().clone());
    let right = split.right_page;

    let mut expected: Vec<(Vec<u8>, Vec<u8>)> = (0..4)
        .map(|i| (key(i), value(b'a' + i as u8, 1700)))
        .collect();
    expected[2] = (updated_key.clone(), updated_val.clone());

    assert_split_invariants(&left, &right, &split.separator_key, &expected);

    let combined = left
        .entries()
        .into_iter()
        .chain(right.entries())
        .collect::<Vec<_>>();
    let occurrences = combined
        .iter()
        .filter(|(k, _)| k.as_slice() == updated_key.as_slice())
        .count();
    assert_eq!(
        occurrences, 1,
        "updated key should appear exactly once after split"
    );
}
use super::tree::BTree;

fn k(i: usize) -> Vec<u8> {
    format!("key_{i:04}").into_bytes()
}

fn small_val(i: usize) -> Vec<u8> {
    format!("val_{i:04}").into_bytes()
}

fn large_val(i: usize) -> Vec<u8> {
    vec![(i % 256) as u8; 1500]
}

// ── empty-tree edge cases ────────────────────────────────────────────────────

#[test]
fn get_from_empty_tree_returns_none() {
    let tree = BTree::new();
    assert!(tree.get(b"any").is_none());
}

#[test]
fn remove_from_empty_tree_returns_none() {
    let mut tree = BTree::new();
    assert!(tree.remove(b"any").is_none());
}

#[test]
fn scan_empty_tree_yields_nothing() {
    let tree = BTree::new();
    assert_eq!(tree.scan(None).count(), 0);
}

// ── single-entry operations ──────────────────────────────────────────────────

#[test]
fn insert_and_get_roundtrip() {
    let mut tree = BTree::new();
    tree.insert(b"hello", b"world").unwrap();
    assert_eq!(tree.get(b"hello"), Some(b"world".as_slice()));
}

#[test]
fn get_missing_key_returns_none() {
    let mut tree = BTree::new();
    tree.insert(b"hello", b"world").unwrap();
    assert!(tree.get(b"missing").is_none());
}

#[test]
fn insert_overwrites_existing_key() {
    let mut tree = BTree::new();
    tree.insert(b"key", b"old").unwrap();
    tree.insert(b"key", b"new").unwrap();
    assert_eq!(tree.get(b"key"), Some(b"new".as_slice()));
}

#[test]
fn remove_existing_key_returns_old_value() {
    let mut tree = BTree::new();
    tree.insert(b"key", b"val").unwrap();
    assert_eq!(tree.remove(b"key"), Some(b"val".to_vec()));
    assert!(tree.get(b"key").is_none());
}

#[test]
fn remove_missing_key_returns_none() {
    let mut tree = BTree::new();
    tree.insert(b"key", b"val").unwrap();
    assert!(tree.remove(b"missing").is_none());
}

// ── scan with no splits ──────────────────────────────────────────────────────

#[test]
fn scan_all_returns_entries_in_sorted_order() {
    let mut tree = BTree::new();
    let n = 10;
    for i in (0..n).rev() {
        tree.insert(&k(i), &small_val(i)).unwrap();
    }
    let got: Vec<Vec<u8>> = tree.scan(None).map(|(key, _)| key.to_vec()).collect();
    let expected: Vec<Vec<u8>> = (0..n).map(k).collect();
    assert_eq!(got, expected);
}

#[test]
fn scan_from_start_key_returns_gte_entries() {
    let mut tree = BTree::new();
    let n = 10;
    for i in 0..n {
        tree.insert(&k(i), &small_val(i)).unwrap();
    }
    let start = k(5);
    let got: Vec<Vec<u8>> = tree
        .scan(Some(&start))
        .map(|(key, _)| key.to_vec())
        .collect();
    let expected: Vec<Vec<u8>> = (5..n).map(k).collect();
    assert_eq!(got, expected);
}

#[test]
fn scan_from_key_between_existing_keys() {
    let mut tree = BTree::new();
    // Insert even keys only
    for i in (0..10usize).filter(|x| x % 2 == 0) {
        tree.insert(&k(i), &small_val(i)).unwrap();
    }
    // Start scan from an odd key (k(3) = "key_0003"), should start at k(4)
    let start = k(3);
    let got: Vec<Vec<u8>> = tree
        .scan(Some(&start))
        .map(|(key, _)| key.to_vec())
        .collect();
    let expected: Vec<Vec<u8>> = (0..10usize)
        .filter(|x| x % 2 == 0 && *x >= 4)
        .map(k)
        .collect();
    assert_eq!(got, expected);
}

// ── multi-page scenarios (large values force splits) ─────────────────────────

#[test]
fn insert_many_all_keys_accessible_after_splits() {
    let mut tree = BTree::new();
    let n = 50;
    for i in 0..n {
        tree.insert(&k(i), &large_val(i)).unwrap();
    }
    for i in 0..n {
        assert_eq!(
            tree.get(&k(i)).map(|v| v.to_vec()),
            Some(large_val(i)),
            "key {i} missing or wrong after splits"
        );
    }
}

#[test]
fn insert_reverse_order_scan_is_sorted() {
    let mut tree = BTree::new();
    let n = 50;
    for i in (0..n).rev() {
        tree.insert(&k(i), &large_val(i)).unwrap();
    }
    let got: Vec<Vec<u8>> = tree.scan(None).map(|(key, _)| key.to_vec()).collect();
    let expected: Vec<Vec<u8>> = (0..n).map(k).collect();
    assert_eq!(got, expected);
}

#[test]
fn scan_all_values_correct_after_splits() {
    let mut tree = BTree::new();
    let n = 50;
    for i in 0..n {
        tree.insert(&k(i), &large_val(i)).unwrap();
    }
    let got: Vec<(Vec<u8>, Vec<u8>)> = tree
        .scan(None)
        .map(|(key, val)| (key.to_vec(), val.to_vec()))
        .collect();
    let expected: Vec<(Vec<u8>, Vec<u8>)> = (0..n).map(|i| (k(i), large_val(i))).collect();
    assert_eq!(got, expected);
}

#[test]
fn remove_half_keys_after_splits() {
    let mut tree = BTree::new();
    let n = 50;
    for i in 0..n {
        tree.insert(&k(i), &large_val(i)).unwrap();
    }
    for i in (0..n).step_by(2) {
        tree.remove(&k(i));
    }
    for i in 0..n {
        if i % 2 == 0 {
            assert!(tree.get(&k(i)).is_none(), "key {i} should be removed");
        } else {
            assert!(tree.get(&k(i)).is_some(), "key {i} should still exist");
        }
    }
}

#[test]
fn scan_from_mid_after_splits() {
    let mut tree = BTree::new();
    let n = 50;
    for i in 0..n {
        tree.insert(&k(i), &large_val(i)).unwrap();
    }
    let start = k(25);
    let got: Vec<Vec<u8>> = tree
        .scan(Some(&start))
        .map(|(key, _)| key.to_vec())
        .collect();
    let expected: Vec<Vec<u8>> = (25..n).map(k).collect();
    assert_eq!(got, expected);
}

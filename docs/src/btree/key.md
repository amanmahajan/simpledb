# Key Abstraction

`Key<D>` is a thin newtype wrapper over any byte-like type. Its only job is to give raw byte data a meaningful ordering and display.

```rust
pub struct Key<D> {
    data: D,
}
```

## Why a newtype?

Rust's built-in `[u8]` slice comparison is already lexicographic, but wrapping keys in `Key<D>` makes intent explicit and keeps comparison logic in one place. If the ordering ever needs to change (e.g. to a numeric or collation-aware comparison), only this type needs to change.

## Ordering

```rust
impl<D: AsRef<[u8]>> Ord for Key<D> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.data.as_ref().cmp(other.data.as_ref())
    }
}
```

Bytes are compared left-to-right. Shorter keys that are a prefix of a longer key sort earlier (e.g. `"foo" < "foobar"`).

## Generic over the backing storage

`D` can be any type implementing `AsRef<[u8]>`:

| D | Typical use |
|---|-------------|
| `&[u8]` | Zero-copy view into page memory |
| `Vec<u8>` | Owned key |
| `&str` | String keys in tests |

## Accessors

```rust
key.bytes()  // &D — the raw backing data
key.len()    // usize — number of bytes
```

`Display` formats as `Key([104, 101, 108, 108, 111])` (raw byte array).

# Tuples

A tuple is the unit of data stored inside a page. It carries a small header (flags, key length, value length) followed by the raw key and value bytes. The same wire format is used both inside the page buffer and in the owned heap representation.

## Wire format

```
┌──────────┬─────────┬─────────┬──────────┬──────────┐
│  flags   │ key_len │ val_len │   key    │   value  │
│  (1 B)   │ (2 B LE)│ (2 B LE)│ (n bytes)│ (m bytes)│
└──────────┴─────────┴─────────┴──────────┴──────────┘
  byte 0     byte 1    byte 3    byte 5     byte 5+n
```

Total size = `5 + key_len + val_len`.

The `flags` byte is currently used as a tombstone flag by the `Page` layer (`0` = live, `1` = deleted). The tuple type itself treats it as an opaque `u8`.

## `Tuple<A>` — zero-copy generic view

```rust
pub struct Tuple<A> {
    pub data: A,
}
```

`Tuple<A>` works over any buffer type. Common variants:

| Type | Use |
|------|-----|
| `Tuple<&[u8]>` | Read-only view into page memory |
| `Tuple<&mut [u8]>` | Mutable view for in-place header edits |
| `Tuple<Vec<u8>>` | Owned tuple (aliased as `OwningTuple`) |

Accessors delegate to `TupleHeader`:

```rust
impl<A: AsRef<[u8]>> Tuple<A> {
    pub fn header(&self) -> TupleHeader<&[u8]> { ... }
    pub fn key(&self) -> Key<&[u8]> { ... }   // borrows key bytes
    pub fn bytes(&self) -> &[u8] { ... }       // everything after header
}
```

## `TupleHeader<A>`

Reads and writes the 5-byte header:

```rust
impl<A: AsRef<[u8]>> TupleHeader<A> {
    pub fn flags(&self) -> u8    { ... }
    pub fn key_len(&self) -> u16 { ... }  // little-endian u16
    pub fn value_len(&self) -> u16 { ... }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> TupleHeader<A> {
    pub fn set_flags(&mut self, flags: u8)       { ... }
    pub fn set_key_len(&mut self, len: u16)      { ... }
    pub fn set_value_len(&mut self, len: u16)    { ... }
}
```

## `TupleBuilder` — constructing owned tuples

```rust
let tuple = TupleBuilder::new()
    .flags(0)
    .key(b"hello")
    .value(b"world")
    .build(); // returns OwningTuple (= Tuple<Vec<u8>>)

let bytes = tuple.as_bytes(); // &[u8] ready to copy into a page
```

`TupleBuilder` allocates a `Vec<u8>`, writes the header fields in little-endian order, then appends key and value bytes.

## Conversion helpers

```rust
let owned: OwningTuple = TupleBuilder::new().key("k").value("v").build();

let view: Tuple<&[u8]>     = owned.to_ref();
let mut_view: Tuple<&mut [u8]> = owned.to_mut_ref();
let raw: Vec<u8>            = owned.into_vec();
```

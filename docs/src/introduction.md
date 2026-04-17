# Introduction

`simpledb` is a Rust project for hands-on exploration of database storage internals. Rather than wrapping an existing engine, it builds core primitives from scratch: fixed-size pages, slotted tuple storage, B-tree leaf pages, and a page manager.

The goal is clarity over completeness. Each layer is small enough to read in one sitting, so the design decisions are visible and the trade-offs are easy to reason about.

## What's implemented

| Component | Status |
|-----------|--------|
| 8 KB slotted page (`Page`) | Done |
| Tuple serialization (`Tuple`, `TupleBuilder`) | Done |
| B-tree leaf page + split (`LeafPage`, `LeafPageMut`) | Done |
| In-memory page manager (`Pager`) | Done |
| B-tree internal nodes + full tree | Not yet |
| Disk I/O | Not yet |

## Getting started

```bash
# Build
cargo build

# Run all tests
cargo test

# Run benchmarks
cargo bench
```

## Project layout

```
src/
  page/       8 KB slotted page
  btree/      Key, Tuple, Leaf, and Tree types
  pager/      In-memory page manager
  utils/      Byte I/O helpers and utilities
benches/      Criterion benchmarks
docs/         This book
```

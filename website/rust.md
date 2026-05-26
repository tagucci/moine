# Rust Usage

Add the root crate when you want to use mòine from Rust:

```bash
cargo add moine
```

Library-only users can omit the CLI support dependency:

```bash
cargo add moine --no-default-features
```

## Dictionary Bundles

Rust users load dictionary artifacts explicitly by path.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dictionary = moine::zh::load_bundle("/path/to/moine-cedict-20260520")?;

    assert_eq!(dictionary.distance("weishiji", "威士忌")?, 0);
    assert_eq!(dictionary.distance("布納哈奔", "布納哈本")?, 0);

    Ok(())
}
```

Japanese bundles use the matching `ja` module:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dictionary = moine::ja::load_bundle("/path/to/moine-unidic-cwj-202512")?;

    assert_eq!(dictionary.distance("もいにゃ", "モイニャ")?, 0);

    Ok(())
}
```

## Lower-Level Lattices

The root crate also exposes language-independent lattice APIs for lower-level
work.

```rust
use moine::{damerau_distance, distance, Lattice};

let left = Lattice::from_paths(["moine"]);
let right = Lattice::from_paths(["moinya"]);

assert_eq!(distance(&left, &right), 2);
assert_eq!(damerau_distance(&left, &Lattice::from_paths(["mione"])), 1);
```

Use the `try_from_*` constructors when paths come from external input and
invalid path sets should be reported as errors.

## Crate Docs

Detailed Rust API documentation belongs on docs.rs:

- [`moine`](https://docs.rs/moine): umbrella API and verified bundle loaders
- [`moine-core`](https://docs.rs/moine-core): language-independent lattice edit distance
- [`moine-ja`](https://docs.rs/moine-ja): Japanese kana, romaji, and UniDic adapter
- [`moine-zh`](https://docs.rs/moine-zh): Chinese pinyin and CC-CEDICT adapter
- [`moine-cli`](https://docs.rs/moine-cli): CLI implementation crate

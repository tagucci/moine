# moine-ja

`moine-ja` is the Japanese adapter for `moine`.

It converts kana, ASCII romaji, override dictionaries, UniDic-derived reading
artifacts, and SudachiDict-derived reading artifacts into romaji lattices that
can be scored by `moine-core`.

Most Rust users should depend on the umbrella `moine` crate and load verified
dictionary bundles through `moine::ja::load_bundle`. Use `moine-ja` directly
when building custom artifacts, inspecting reading paths, or integrating the
Japanese adapter without the umbrella crate.

Dictionary data is distributed separately from the Rust crate and carries its
own license metadata.

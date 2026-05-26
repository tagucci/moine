# moine-zh

`moine-zh` is the Chinese adapter for `moine`.

It indexes simplified and traditional Chinese forms with Mandarin pinyin
readings from CC-CEDICT-derived artifacts. The default public artifact view is
no-tone pinyin; tone-number pinyin is available for explicit tone-aware
artifacts.

Most Rust users should depend on the umbrella `moine` crate and load verified
dictionary bundles through `moine::zh::load_bundle`. Use `moine-zh` directly
when building custom artifacts, inspecting pinyin paths, or integrating the
Chinese adapter without the umbrella crate.

Cantonese, Jyutping, and non-Mandarin readings are outside the current scope.

# Contributing

Thank you for considering a contribution to `moine`.

`moine` is still pre-1.0. Please keep changes small, documented, and aligned
with the current public surface: reading-aware string comparison, explicit
dictionary artifact downloads, and conservative Rust/Python APIs.

Before opening a pull request:

- Run `cargo fmt --check` and `cargo test` for Rust changes.
- Build a local wheel and run pytest against that wheel for Python changes;
  see `docs/development.md` for the exact commands.
- Update README, `website/`, or `docs/` when public behavior changes.
- Do not commit local dictionary packages, generated release artifacts, or
  local scratch workspaces.

Dictionary artifacts are separate from the source package. Generated UniDic or
CC-CEDICT bundles must keep dictionary license and attribution metadata
separate from the `moine` source-code license.

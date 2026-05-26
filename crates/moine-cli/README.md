# moine-cli

`moine-cli` provides the command-line implementation used by the `moine`
binary.

End users normally install the umbrella crate:

```bash
cargo install moine
```

The CLI exposes runtime commands such as dictionary download/list/where and
language-specific comparison, plus maintainer commands for building, verifying,
and packaging dictionary artifacts.

Most library users should depend on `moine` or the lower-level adapter crates
instead of using `moine-cli` directly.

# moine-core

`moine-core` is the language-independent Lattice Path Edit Distance engine used
by `moine`.

It provides the `Lattice` DAG representation, exact Levenshtein-style distance
over lattice paths, lattice-aware Damerau-Levenshtein distance, thresholded
checks, trace reconstruction, and plain string helper functions.

Most users should depend on the umbrella `moine` crate. Use `moine-core`
directly when you already have reading paths or another language adapter and
want only the edit-distance algorithms.

Dictionary loading, Japanese/Chinese romanization, Python bindings, and the CLI
live in sibling crates.

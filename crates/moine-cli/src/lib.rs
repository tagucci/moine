//! Library entrypoint for the `moine` command-line interface.
//!
//! The published CLI crate primarily exposes the binary target. This library
//! target exists so integration tests and downstream wrappers can invoke the
//! same command dispatcher as the binary.

#![deny(missing_docs)]
#![allow(clippy::items_after_test_module)]

include!("main.rs");

/// Runs the `moine` CLI dispatcher using process arguments.
pub fn run_cli() {
    main();
}

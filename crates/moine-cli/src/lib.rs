//! Library entrypoint for the `moine` command-line interface.
//!
//! The published CLI crate primarily exposes the binary target. This library
//! target keeps the dispatcher testable while the binary remains a thin wrapper.

#![deny(missing_docs)]

mod archive;
mod args;
mod commands;

#[cfg(test)]
mod tests;

/// Runs the `moine` CLI dispatcher using process arguments.
pub fn run_from_env() {
    if let Err(err) = commands::run_from_env() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

/// Runs the `moine` CLI dispatcher with explicit arguments.
pub fn run_with_args<I, S>(args: I) -> Result<(), Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    commands::run_with_args(args)
}

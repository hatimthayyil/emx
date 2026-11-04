//! `emx` builds emx's Emacs and its Elisp packages from an emx checkout into
//! emx's own Nix profile, which the `emacs` and `emacsclient` launchers run.
//!
//! Exit status: 0 on success, 1 when a command fails, 2 on a usage error.

mod build;
mod checkout;
mod cli;
mod diff;
mod error;
mod manifest;
mod nix;
mod profile;
mod prompt;
mod sync;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::Error;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("emx: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Sync(args) => sync::run(cli.checkout, &args),
    }
}

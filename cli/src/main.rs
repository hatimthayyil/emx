//! `emx` builds emx's Emacs and its Elisp packages from the emx repo into
//! emx's own Nix profile, which the `emacs` and `emacsclient` launchers run,
//! upgrades those packages, and rolls the profile back.
//!
//! Exit status: 0 on success, 1 when a command fails, 2 on a usage error.

mod build;
mod cli;
mod daemon;
mod diff;
mod error;
mod manifest;
mod nix;
mod profile;
mod prompt;
mod rollback;
mod source;
mod sync;
mod upgrade;

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
        Command::Sync(args) => sync::run(cli.src, &args),
        Command::Upgrade(args) => upgrade::run(cli.src, &args),
        Command::Rollback => rollback::run(),
    }
}

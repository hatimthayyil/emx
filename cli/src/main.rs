//! `emx` builds emx's Emacs and its Elisp packages from the emx repo into
//! emx's own Nix profile, which the `emacs` and `emacsclient` launchers run,
//! upgrades those packages, and rolls the profile back. After each switch, it
//! reloads a running Emacs into the new generation.
//!
//! Exit status: 0 on success, 1 when a command fails, 2 on a usage error. A
//! command that switched the profile but couldn't reload the running Emacs
//! exits 1, and the profile stays switched.

mod build;
mod cli;
mod daemon;
mod diff;
mod error;
mod manifest;
mod nix;
mod profile;
mod prompt;
mod reload;
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

//! The command line: global options and one variant per subcommand.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Builds emx's Emacs and its Elisp packages into emx's Nix profile.
#[derive(Debug, Parser)]
#[command(name = "emx", version)]
pub struct Cli {
    /// The emx repo to build from.
    #[arg(long, global = true, env = "EMX_SRC", value_name = "DIR")]
    pub src: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build the emx repo's Emacs and packages, as its lock pins them, and make
    /// the result the current generation of emx's profile. The sync that creates
    /// the profile also starts the Emacs daemon, if its service is enabled; a
    /// later one reloads a running Emacs into the new generation.
    Sync(SyncArgs),

    /// Move packages in the emx repo's lock to their newest versions, then sync.
    /// The new pins stay uncommitted in lock/. Without package names, every
    /// package moves, ELPA packages included. A package whose source can't be
    /// reached keeps its pin: the rest still move, then emx names it and exits 1.
    Upgrade(UpgradeArgs),

    /// Return emx's profile to its previous generation, and reload a running
    /// Emacs into it. lock/ stays as it is.
    Rollback,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Switch to the new build without asking.
    #[arg(long, short)]
    pub yes: bool,

    /// Extra arguments for the Nix commands that fetch and build, given after
    /// `--` (for example `-- --max-jobs 2 --cores 4`).
    #[arg(last = true, value_name = "NIX_ARGS")]
    pub nix_args: Vec<OsString>,
}

#[derive(Debug, Args)]
pub struct UpgradeArgs {
    /// Git packages to move. ELPA packages move together, so they cannot be
    /// named.
    #[arg(value_name = "PACKAGE")]
    pub packages: Vec<String>,

    #[command(flatten)]
    pub sync: SyncArgs,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn sync_passes_arguments_after_double_dash_to_nix() {
        let cli = Cli::try_parse_from(["emx", "sync", "--", "--max-jobs", "2"]).unwrap();
        let Command::Sync(args) = cli.command else {
            panic!("parsed as another command");
        };
        assert_eq!(args.nix_args, ["--max-jobs", "2"]);
    }

    #[test]
    fn upgrade_takes_packages_and_arguments_for_nix() {
        let cli = Cli::try_parse_from([
            "emx",
            "upgrade",
            "gptel",
            "magit",
            "--yes",
            "--",
            "--max-jobs",
            "2",
        ])
        .unwrap();
        let Command::Upgrade(args) = cli.command else {
            panic!("parsed as another command");
        };
        assert_eq!(args.packages, ["gptel", "magit"]);
        assert!(args.sync.yes);
        assert_eq!(args.sync.nix_args, ["--max-jobs", "2"]);
    }
}

//! Every way an emx command can fail, each with a message that says what to do.

use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no emx repo given: pass --src DIR or set EMX_SRC")]
    NoSource,

    #[error("cannot open the emx repo {path}: {source}")]
    SourceUnreadable { path: PathBuf, source: io::Error },

    #[error("{path} is not an emx repo: it has no {missing}")]
    NotASource {
        path: PathBuf,
        missing: &'static str,
    },

    #[error("cannot locate emx's profile: set XDG_STATE_HOME or HOME to an absolute path")]
    NoStateHome,

    #[error("cannot create {path}: {source}")]
    CreateDir { path: PathBuf, source: io::Error },

    #[error("cannot read emx's profile {path}: {source}")]
    ReadProfile { path: PathBuf, source: io::Error },

    #[error("emx's profile {path} does not point to a Nix profile generation")]
    UnexpectedProfile { path: PathBuf },

    #[error("nix build succeeded but did not create emx's profile {path}")]
    ProfileNotCreated { path: PathBuf },

    #[error("nix build succeeded but did not link its result at {path}: {source}")]
    BuildNotLinked { path: PathBuf, source: io::Error },

    #[error("cannot run nix: it is not on PATH")]
    NixMissing,

    #[error("cannot run nix: {source}")]
    NixSpawn { source: io::Error },

    #[error("nix failed ({status}); its output is above")]
    NixFailed { status: ExitStatus },

    #[error("cannot run nom (nix-output-monitor): it is not on PATH")]
    NomMissing,

    #[error("cannot run nom (nix-output-monitor): {source}")]
    NomSpawn { source: io::Error },

    #[error("stdin is not a terminal, so emx cannot ask before switching: pass --yes")]
    NeedsYes,

    #[error("cannot read the answer: {source}")]
    Prompt { source: io::Error },

    #[error("cannot read {path}: {source}")]
    ReadLock { path: PathBuf, source: io::Error },

    #[error("cannot parse {path}: {source}")]
    ParseLock {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error(
        "{name} is an ELPA package, and those move together: \
         run `emx upgrade` with no package names"
    )]
    ArchivePackage { name: String },

    #[error("{name} is not locked: add it to packages.el, then run `nix run --impure .#lock`")]
    UnknownPackage { name: String },

    #[error(
        "could not refresh {}: pins stay as they were, and Nix's output above says why",
        names.join(", ")
    )]
    NotRefreshed { names: Vec<String> },

    #[error("emx's profile has no generation to return to: run `emx sync` first")]
    NoGeneration,

    #[error("not switched: {}", match generation {
        Some(number) => format!("emx's profile stays at generation {number}"),
        None => "emx's profile stays empty".to_owned(),
    })]
    Declined { generation: Option<u64> },
}

//! Builds a flake output into a temporary garbage collector root, so the
//! result stays in the store while emx decides what to do with it.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::nix::{self, Progress};

/// A built store path, kept alive by the root link until this is dropped.
#[derive(Debug)]
pub struct Build {
    root: PathBuf,
    store_path: PathBuf,
}

impl Build {
    pub fn store_path(&self) -> &Path {
        &self.store_path
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        // Nix forgets a root whose link is gone; a link left behind by a
        // crash is replaced by the next build.
        let _ = fs::remove_file(&self.root);
    }
}

/// Builds `installable` with `extra` arguments for `nix build`, linking the
/// result at `root`. The log goes through nix-output-monitor when stderr is a
/// terminal.
pub fn build(installable: &OsStr, root: &Path, extra: &[OsString]) -> Result<Build, Error> {
    let progress = if io::stderr().is_terminal() {
        Progress::Nom
    } else {
        Progress::Plain
    };
    let args = nix::build(installable, root, progress, extra);
    match progress {
        Progress::Plain => nix::run(&args)?,
        Progress::Nom => nix::run_with_nom(&args)?,
    }
    let store_path = fs::read_link(root).map_err(|source| Error::BuildNotLinked {
        path: root.to_owned(),
        source,
    })?;
    Ok(Build {
        root: root.to_owned(),
        store_path,
    })
}

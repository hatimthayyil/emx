//! The emx repo: the flake that emx's Emacs and packages are built from.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Files every emx repo has at its root.
const MARKERS: [&str; 2] = ["flake.nix", "packages.el"];

/// The emx repo, validated and with its path made absolute.
#[derive(Debug)]
pub struct Source {
    root: PathBuf,
}

impl Source {
    /// Opens the emx repo at `dir`, which comes from `--src` or `EMX_SRC`.
    pub fn open(dir: Option<PathBuf>) -> Result<Self, Error> {
        let dir = dir.ok_or(Error::NoSource)?;
        let root = dir
            .canonicalize()
            .map_err(|source| Error::SourceUnreadable { path: dir, source })?;
        for marker in MARKERS {
            if !root.join(marker).is_file() {
                return Err(Error::NotASource {
                    path: root,
                    missing: marker,
                });
            }
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The nested flake that pins every package.
    pub fn lock_dir(&self) -> PathBuf {
        self.root.join("lock")
    }

    /// The installable naming the flake output `output` of this repo.
    /// Nix reads a plain path as a Git flake when it is a Git work tree, so
    /// ignored files such as `ai/` never reach the store.
    pub fn installable(&self, output: &str) -> OsString {
        let mut installable = self.root.clone().into_os_string();
        installable.push("#");
        installable.push(output);
        installable
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn source_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for marker in MARKERS {
            fs::write(dir.path().join(marker), "").unwrap();
        }
        dir
    }

    #[test]
    fn requires_a_source() {
        assert!(matches!(Source::open(None), Err(Error::NoSource)));
    }

    #[test]
    fn rejects_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(matches!(
            Source::open(Some(missing)),
            Err(Error::SourceUnreadable { .. })
        ));
    }

    #[test]
    fn rejects_a_directory_without_packages_el() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        assert!(matches!(
            Source::open(Some(dir.path().to_owned())),
            Err(Error::NotASource {
                missing: "packages.el",
                ..
            })
        ));
    }

    #[test]
    fn names_an_output_of_the_absolute_source() {
        let dir = source_dir();
        let relative = dir.path().join("..").join(dir.path().file_name().unwrap());
        let source = Source::open(Some(relative)).unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(
            source.installable("emacs"),
            OsString::from(format!("{}#emacs", root.display()))
        );
    }
}

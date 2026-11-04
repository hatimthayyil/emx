//! emx's Nix profile: the link that the `emacs` and `emacsclient` launchers
//! run Emacs from. Each `emx sync` adds a generation, so earlier builds stay
//! available until they are deleted.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// emx's profile link, `$XDG_STATE_HOME/emx/profile`.
#[derive(Debug)]
pub struct Profile {
    link: PathBuf,
}

/// A generation of the profile: Nix's `profile-<number>-link` beside the
/// profile, which links to a store path.
#[derive(Debug, PartialEq, Eq)]
pub struct Generation {
    pub number: u64,
    pub store_path: PathBuf,
}

impl Profile {
    /// Locates the profile under `xdg_state_home`. As the XDG Base Directory
    /// specification requires, an unset, empty or relative value falls back to
    /// `$HOME/.local/state`. The launchers in nix/home-module.nix follow the
    /// same rule.
    pub fn locate(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Result<Self, Error> {
        let absolute =
            |value: Option<OsString>| value.map(PathBuf::from).filter(|p| p.is_absolute());
        let state = absolute(xdg_state_home)
            .or_else(|| absolute(home).map(|home| home.join(".local/state")))
            .ok_or(Error::NoStateHome)?;
        Ok(Self {
            link: state.join("emx").join("profile"),
        })
    }

    /// Locates the profile from this process's environment.
    pub fn from_env() -> Result<Self, Error> {
        Self::locate(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
    }

    pub fn link(&self) -> &Path {
        &self.link
    }

    /// Creates the directory that holds the profile and its generations.
    pub fn create_parent(&self) -> Result<(), Error> {
        let parent = self.parent();
        fs::create_dir_all(parent).map_err(|source| Error::CreateDir {
            path: parent.to_owned(),
            source,
        })
    }

    /// The generation the profile points to, or `None` before the first sync.
    pub fn current(&self) -> Result<Option<Generation>, Error> {
        let generation_link = match fs::read_link(&self.link) {
            Ok(target) => target,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(self.read_error(source)),
        };
        let number =
            self.generation_number(&generation_link)
                .ok_or_else(|| Error::UnexpectedProfile {
                    path: self.link.clone(),
                })?;
        let store_path = fs::read_link(self.parent().join(&generation_link))
            .map_err(|source| self.read_error(source))?;
        Ok(Some(Generation { number, store_path }))
    }

    fn parent(&self) -> &Path {
        self.link
            .parent()
            .expect("the profile link is always below a state directory")
    }

    /// Parses `<number>` from `profile-<number>-link`.
    fn generation_number(&self, generation_link: &Path) -> Option<u64> {
        if generation_link.parent() != Some(Path::new("")) {
            return None;
        }
        let name = generation_link.to_str()?;
        let profile = self.link.file_name()?.to_str()?;
        name.strip_prefix(profile)?
            .strip_prefix('-')?
            .strip_suffix("-link")?
            .parse()
            .ok()
    }

    fn read_error(&self, source: io::Error) -> Error {
        Error::ReadProfile {
            path: self.link.clone(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    fn os(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    #[test]
    fn lives_under_xdg_state_home() {
        let profile = Profile::locate(os("/state"), os("/home/u")).unwrap();
        assert_eq!(profile.link(), Path::new("/state/emx/profile"));
    }

    #[test]
    fn falls_back_to_home_when_xdg_state_home_is_unset_empty_or_relative() {
        for xdg in [None, os(""), os("state")] {
            let profile = Profile::locate(xdg, os("/home/u")).unwrap();
            assert_eq!(
                profile.link(),
                Path::new("/home/u/.local/state/emx/profile")
            );
        }
    }

    #[test]
    fn needs_an_absolute_state_directory() {
        assert!(matches!(
            Profile::locate(None, os("relative")),
            Err(Error::NoStateHome)
        ));
        assert!(matches!(
            Profile::locate(None, None),
            Err(Error::NoStateHome)
        ));
    }

    #[test]
    fn has_no_generation_before_the_first_sync() {
        let state = tempfile::tempdir().unwrap();
        let profile = Profile::locate(Some(state.path().into()), None).unwrap();
        assert_eq!(profile.current().unwrap(), None);
    }

    #[test]
    fn reads_the_current_generation() {
        let state = tempfile::tempdir().unwrap();
        let profile = Profile::locate(Some(state.path().into()), None).unwrap();
        profile.create_parent().unwrap();
        let dir = state.path().join("emx");
        symlink("/nix/store/abc-emacs", dir.join("profile-12-link")).unwrap();
        symlink("profile-12-link", profile.link()).unwrap();
        assert_eq!(
            profile.current().unwrap(),
            Some(Generation {
                number: 12,
                store_path: PathBuf::from("/nix/store/abc-emacs"),
            })
        );
    }

    #[test]
    fn rejects_a_link_that_is_not_a_generation() {
        let state = tempfile::tempdir().unwrap();
        let profile = Profile::locate(Some(state.path().into()), None).unwrap();
        profile.create_parent().unwrap();
        symlink("/nix/store/abc-emacs", profile.link()).unwrap();
        assert!(matches!(
            profile.current(),
            Err(Error::UnexpectedProfile { .. })
        ));
    }
}

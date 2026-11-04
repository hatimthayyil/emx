//! The emx checkout: the flake that emx's Emacs and packages are built from.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::error::Error;

/// Files every emx checkout has at its root.
const MARKERS: [&str; 2] = ["flake.nix", "packages.el"];

/// An emx checkout, validated and with its path made absolute.
#[derive(Debug)]
pub struct Checkout {
    root: PathBuf,
}

impl Checkout {
    /// Opens the checkout at `dir`, which comes from `--checkout` or
    /// `EMX_CHECKOUT`.
    pub fn open(dir: Option<PathBuf>) -> Result<Self, Error> {
        let dir = dir.ok_or(Error::NoCheckout)?;
        let root = dir
            .canonicalize()
            .map_err(|source| Error::CheckoutUnreadable { path: dir, source })?;
        for marker in MARKERS {
            if !root.join(marker).is_file() {
                return Err(Error::NotACheckout {
                    path: root,
                    missing: marker,
                });
            }
        }
        Ok(Self { root })
    }

    /// The installable naming the flake output `output` of this checkout.
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

    fn checkout_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for marker in MARKERS {
            fs::write(dir.path().join(marker), "").unwrap();
        }
        dir
    }

    #[test]
    fn requires_a_checkout() {
        assert!(matches!(Checkout::open(None), Err(Error::NoCheckout)));
    }

    #[test]
    fn rejects_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(matches!(
            Checkout::open(Some(missing)),
            Err(Error::CheckoutUnreadable { .. })
        ));
    }

    #[test]
    fn rejects_a_directory_without_packages_el() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        assert!(matches!(
            Checkout::open(Some(dir.path().to_owned())),
            Err(Error::NotACheckout {
                missing: "packages.el",
                ..
            })
        ));
    }

    #[test]
    fn names_an_output_of_the_absolute_checkout() {
        let dir = checkout_dir();
        let relative = dir.path().join("..").join(dir.path().file_name().unwrap());
        let checkout = Checkout::open(Some(relative)).unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(
            checkout.installable("emacs"),
            OsString::from(format!("{}#emacs", root.display()))
        );
    }
}

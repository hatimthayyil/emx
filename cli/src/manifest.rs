//! twist's manifest of a built Emacs: the store file `elisp-digest.json` that
//! the Emacs wrapper references, naming the Emacs it wraps and the store path
//! of every Elisp package.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The store name twist gives every manifest.
const FILE_NAME: &str = "elisp-digest.json";

/// What follows a package's store path in its manifest entry.
const SITE_LISP: &str = "/share/emacs/site-lisp";

/// The Emacs and packages of one build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The store path of the Emacs the wrapper runs.
    pub emacs: PathBuf,
    /// Each package's name, mapped to the store path that holds it.
    pub packages: BTreeMap<String, PathBuf>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    emacs_path: PathBuf,
    elisp_packages: BTreeMap<String, String>,
}

impl Manifest {
    /// Finds the manifest among `paths`, the closure of a built Emacs.
    pub fn find<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
        paths
            .into_iter()
            .find(|path| store_name(Path::new(path)) == Some(FILE_NAME))
    }

    /// Reads the manifest at `path`.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
        Self::parse(&text).map_err(|err| format!("cannot parse {}: {err}", path.display()))
    }

    fn parse(text: &str) -> Result<Self, serde_json::Error> {
        let document: Document = serde_json::from_str(text)?;
        let packages = document
            .elisp_packages
            .into_iter()
            .map(|(name, dir)| {
                let dir = dir.trim_end_matches('/');
                let path = dir.strip_suffix(SITE_LISP).unwrap_or(dir);
                (name, PathBuf::from(path))
            })
            .collect();
        Ok(Self {
            emacs: document.emacs_path,
            packages,
        })
    }
}

/// The name of a store path: what follows `<hash>-` in its file name.
pub fn store_name(path: &Path) -> Option<&str> {
    let file_name = path.file_name()?.to_str()?;
    let (hash, name) = file_name.split_once('-')?;
    (hash.len() == 32).then_some(name)
}

/// The version in the store name of package `name`'s `path`, which twist
/// names `<emacs>-<package>-<version>`.
pub fn version<'a>(name: &str, path: &'a Path) -> Option<&'a str> {
    let store_name = store_name(path)?;
    let marker = format!("-{name}-");
    let at = store_name.rfind(&marker)?;
    Some(&store_name[at + marker.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "pqamph3sm9w74z77jmxphj5bwisykb6x";

    fn store(name: &str) -> String {
        format!("/nix/store/{HASH}-{name}")
    }

    #[test]
    fn finds_the_manifest_in_a_closure() {
        let manifest = store("elisp-digest.json");
        let closure = [store("emacs"), manifest.clone(), store("elisp-packages")];
        assert_eq!(
            Manifest::find(closure.iter().map(String::as_str)),
            Some(manifest.as_str())
        );
        assert_eq!(Manifest::find([store("emacs").as_str()]), None);
    }

    #[test]
    fn reads_packages_as_their_store_paths() {
        let gptel = store("emacs-pgtk-31-1-gptel-0.9.9.6");
        let text = format!(
            r#"{{
              "configurationRevision": null,
              "emacsPath": "{emacs}",
              "executablePackages": [],
              "elispPackages": {{ "gptel": "{gptel}/share/emacs/site-lisp/" }}
            }}"#,
            emacs = store("emacs-pgtk-31.1"),
        );
        let manifest = Manifest::parse(&text).unwrap();
        assert_eq!(manifest.emacs, PathBuf::from(store("emacs-pgtk-31.1")));
        assert_eq!(
            manifest.packages,
            BTreeMap::from([("gptel".to_owned(), PathBuf::from(gptel))])
        );
    }

    #[test]
    fn reads_versions_from_store_names() {
        let gptel = store("emacs-pgtk-31-1-gptel-0.9.9.6");
        assert_eq!(version("gptel", Path::new(&gptel)), Some("0.9.9.6"));
        let quick = store("emacs-pgtk-31-1-gptel-quick-0.0.0");
        assert_eq!(version("gptel-quick", Path::new(&quick)), Some("0.0.0"));
        assert_eq!(version("magit", Path::new(&gptel)), None);
    }

    #[test]
    fn names_only_store_paths() {
        assert_eq!(store_name(Path::new(&store("emacs"))), Some("emacs"));
        assert_eq!(store_name(Path::new("/tmp/short-emacs")), None);
    }
}

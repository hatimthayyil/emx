//! `emx upgrade`: moves packages in the emx repo's `lock/` to their newest
//! versions, then syncs. The moved pins stay uncommitted in the work tree: a
//! commit keeps them, and `git restore lock` undoes them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::cli::UpgradeArgs;
use crate::daemon;
use crate::error::Error;
use crate::nix;
use crate::profile::Profile;
use crate::prompt;
use crate::source::Source;
use crate::sync::{self, Outcome};

/// The flake app that refreshes `lock/metadata.json` and leaves ELPA packages
/// where they are.
const LOCK_APP: &str = "lock";

/// The flake app that refreshes `lock/metadata.json` and moves every ELPA
/// package to its newest release.
const UPDATE_APP: &str = "update";

/// `lock/flake.lock`, whose root node has one input per git package.
#[derive(Deserialize)]
struct FlakeLock {
    nodes: Nodes,
}

#[derive(Deserialize)]
struct Nodes {
    root: Root,
    #[serde(flatten)]
    inputs: BTreeMap<String, Input>,
}

#[derive(Deserialize)]
struct Root {
    inputs: BTreeMap<String, IgnoredAny>,
}

#[derive(Deserialize)]
struct Input {
    original: Origin,
}

/// Where a flake input comes from.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Origin {
    Git {
        url: String,
    },
    Github {
        owner: String,
        repo: String,
    },
    #[serde(other)]
    Other,
}

impl Origin {
    /// Whether Nix means this origin when its log names `location`: a Git
    /// repository by its URL, a GitHub one by the API call for its newest
    /// commit.
    fn is_at(&self, location: &str) -> bool {
        match self {
            Self::Git { url } => url == location,
            Self::Github { owner, repo } => location.contains(&format!("/repos/{owner}/{repo}/")),
            Self::Other => false,
        }
    }
}

pub fn run(src: Option<PathBuf>, args: &UpgradeArgs) -> Result<(), Error> {
    let source = Source::open(src)?;
    let profile = Profile::from_env()?;
    prompt::ensure_answerable(args.sync.yes)?;
    check_packages(&source.lock_dir(), &args.packages)?;

    let nix_args = &args.sync.nix_args;
    eprintln!("emx: fetching the newest versions");
    let mut failed = nix::run_noting_failed_fetches(
        source.root(),
        &nix::update_inputs(&source.lock_dir(), &args.packages, nix_args),
    )?;
    let app = if args.packages.is_empty() {
        UPDATE_APP
    } else {
        LOCK_APP
    };
    // The app writes to `lock/` relative to where it runs.
    let synced = nix::run_noting_failed_fetches(
        source.root(),
        &nix::run_app(&source.installable(app), nix_args),
    )
    .and_then(|failed_by_app| {
        failed.extend(failed_by_app);
        sync::apply(&source, &profile, &args.sync)
    });

    match &synced {
        Ok(Outcome::Switched) => daemon::print_restart(),
        Ok(Outcome::Created | Outcome::Unchanged) => {}
        Err(_) => eprintln!(
            "emx: lock/ keeps the moved pins: `git restore lock` in {} undoes them",
            source.root().display()
        ),
    }
    if failed.is_empty() {
        return synced.map(|_| ());
    }
    let not_refreshed = Error::NotRefreshed {
        names: names_at(&source.lock_dir(), &failed)?,
    };
    match synced {
        Ok(_) => Err(not_refreshed),
        Err(err) => {
            eprintln!("emx: {not_refreshed}");
            Err(err)
        }
    }
}

/// The flake inputs fetched from each of `locations`. A location no input
/// comes from, such as a package archive, stands for itself.
fn names_at(lock_dir: &Path, locations: &BTreeSet<String>) -> Result<Vec<String>, Error> {
    let flake: FlakeLock = read_json(&lock_dir.join("flake.lock"))?;
    let mut names = BTreeSet::new();
    for location in locations {
        let mut inputs: Vec<String> = flake
            .nodes
            .inputs
            .iter()
            .filter(|(_, input)| input.original.is_at(location))
            .map(|(name, _)| name.clone())
            .collect();
        if inputs.is_empty() {
            inputs.push(location.clone());
        }
        names.extend(inputs);
    }
    Ok(names.into_iter().collect())
}

/// Refuses what `nix flake update` cannot move: ELPA packages are no flake
/// inputs, and Nix only warns about a name it doesn't know.
fn check_packages(lock_dir: &Path, packages: &[String]) -> Result<(), Error> {
    if packages.is_empty() {
        return Ok(());
    }
    let archive: BTreeMap<String, IgnoredAny> = read_json(&lock_dir.join("archive.lock"))?;
    let flake: FlakeLock = read_json(&lock_dir.join("flake.lock"))?;
    for name in packages {
        if archive.contains_key(name) {
            return Err(Error::ArchivePackage { name: name.clone() });
        }
        if !flake.nodes.root.inputs.contains_key(name) {
            return Err(Error::UnknownPackage { name: name.clone() });
        }
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Error> {
    let text = fs::read_to_string(path).map_err(|source| Error::ReadLock {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| Error::ParseLock {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("archive.lock"), r#"{"denote": {}}"#).unwrap();
        fs::write(
            dir.path().join("flake.lock"),
            r#"{"nodes": {"root": {"inputs": {"gptel": "gptel"}}}}"#,
        )
        .unwrap();
        dir
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn accepts_git_packages() {
        check_packages(lock_dir().path(), &names(&["gptel"])).unwrap();
    }

    #[test]
    fn needs_no_lock_files_without_names() {
        check_packages(Path::new("/missing"), &[]).unwrap();
    }

    #[test]
    fn refuses_an_elpa_package() {
        assert!(matches!(
            check_packages(lock_dir().path(), &names(&["gptel", "denote"])),
            Err(Error::ArchivePackage { name }) if name == "denote"
        ));
    }

    #[test]
    fn refuses_an_unknown_package() {
        assert!(matches!(
            check_packages(lock_dir().path(), &names(&["gptl"])),
            Err(Error::UnknownPackage { name }) if name == "gptl"
        ));
    }

    #[test]
    fn names_the_inputs_behind_failed_locations() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("flake.lock"),
            r#"{"nodes": {
                "root": {"inputs": {}},
                "forge": {"original": {"type": "github", "owner": "magit", "repo": "forge"}},
                "magit": {"original": {"type": "github", "owner": "magit", "repo": "magit"}},
                "ws-butler": {"original": {"type": "git", "url": "https://nongnu.git", "ref": "a"}},
                "yasnippet": {"original": {"type": "git", "url": "https://nongnu.git", "ref": "b"}},
                "local": {"original": {"type": "path", "path": "/src"}}
            }}"#,
        )
        .unwrap();
        let locations = [
            "https://api.github.com/repos/magit/magit/commits/HEAD",
            "https://elpa.gnu.org/packages/archive-contents",
            "https://nongnu.git",
        ]
        .map(str::to_owned);

        assert_eq!(
            names_at(dir.path(), &BTreeSet::from(locations)).unwrap(),
            [
                "https://elpa.gnu.org/packages/archive-contents",
                "magit",
                "ws-butler",
                "yasnippet",
            ]
        );
    }

    #[test]
    fn reports_an_unreadable_lock() {
        let dir = lock_dir();
        fs::write(dir.path().join("flake.lock"), "not json").unwrap();
        assert!(matches!(
            check_packages(dir.path(), &names(&["gptel"])),
            Err(Error::ParseLock { .. })
        ));
        fs::remove_file(dir.path().join("archive.lock")).unwrap();
        assert!(matches!(
            check_packages(dir.path(), &names(&["gptel"])),
            Err(Error::ReadLock { .. })
        ));
    }
}

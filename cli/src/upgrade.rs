//! `emx upgrade`: moves packages in the emx repo's `lock/` to their newest
//! versions, then syncs. The moved pins stay uncommitted in the work tree: a
//! commit keeps them, and `git restore lock` undoes them. Nix keeps a pin it
//! couldn't refresh without failing, so the upgrade then asks upstream itself
//! (upstream.rs) and fails naming what didn't reach upstream's newest.

use std::path::{Path, PathBuf};

use crate::cli::UpgradeArgs;
use crate::error::Error;
use crate::lock::{self, ArchiveLock, FlakeLock};
use crate::nix;
use crate::profile::Profile;
use crate::prompt;
use crate::source::Source;
use crate::sync;
use crate::upstream;

/// The flake app that refreshes `lock/metadata.json` and leaves ELPA packages
/// where they are.
const LOCK_APP: &str = "lock";

/// The flake app that refreshes `lock/metadata.json` and moves every ELPA
/// package to its newest release.
const UPDATE_APP: &str = "update";

pub fn run(src: Option<PathBuf>, args: &UpgradeArgs) -> Result<(), Error> {
    let source = Source::open(src)?;
    let profile = Profile::from_env()?;
    prompt::ensure_answerable(args.sync.yes)?;
    let lock_dir = source.lock_dir();
    check_packages(&lock_dir, &args.packages)?;

    let nix_args = &args.sync.nix_args;
    eprintln!("emx: fetching the newest versions");
    nix::run_in(
        source.root(),
        &nix::update_inputs(&lock_dir, &args.packages, nix_args),
    )?;
    let moves_archives = args.packages.is_empty();
    let app = if moves_archives { UPDATE_APP } else { LOCK_APP };
    // The app writes to `lock/` relative to where it runs.
    let locked = nix::run_in(
        source.root(),
        &nix::run_app(&source.installable(app), moves_archives, nix_args),
    );
    let report = compare_with_upstream(&lock_dir, &args.packages, locked.is_ok())?;
    let synced = locked.and_then(|()| sync::apply(&source, &profile, &args.sync));

    if synced.is_err() {
        eprintln!(
            "emx: lock/ keeps the moved pins: `git restore lock` in {} undoes them",
            source.root().display()
        );
    }
    let synced = synced.and_then(|outcome| sync::reload_if_switched(&profile, outcome));
    if report.is_current() {
        return synced;
    }
    let not_current = Error::NotCurrent(report);
    match synced {
        Ok(()) => Err(not_current),
        Err(err) => {
            eprintln!("emx: {not_current}");
            Err(err)
        }
    }
}

/// How the pins of the packages this upgrade moved compare with upstream:
/// the named git packages, or with no names every git package and, once the
/// update app has moved them, every ELPA package.
fn compare_with_upstream(
    lock_dir: &Path,
    packages: &[String],
    archives_moved: bool,
) -> Result<upstream::Report, Error> {
    let inputs: Vec<lock::Input> = FlakeLock::read(lock_dir)?
        .inputs()
        .into_iter()
        .filter(|input| packages.is_empty() || packages.contains(&input.name))
        .collect();
    let archive = if packages.is_empty() && archives_moved {
        lock::read_archive(lock_dir)?
    } else {
        ArchiveLock::new()
    };
    eprintln!("emx: checking each package against upstream");
    Ok(upstream::check(&inputs, &archive))
}

/// Refuses what `nix flake update` cannot move: ELPA packages are no flake
/// inputs, and Nix only warns about a name it doesn't know.
fn check_packages(lock_dir: &Path, packages: &[String]) -> Result<(), Error> {
    if packages.is_empty() {
        return Ok(());
    }
    let archive = lock::archive_names(lock_dir)?;
    let flake = FlakeLock::read(lock_dir)?;
    for name in packages {
        if archive.contains(name) {
            return Err(Error::ArchivePackage { name: name.clone() });
        }
        if !flake.has_input(name) {
            return Err(Error::UnknownPackage { name: name.clone() });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn lock_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("archive.lock"), r#"{"denote": {}}"#).unwrap();
        fs::write(
            dir.path().join("flake.lock"),
            r#"{"nodes": {"root": {"inputs": {"gptel": "gptel"}},
                "gptel": {"original": {"type": "github", "owner": "karthink", "repo": "gptel"}}}}"#,
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

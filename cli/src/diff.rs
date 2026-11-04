//! What a new build of emx's Emacs changes: its Elisp packages, read from
//! twist's manifests, and its closure, diffed by dix as nh shows it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use dix::{DiffReport, RenderOptions, Size, StoreSnapshot};
use yansi::Paint as _;

use crate::manifest::{self, Manifest};
use crate::nix;

/// Why a package with the same version has a new store path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// Its source moved, for example to a new commit without a version bump.
    Source,
    /// Same source, rebuilt because something it is built with changed.
    Rebuild,
    /// The derivations that would tell are not in the store.
    Unknown,
}

/// How one Elisp package differs between two builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added {
        version: Option<String>,
    },
    Removed {
        version: Option<String>,
    },
    Version {
        old: Option<String>,
        new: Option<String>,
    },
    Changed {
        version: Option<String>,
        cause: Cause,
    },
}

/// Prints what changes from the build at `old` to the build at `new`, both
/// store paths of emx's Emacs.
pub fn show(old: &Path, new: &Path) -> Result<(), String> {
    let store = dix::Store::open();
    let old_snapshot = snapshot(&store, old)?;
    let new_snapshot = snapshot(&store, new)?;
    let manifests =
        manifest_of(&old_snapshot).and_then(|old| Ok((old, manifest_of(&new_snapshot)?)));

    println!("{} {}", "<<<".bold(), old.display());
    println!("{} {}", ">>>".bold(), new.display());
    let elisp_paths = match &manifests {
        Ok((old_manifest, new_manifest)) => {
            let sources = sources(&same_version_pairs(old_manifest, new_manifest));
            print!(
                "{}",
                render_elisp(&elisp_changes(old_manifest, new_manifest, &sources))
            );
            package_names([old_manifest, new_manifest])
        }
        Err(err) => {
            eprintln!("emx: cannot list the Elisp packages that change: {err}");
            HashSet::new()
        }
    };
    let report = closure_report(&old_snapshot, &new_snapshot, &elisp_paths);
    print!("{}", render_closure(&report));
    Ok(())
}

/// Prints what the first build at `new` installs.
pub fn summarize(new: &Path) -> Result<(), String> {
    let snapshot = snapshot(&dix::Store::open(), new)?;
    let manifest = manifest_of(&snapshot)?;
    print!("{}", render_summary(new, &manifest, &snapshot));
    Ok(())
}

fn snapshot(store: &dix::Store, path: &Path) -> Result<StoreSnapshot, String> {
    store
        .query_snapshot_document(path)
        .map(|document| document.snapshot)
        .map_err(|err| format!("cannot read the closure of {}: {err}", path.display()))
}

fn manifest_of(snapshot: &StoreSnapshot) -> Result<Manifest, String> {
    let path = Manifest::find(snapshot.closure.iter().map(|info| info.path.as_str()))
        .ok_or("the build has no twist manifest (elisp-digest.json)")?;
    Manifest::read(Path::new(path))
}

/// The old and new store paths of packages whose version stayed the same but
/// whose store path changed: only their derivations tell why.
fn same_version_pairs<'a>(old: &'a Manifest, new: &'a Manifest) -> Vec<(&'a Path, &'a Path)> {
    old.packages
        .iter()
        .filter_map(|(name, old_path)| {
            let new_path = new.packages.get(name)?;
            (old_path != new_path
                && manifest::version(name, old_path) == manifest::version(name, new_path))
            .then_some((old_path.as_path(), new_path.as_path()))
        })
        .collect()
}

/// The derivation sources of every path in `pairs`, by store file name. Empty
/// when Nix cannot show the derivations, which leaves the cause unknown.
fn sources(pairs: &[(&Path, &Path)]) -> HashMap<String, PathBuf> {
    if pairs.is_empty() {
        return HashMap::new();
    }
    let paths: Vec<&Path> = pairs.iter().flat_map(|&(old, new)| [old, new]).collect();
    nix::output(&nix::show_derivations(&paths))
        .and_then(|json| nix::derivation_sources(&json).map_err(|err| err.to_string()))
        .unwrap_or_default()
}

/// How each package differs from `old` to `new`; unchanged packages are left
/// out. `sources` maps store file names, `<hash>-<name>`, to derivation sources.
pub fn elisp_changes(
    old: &Manifest,
    new: &Manifest,
    sources: &HashMap<String, PathBuf>,
) -> BTreeMap<String, Change> {
    let version = |name: &str, path: &Path| manifest::version(name, path).map(str::to_owned);
    let source = |path: &Path| sources.get(path.file_name()?.to_str()?);
    let mut changes = BTreeMap::new();
    for (name, old_path) in &old.packages {
        let change = match new.packages.get(name) {
            None => Change::Removed {
                version: version(name, old_path),
            },
            Some(new_path) if new_path == old_path => continue,
            Some(new_path) => {
                let (old_version, new_version) = (version(name, old_path), version(name, new_path));
                if old_version != new_version {
                    Change::Version {
                        old: old_version,
                        new: new_version,
                    }
                } else {
                    let cause = match (source(old_path), source(new_path)) {
                        (Some(old_src), Some(new_src)) if old_src == new_src => Cause::Rebuild,
                        (Some(_), Some(_)) => Cause::Source,
                        _ => Cause::Unknown,
                    };
                    Change::Changed {
                        version: new_version,
                        cause,
                    }
                }
            }
        };
        changes.insert(name.clone(), change);
    }
    for (name, new_path) in &new.packages {
        if !old.packages.contains_key(name) {
            let change = Change::Added {
                version: version(name, new_path),
            };
            changes.insert(name.clone(), change);
        }
    }
    changes
}

/// The store names of every package in `manifests`.
fn package_names<'a>(manifests: impl IntoIterator<Item = &'a Manifest>) -> HashSet<String> {
    manifests
        .into_iter()
        .flat_map(|manifest| manifest.packages.values())
        .filter_map(|path| manifest::store_name(path).map(str::to_owned))
        .collect()
}

/// dix's diff of the two closures. Its package list leaves out the Elisp
/// packages named in `elisp`, and their other outputs such as `-info`: twist
/// names them `<emacs>-<version>-<package>-<version>`, which dix would read as
/// one package, Emacs, at many versions. The path counts and sizes still cover
/// the whole closures.
pub fn closure_report(
    old: &StoreSnapshot,
    new: &StoreSnapshot,
    elisp: &HashSet<String>,
) -> DiffReport {
    let without_elisp = |snapshot: &StoreSnapshot| StoreSnapshot {
        closure: snapshot
            .closure
            .iter()
            .filter(|info| !is_elisp(info.path.as_str(), elisp))
            .cloned()
            .collect(),
        selected: snapshot.selected.clone(),
    };
    let whole = dix::diff_store_snapshots(old, new);
    let packages = dix::diff_store_snapshots(&without_elisp(old), &without_elisp(new));
    DiffReport {
        diffs: packages.diffs,
        ..whole
    }
}

fn is_elisp(path: &str, elisp: &HashSet<String>) -> bool {
    let Some(name) = manifest::store_name(Path::new(path)) else {
        return false;
    };
    elisp.contains(name)
        || name
            .rmatch_indices('-')
            .any(|(at, _)| elisp.contains(&name[..at]))
}

fn render_elisp(changes: &BTreeMap<String, Change>) -> String {
    let mut out = String::new();
    write_elisp(&mut out, changes).expect("writing to a String cannot fail");
    out
}

fn write_elisp(out: &mut String, changes: &BTreeMap<String, Change>) -> fmt::Result {
    let version = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".to_owned());
    let (rebuilt, listed): (Vec<_>, Vec<_>) = changes.iter().partition(|(_, change)| {
        matches!(
            change,
            Change::Changed {
                cause: Cause::Rebuild,
                ..
            }
        )
    });
    writeln!(out)?;
    writeln!(
        out,
        "{} {}",
        "ELISP PACKAGES".bold(),
        format!("({} changed)", changes.len()).dim()
    )?;
    if changes.is_empty() {
        return writeln!(out, "No Elisp package changed.");
    }
    let width = listed.iter().map(|(name, _)| name.len()).max().unwrap_or(0) + 2;
    for (name, change) in listed {
        let detail = match change {
            Change::Added { version: v } => format!("{} {}", "added".green(), version(v)),
            Change::Removed { version: v } => format!("{} {}", "removed".red(), version(v)),
            Change::Version { old, new } => format!("{} -> {}", version(old), version(new)),
            Change::Changed { version: v, cause } => {
                let cause = match cause {
                    Cause::Source => "new source".yellow().to_string(),
                    Cause::Rebuild => unreachable!("rebuilt packages are summarised"),
                    Cause::Unknown => "changed".yellow().to_string(),
                };
                format!("{}, {cause}", version(v))
            }
        };
        writeln!(out, "{name:<width$}{detail}")?;
    }
    if !rebuilt.is_empty() {
        let names: Vec<&str> = rebuilt.iter().map(|(name, _)| name.as_str()).collect();
        writeln!(
            out,
            "{} {}",
            format!("Rebuilt, as packages they use changed ({}):", names.len()).dim(),
            names.join(", ")
        )?;
    }
    Ok(())
}

fn render_closure(report: &DiffReport) -> String {
    let mut body = String::new();
    let hidden = dix::write_diff_report(&mut body, report, RenderOptions::default())
        .expect("writing to a String cannot fail");
    // dix opens its report with a blank line.
    let mut out = format!("\n{}\n{}", "CLOSURE".bold(), body.trim_start_matches('\n'));
    if hidden > 0 {
        let note = format!("{hidden} other packages changed only in size.");
        out.push_str(&format!("{}\n", note.dim()));
    }
    out
}

fn render_summary(path: &Path, manifest: &Manifest, snapshot: &StoreSnapshot) -> String {
    let size = Size::from_bytes(
        snapshot
            .closure
            .iter()
            .map(|info| info.nar_size.bytes())
            .sum::<i64>(),
    );
    let emacs = manifest::store_name(&manifest.emacs).unwrap_or("?");
    format!(
        "{} {}\n\
         {:<10}{emacs}\n\
         {:<10}{} Elisp packages\n\
         {:<10}{} store paths, {size}\n",
        ">>>".bold(),
        path.display(),
        "Emacs",
        "Packages",
        manifest.packages.len(),
        "Closure",
        snapshot.closure.len(),
    )
}

#[cfg(test)]
mod tests {
    use dix::{StorePath, StorePathInfo};

    use super::*;

    fn path(hash: char, name: &str) -> PathBuf {
        PathBuf::from(format!("/nix/store/{}-{name}", hash.to_string().repeat(32)))
    }

    fn manifest(packages: &[(&str, char, &str)]) -> Manifest {
        Manifest {
            emacs: path('e', "emacs-pgtk-31.1"),
            packages: packages
                .iter()
                .map(|&(name, hash, version)| {
                    let store = path(hash, &format!("emacs-pgtk-31-1-{name}-{version}"));
                    (name.to_owned(), store)
                })
                .collect(),
        }
    }

    fn store_name(path: &Path) -> String {
        manifest::store_name(path).unwrap().to_owned()
    }

    fn file_name(path: &Path) -> String {
        path.file_name().unwrap().to_str().unwrap().to_owned()
    }

    #[test]
    fn classifies_every_kind_of_change() {
        let old = manifest(&[
            ("same", 'a', "1.0"),
            ("bumped", 'b', "1.0"),
            ("moved", 'c', "2.0"),
            ("rebuilt", 'd', "3.0"),
            ("unknown", 'f', "4.0"),
            ("gone", 'g', "5.0"),
        ]);
        let new = manifest(&[
            ("same", 'a', "1.0"),
            ("bumped", 'h', "1.1"),
            ("moved", 'i', "2.0"),
            ("rebuilt", 'j', "3.0"),
            ("unknown", 'k', "4.0"),
            ("fresh", 'l', "6.0"),
        ]);
        let sources = HashMap::from([
            (
                file_name(&old.packages["moved"]),
                "/nix/store/old-src".into(),
            ),
            (
                file_name(&new.packages["moved"]),
                "/nix/store/new-src".into(),
            ),
            (file_name(&old.packages["rebuilt"]), "/nix/store/src".into()),
            (file_name(&new.packages["rebuilt"]), "/nix/store/src".into()),
        ]);
        let some = |v: &str| Some(v.to_owned());
        assert_eq!(
            elisp_changes(&old, &new, &sources),
            BTreeMap::from([
                (
                    "bumped".to_owned(),
                    Change::Version {
                        old: some("1.0"),
                        new: some("1.1")
                    }
                ),
                (
                    "moved".to_owned(),
                    Change::Changed {
                        version: some("2.0"),
                        cause: Cause::Source
                    }
                ),
                (
                    "rebuilt".to_owned(),
                    Change::Changed {
                        version: some("3.0"),
                        cause: Cause::Rebuild
                    }
                ),
                (
                    "unknown".to_owned(),
                    Change::Changed {
                        version: some("4.0"),
                        cause: Cause::Unknown
                    }
                ),
                (
                    "gone".to_owned(),
                    Change::Removed {
                        version: some("5.0")
                    }
                ),
                (
                    "fresh".to_owned(),
                    Change::Added {
                        version: some("6.0")
                    }
                ),
            ])
        );
    }

    #[test]
    fn asks_derivations_only_about_same_version_changes() {
        let old = manifest(&[
            ("same", 'a', "1"),
            ("bumped", 'b', "1"),
            ("moved", 'c', "2"),
        ]);
        let new = manifest(&[
            ("same", 'a', "1"),
            ("bumped", 'd', "2"),
            ("moved", 'e', "2"),
        ]);
        assert_eq!(
            same_version_pairs(&old, &new),
            [(
                old.packages["moved"].as_path(),
                new.packages["moved"].as_path()
            )]
        );
    }

    #[test]
    fn renders_listed_changes_and_summarises_rebuilds() {
        yansi::disable();
        let changes = BTreeMap::from([
            (
                "gptel".to_owned(),
                Change::Changed {
                    version: Some("0.9.9.6".into()),
                    cause: Cause::Source,
                },
            ),
            (
                "gptel-quick".to_owned(),
                Change::Changed {
                    version: Some("0.0.0".into()),
                    cause: Cause::Rebuild,
                },
            ),
            (
                "magit".to_owned(),
                Change::Version {
                    old: Some("4.7.1".into()),
                    new: Some("4.7.2".into()),
                },
            ),
        ]);
        assert_eq!(
            render_elisp(&changes),
            "\nELISP PACKAGES (3 changed)\n\
             gptel  0.9.9.6, new source\n\
             magit  4.7.1 -> 4.7.2\n\
             Rebuilt, as packages they use changed (1): gptel-quick\n"
        );
    }

    #[test]
    fn renders_the_closure_under_its_own_header() {
        yansi::disable();
        let snapshot = |bytes| StoreSnapshot {
            closure: vec![info(&path('a', "curl-8.0"), bytes)],
            selected: vec![],
        };
        let report = dix::diff_store_snapshots(&snapshot(1), &snapshot(2));
        let out = render_closure(&report);
        assert!(out.starts_with("\nCLOSURE\nPATHS: 1 -> 1"), "{out:?}");
    }

    fn info(path: &Path, bytes: i64) -> StorePathInfo {
        StorePathInfo {
            path: StorePath::try_from(path.to_str().unwrap().to_owned()).unwrap(),
            nar_size: Size::from_bytes(bytes),
        }
    }

    #[test]
    fn leaves_elisp_packages_out_of_dix_package_list_but_not_its_totals() {
        let gptel_old = path('a', "emacs-pgtk-31-1-gptel-1.0");
        let gptel_new = path('b', "emacs-pgtk-31-1-gptel-1.1");
        let info_old = path('c', "emacs-pgtk-31-1-gptel-1.0-info");
        let curl_old = path('d', "curl-8.0");
        let curl_new = path('f', "curl-8.1");
        let old = StoreSnapshot {
            closure: vec![
                info(&gptel_old, 10),
                info(&info_old, 1),
                info(&curl_old, 100),
            ],
            selected: vec![],
        };
        let new = StoreSnapshot {
            closure: vec![info(&gptel_new, 12), info(&curl_new, 100)],
            selected: vec![],
        };
        let elisp = HashSet::from([store_name(&gptel_old), store_name(&gptel_new)]);
        let report = closure_report(&old, &new, &elisp);
        let names: Vec<&str> = report.diffs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["curl"]);
        assert_eq!(report.path_stats.added, 2);
        assert_eq!(report.path_stats.removed, 3);
        assert_eq!(report.size_old, Size::from_bytes(111));
        assert_eq!(report.size_new, Size::from_bytes(112));
    }
}

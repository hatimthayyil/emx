//! The emx repo's `lock/`: `flake.lock` pins each git package as a flake
//! input, and `archive.lock` pins each ELPA package at an archive's release.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::error::Error;

/// `lock/flake.lock`.
#[derive(Debug, Deserialize)]
pub struct FlakeLock {
    nodes: Nodes,
}

#[derive(Debug, Deserialize)]
struct Nodes {
    root: Root,
    #[serde(flatten)]
    others: BTreeMap<String, Node>,
}

#[derive(Debug, Deserialize)]
struct Root {
    inputs: BTreeMap<String, NodeRef>,
}

/// What a root input names: its own node, or the path to an input it follows.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NodeRef {
    Node(String),
    Follows(IgnoredAny),
}

#[derive(Debug, Deserialize)]
struct Node {
    original: Origin,
    locked: Option<Locked>,
}

/// Where an input comes from, as `lock/flake.nix` gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Origin {
    /// The fetcher: `github` or `git` for what twist locks.
    #[serde(rename = "type")]
    pub kind: String,
    /// A `git` input's repository.
    pub url: Option<String>,
    /// A `github` input's owner.
    pub owner: Option<String>,
    /// A `github` input's repository.
    pub repo: Option<String>,
    /// A `github` input's server, when it isn't github.com.
    pub host: Option<String>,
    /// The branch or tag the input follows; the repository's HEAD when none.
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    /// A commit the input is pinned to, which no update moves.
    pub rev: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Locked {
    rev: Option<String>,
}

/// A git package's flake input.
#[derive(Debug, Clone)]
pub struct Input {
    /// The package, which names its root input.
    pub name: String,
    /// Where the input is fetched from.
    pub origin: Origin,
    /// The commit the lock pins.
    pub rev: Option<String>,
}

impl FlakeLock {
    /// Reads `flake.lock` in `lock_dir`.
    pub fn read(lock_dir: &Path) -> Result<Self, Error> {
        read_json(&lock_dir.join("flake.lock"))
    }

    /// Whether the lock has a root input called `name`.
    pub fn has_input(&self, name: &str) -> bool {
        self.nodes.root.inputs.contains_key(name)
    }

    /// The root's inputs with nodes of their own, by name. An input that
    /// follows another is fetched as that one.
    pub fn inputs(&self) -> Vec<Input> {
        self.nodes
            .root
            .inputs
            .iter()
            .filter_map(|(name, node)| match node {
                NodeRef::Node(key) => self.nodes.others.get(key).map(|node| Input {
                    name: name.clone(),
                    origin: node.original.clone(),
                    rev: node.locked.as_ref().and_then(|locked| locked.rev.clone()),
                }),
                NodeRef::Follows(_) => None,
            })
            .collect()
    }
}

/// `lock/archive.lock`: each ELPA package, by name.
pub type ArchiveLock = BTreeMap<String, ArchivePin>;

/// An ELPA package's release, and the archive it was taken from.
#[derive(Debug, Clone, Deserialize)]
pub struct ArchivePin {
    /// The release, its version numbers joined with dots.
    pub version: String,
    /// The archive the release comes from.
    pub inventory: Inventory,
}

/// Where twist found an ELPA package's releases.
#[derive(Debug, Clone, Deserialize)]
pub struct Inventory {
    /// The kind of inventory: `archive` for an ELPA archive.
    #[serde(rename = "type")]
    pub kind: String,
    /// The archive's base URL, which holds its `archive-contents`.
    pub url: String,
}

/// The names in `lock/archive.lock`, whatever their entries hold.
pub fn archive_names(lock_dir: &Path) -> Result<Vec<String>, Error> {
    let archive: BTreeMap<String, IgnoredAny> = read_json(&lock_dir.join("archive.lock"))?;
    Ok(archive.into_keys().collect())
}

/// Reads `archive.lock` in `lock_dir`.
pub fn read_archive(lock_dir: &Path) -> Result<ArchiveLock, Error> {
    read_json(&lock_dir.join("archive.lock"))
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

    #[test]
    fn lists_root_inputs_with_their_pins() {
        let lock: FlakeLock = serde_json::from_str(
            r#"{"nodes": {
                "root": {"inputs": {"gptel": "gptel", "magit": "magit_2", "dash": ["magit", "dash"]}},
                "gptel": {"original": {"type": "github", "owner": "karthink", "repo": "gptel"},
                          "locked": {"type": "github", "rev": "aaa"}},
                "magit_2": {"original": {"type": "git", "url": "https://example.org/magit.git",
                                         "ref": "main"}}
            }}"#,
        )
        .unwrap();
        let inputs = lock.inputs();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].name, "gptel");
        assert_eq!(inputs[0].rev.as_deref(), Some("aaa"));
        assert_eq!(inputs[0].origin.owner.as_deref(), Some("karthink"));
        assert_eq!(inputs[1].name, "magit");
        assert_eq!(inputs[1].rev, None);
        assert_eq!(inputs[1].origin.git_ref.as_deref(), Some("main"));
        assert!(lock.has_input("dash"));
    }

    #[test]
    fn reads_archive_pins() {
        let pins: ArchiveLock = serde_json::from_str(
            r#"{"auctex": {"version": "14.2.0", "packageRequires": {},
                "archive": {"type": "tarball", "url": "https://elpa.gnu.org/packages/auctex-14.2.0.tar"},
                "inventory": {"type": "archive", "url": "https://elpa.gnu.org/packages/"}}}"#,
        )
        .unwrap();
        assert_eq!(pins["auctex"].version, "14.2.0");
        assert_eq!(
            pins["auctex"].inventory.url,
            "https://elpa.gnu.org/packages/"
        );
    }
}

//! Checks that `emx upgrade` moved each package to what upstream has now.
//! Nix keeps a pin it couldn't refresh and still exits 0, so emx asks upstream
//! itself: `git ls-remote` for each git package's commit, and each ELPA
//! archive's `archive-contents` for its releases. Exit statuses, commit hashes
//! and the archives' own data decide, never what Nix prints.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use crate::lock::{ArchiveLock, ArchivePin, Input, Origin};
use crate::process;

/// Upstreams asked at once.
const WORKERS: usize = 16;

/// How long one upstream gets to answer.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What `git rev-parse --local-env-vars` lists: the variables that point git
/// at a repository or its parts, as a hook or `git rebase --exec` hands its
/// commands. `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_COUNT` are left out:
/// they pass config, which applies as whoever set it meant.
const REPOSITORY_VARIABLES: [&str; 13] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
];

/// The packages whose pins don't match upstream.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Upstream has moved past these packages' pins.
    pub stale: Vec<String>,
    /// Packages emx couldn't compare with upstream, each with the reason.
    pub unchecked: Vec<(String, String)>,
}

impl Report {
    /// Whether every package asked about is at what upstream has now.
    pub fn is_current(&self) -> bool {
        self.stale.is_empty() && self.unchecked.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lock/ isn't at upstream's newest for every package:")?;
        if !self.stale.is_empty() {
            write!(
                f,
                "\n  not refreshed: {}. Upstream has moved past their pins, and Nix's \
                 output above may say why. If upstream moved during this run, running \
                 `emx upgrade` again clears this. If a package stays here, Nix may \
                 follow another branch than upstream's default, as it does when the \
                 repository has a branch named HEAD: give the package a recipe that \
                 names its branch (`:branch`)",
                self.stale.join(", ")
            )?;
        }
        for (name, reason) in &self.unchecked {
            write!(f, "\n  couldn't check {name}: {reason}")?;
        }
        Ok(())
    }
}

/// How a package's pin compares with upstream.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Current,
    Stale,
    Unchecked(String),
}

/// One question to upstream: a git package, or an ELPA archive with the
/// packages pinned from it.
enum Task<'a> {
    Input(&'a Input),
    Archive {
        url: &'a str,
        pins: Vec<(&'a str, &'a ArchivePin)>,
    },
    Unsupported {
        kind: &'a str,
        names: Vec<&'a str>,
    },
}

/// Compares the pins of `inputs` and `archive` with what upstream has now.
/// An input that names a commit can't move, so it isn't asked about.
pub fn check(inputs: &[Input], archive: &ArchiveLock) -> Report {
    let mut tasks: Vec<Task> = inputs
        .iter()
        .filter(|input| input.origin.rev.is_none())
        .map(Task::Input)
        .collect();
    let mut by_archive: BTreeMap<&str, Vec<(&str, &ArchivePin)>> = BTreeMap::new();
    let mut unsupported: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, pin) in archive {
        if pin.inventory.kind == "archive" {
            by_archive
                .entry(&pin.inventory.url)
                .or_default()
                .push((name, pin));
        } else {
            unsupported
                .entry(&pin.inventory.kind)
                .or_default()
                .push(name);
        }
    }
    tasks.extend(
        by_archive
            .into_iter()
            .map(|(url, pins)| Task::Archive { url, pins }),
    );
    tasks.extend(
        unsupported
            .into_iter()
            .map(|(kind, names)| Task::Unsupported { kind, names }),
    );

    let mut verdicts: Vec<(String, Verdict)> =
        parallel(&tasks, run).into_iter().flatten().collect();
    verdicts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut report = Report::default();
    for (name, verdict) in verdicts {
        match verdict {
            Verdict::Current => {}
            Verdict::Stale => report.stale.push(name),
            Verdict::Unchecked(reason) => report.unchecked.push((name, reason)),
        }
    }
    report
}

fn run(task: &Task) -> Vec<(String, Verdict)> {
    match task {
        Task::Input(input) => vec![(input.name.clone(), check_input(input))],
        Task::Archive { url, pins } => check_archive(url, pins),
        Task::Unsupported { kind, names } => names
            .iter()
            .map(|name| {
                let reason = format!("emx can't ask upstream about {kind} inventories");
                ((*name).to_owned(), Verdict::Unchecked(reason))
            })
            .collect(),
    }
}

fn check_input(input: &Input) -> Verdict {
    let Some(pinned) = &input.rev else {
        return Verdict::Unchecked("lock/flake.lock records no commit for it".to_owned());
    };
    let remote = match Remote::of(&input.origin) {
        Ok(remote) => remote,
        Err(reason) => return Verdict::Unchecked(reason),
    };
    match remote.commit() {
        Ok(commit) if commit == *pinned => Verdict::Current,
        Ok(_) => Verdict::Stale,
        Err(reason) => Verdict::Unchecked(reason),
    }
}

/// A git repository, and the refs that name the commit Nix follows in it,
/// best first.
#[derive(Debug, PartialEq, Eq)]
struct Remote {
    url: String,
    refs: Vec<String>,
}

impl Remote {
    fn of(origin: &Origin) -> Result<Self, String> {
        match origin.kind.as_str() {
            "git" => {
                let url = origin.url.clone().ok_or("its input has no url")?;
                // Nix fetches a ref outside refs/ as a branch.
                let git_ref = match origin.git_ref.as_deref() {
                    None => "HEAD".to_owned(),
                    Some(r) if r == "HEAD" || r.starts_with("refs/") => r.to_owned(),
                    Some(r) => format!("refs/heads/{r}"),
                };
                Ok(Self {
                    url,
                    refs: vec![git_ref],
                })
            }
            "github" => {
                let (Some(owner), Some(repo)) = (&origin.owner, &origin.repo) else {
                    return Err("its input has no owner or repo".to_owned());
                };
                let host = origin.host.as_deref().unwrap_or("github.com");
                // GitHub takes a ref outside refs/ as a branch or a tag; a tag
                // pins the commit it points to.
                // Nix asks GitHub for the commit `ref` names, with HEAD for none.
                let refs = match origin.git_ref.as_deref() {
                    None | Some("HEAD") => vec!["HEAD".to_owned()],
                    Some(r) if r.starts_with("refs/") => vec![r.to_owned()],
                    Some(r) => vec![
                        format!("refs/heads/{r}"),
                        format!("refs/tags/{r}^{{}}"),
                        format!("refs/tags/{r}"),
                    ],
                };
                Ok(Self {
                    url: format!("https://{host}/{owner}/{repo}.git"),
                    refs,
                })
            }
            kind => Err(format!("emx can't ask upstream about {kind} inputs")),
        }
    }

    /// The commit upstream's ref names now.
    fn commit(&self) -> Result<String, String> {
        let output = self.ls_remote()?;
        let listed: BTreeMap<&str, &str> = output
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(commit, name)| (name, commit))
            .collect();
        self.refs
            .iter()
            .find_map(|name| listed.get(name.as_str()))
            .map(|commit| (*commit).to_owned())
            .ok_or_else(|| format!("{} has no {}", self.url, self.refs[0]))
    }

    /// Runs `git ls-remote` without the user's Git config, which could send it
    /// over SSH or ask for credentials, and without any way to prompt. It runs
    /// in `/` without the variables that name a repository, so the config of
    /// a repository emx was started in, or from, doesn't apply either.
    fn ls_remote(&self) -> Result<String, String> {
        let mut command = Command::new("git");
        command
            .arg("ls-remote")
            .arg(&self.url)
            .args(&self.refs)
            .current_dir("/")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in REPOSITORY_VARIABLES {
            command.env_remove(name);
        }
        let mut child =
            process::spawn_group(&mut command).map_err(|err| format!("cannot run git: {err}"))?;
        // ls-remote prints a line per ref asked for, too little to fill a pipe.
        let status = process::wait(&mut child, TIMEOUT)
            .map_err(|err| format!("cannot run git: {err}"))?
            .ok_or_else(|| {
                format!(
                    "git ls-remote {} gave no answer in {} s",
                    self.url,
                    TIMEOUT.as_secs()
                )
            })?;
        let mut stdout = String::new();
        let mut stderr = String::new();
        let _ = child
            .stdout
            .take()
            .map(|mut out| out.read_to_string(&mut stdout));
        let _ = child
            .stderr
            .take()
            .map(|mut err| err.read_to_string(&mut stderr));
        if status.success() {
            Ok(stdout)
        } else {
            Err(failure("git ls-remote", &self.url, status, &stderr))
        }
    }
}

fn check_archive(url: &str, pins: &[(&str, &ArchivePin)]) -> Vec<(String, Verdict)> {
    let contents = format!("{url}archive-contents");
    let releases = fetch(&contents)
        .and_then(|text| releases(&text).map_err(|err| format!("cannot read {contents}: {err}")));
    pins.iter()
        .map(|(name, pin)| {
            let verdict = match &releases {
                Err(reason) => Verdict::Unchecked(reason.clone()),
                Ok(releases) => match releases.get(*name) {
                    Some(version) if *version == pin.version => Verdict::Current,
                    Some(_) => Verdict::Stale,
                    None => Verdict::Unchecked(format!("{contents} doesn't list it")),
                },
            };
            ((*name).to_owned(), verdict)
        })
        .collect()
}

/// Downloads `url` with curl, which stops itself at the timeout. `-q`, which
/// only works first, keeps the user's `.curlrc` from changing what it fetches.
fn fetch(url: &str) -> Result<String, String> {
    let mut child = Command::new("curl")
        .args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
        ])
        .arg(TIMEOUT.as_secs().to_string())
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("cannot run curl: {err}"))?;
    let mut body = String::new();
    let read = child
        .stdout
        .take()
        .expect("curl's stdout is piped")
        .read_to_string(&mut body);
    let mut stderr = String::new();
    let _ = child
        .stderr
        .take()
        .map(|mut err| err.read_to_string(&mut stderr));
    let status = child
        .wait()
        .map_err(|err| format!("cannot run curl: {err}"))?;
    if !status.success() {
        return Err(failure("curl", url, status, &stderr));
    }
    read.map_err(|err| format!("cannot read {url}: {err}"))?;
    Ok(body)
}

fn failure(program: &str, url: &str, status: std::process::ExitStatus, stderr: &str) -> String {
    match stderr.lines().find(|line| !line.trim().is_empty()) {
        Some(line) => format!("{program} {url} failed ({status}): {}", line.trim()),
        None => format!("{program} {url} failed ({status})"),
    }
}

/// Each package's newest release in an ELPA archive's `archive-contents`, an
/// Elisp list of `(NAME . [VERSION REQUIREMENTS DESCRIPTION KIND EXTRAS])`
/// after a format number. VERSION is a list of integers, which twist joins
/// with dots for `lock/archive.lock`. Entries of another shape are skipped,
/// so a package in one reads as not listed.
fn releases(text: &str) -> Result<BTreeMap<String, String>, String> {
    let contents = lexpr::from_str_custom(text, lexpr::parse::Options::elisp())
        .map_err(|err| err.to_string())?;
    let items = contents.list_iter().ok_or("it is not a list")?;
    let mut releases = BTreeMap::new();
    for item in items.skip(1) {
        let Some((name, fields)) = item
            .as_cons()
            .and_then(|entry| Some((entry.car().as_symbol()?, entry.cdr().as_slice()?)))
        else {
            continue;
        };
        let Some(numbers) = fields.first().and_then(lexpr::Value::list_iter) else {
            continue;
        };
        let version: Option<Vec<String>> = numbers
            .map(|number| number.as_i64().map(|n| n.to_string()))
            .collect();
        if let Some(version) = version {
            releases.insert(name.to_owned(), version.join("."));
        }
    }
    Ok(releases)
}

/// `f` applied to each of `items`, in order, with at most `WORKERS` running
/// at once.
fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new(items.iter().map(|_| None).collect());
    thread::scope(|scope| {
        for _ in 0..WORKERS.min(items.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    let result = f(item);
                    results.lock().expect("no worker panics holding it")[index] = Some(result);
                }
            });
        }
    });
    results
        .into_inner()
        .expect("no worker panics holding it")
        .into_iter()
        .map(|result| result.expect("every item ran"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(json: &str) -> Origin {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn asks_a_git_repository_for_its_head_or_branch() {
        assert_eq!(
            Remote::of(&origin(
                r#"{"type": "git", "url": "https://example.org/a.git"}"#
            )),
            Ok(Remote {
                url: "https://example.org/a.git".to_owned(),
                refs: vec!["HEAD".to_owned()],
            })
        );
        let branch =
            origin(r#"{"type": "git", "url": "https://x/nongnu.git", "ref": "elpa/ws-butler"}"#);
        assert_eq!(
            Remote::of(&branch).unwrap().refs,
            ["refs/heads/elpa/ws-butler"]
        );
        let full = origin(r#"{"type": "git", "url": "https://x/a.git", "ref": "refs/tags/v1"}"#);
        assert_eq!(Remote::of(&full).unwrap().refs, ["refs/tags/v1"]);
    }

    #[test]
    fn asks_github_over_git_for_a_head_branch_or_tag() {
        let head = origin(r#"{"type": "github", "owner": "magit", "repo": "magit"}"#);
        assert_eq!(
            Remote::of(&head),
            Ok(Remote {
                url: "https://github.com/magit/magit.git".to_owned(),
                refs: vec!["HEAD".to_owned()],
            })
        );
        let named = origin(
            r#"{"type": "github", "owner": "o", "repo": "r", "ref": "v1", "host": "git.example.org"}"#,
        );
        assert_eq!(
            Remote::of(&named),
            Ok(Remote {
                url: "https://git.example.org/o/r.git".to_owned(),
                refs: vec![
                    "refs/heads/v1".to_owned(),
                    "refs/tags/v1^{}".to_owned(),
                    "refs/tags/v1".to_owned(),
                ],
            })
        );
    }

    #[test]
    fn asks_github_for_its_head_when_the_ref_is_head() {
        let head = origin(r#"{"type": "github", "owner": "o", "repo": "r", "ref": "HEAD"}"#);
        assert_eq!(Remote::of(&head).unwrap().refs, ["HEAD"]);
    }

    /// `git ls-remote` lists an annotated tag twice: the tag object, and with
    /// `^{}` the commit it points to, which is what Nix pins.
    #[test]
    fn resolves_an_annotated_tag_to_its_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("r.git");
        let made = Command::new("sh")
            .args([
                "-c",
                r#"set -e
git init --quiet --bare --initial-branch=main "$1"
cd "$1"
tree=$(git hash-object -w -t tree /dev/null)
commit=$(git commit-tree -m one "$tree")
git update-ref refs/heads/main "$commit"
git tag -a -m v1 v1 "$commit"
echo "$commit""#,
                "sh",
            ])
            .arg(&repo)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "emx")
            .env("GIT_AUTHOR_EMAIL", "emx@example.org")
            .env("GIT_COMMITTER_NAME", "emx")
            .env("GIT_COMMITTER_EMAIL", "emx@example.org")
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        let commit = String::from_utf8(made.stdout).unwrap().trim().to_owned();
        let tagged = origin(r#"{"type": "github", "owner": "o", "repo": "r", "ref": "v1"}"#);
        let remote = Remote {
            url: format!("file://{}", repo.display()),
            refs: Remote::of(&tagged).unwrap().refs,
        };
        assert_eq!(remote.commit(), Ok(commit));
    }

    #[test]
    fn clears_each_variable_git_ties_to_a_repository() {
        let listed = Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .current_dir("/")
            .output()
            .unwrap();
        assert!(listed.status.success(), "{listed:?}");
        let mut names: Vec<&str> = std::str::from_utf8(&listed.stdout)
            .unwrap()
            .lines()
            .filter(|name| !["GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT"].contains(name))
            .collect();
        names.sort_unstable();
        assert_eq!(names, REPOSITORY_VARIABLES);
    }

    #[test]
    fn refuses_inputs_it_cannot_ask_about() {
        assert_eq!(
            Remote::of(&origin(r#"{"type": "path", "path": "/src"}"#)),
            Err("emx can't ask upstream about path inputs".to_owned())
        );
    }

    #[test]
    fn skips_inputs_pinned_to_a_commit() {
        let input = Input {
            name: "pinned".to_owned(),
            origin: origin(r#"{"type": "git", "url": "file:///missing", "rev": "abc"}"#),
            rev: Some("abc".to_owned()),
        };
        assert!(check(&[input], &ArchiveLock::new()).is_current());
    }

    #[test]
    fn reads_each_packages_newest_release() {
        let text = r#";; archive-contents
(1
 (ace-window .
   [(0 10 0) ((avy (0 5 0))) "Quickly \"switch\" windows." tar
    ((:url . "https://github.com/abo-abo/ace-window")
     (:authors ("Oleh Krehel" . "o@example.org")))])
 (pre . [(1 0 -4) nil "A pre-release; with a semicolon." single nil])
 (odd . "not a vector")
 (auctex . [(14 2 0) ((emacs (28 1))) "Integrated environment for TeX" tar nil]))"#;
        assert_eq!(
            releases(text).unwrap(),
            BTreeMap::from([
                ("ace-window".to_owned(), "0.10.0".to_owned()),
                ("auctex".to_owned(), "14.2.0".to_owned()),
                ("pre".to_owned(), "1.0.-4".to_owned()),
            ])
        );
    }

    #[test]
    fn refuses_archive_contents_it_cannot_read() {
        for text in [
            "",
            "(1 (a . [(1) nil \"open",
            "(1 (a b . c d))",
            "#s(hash-table)",
            "(1 (a . [(1) nil \"d\" tar nil])) trailing",
        ] {
            assert!(releases(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn keeps_the_order_of_its_items() {
        let items: Vec<usize> = (0..100).collect();
        assert_eq!(
            parallel(&items, |n| n * 2),
            (0..100).map(|n| n * 2).collect::<Vec<_>>()
        );
        assert!(parallel(&[] as &[usize], |n| *n).is_empty());
    }

    #[test]
    fn says_what_is_stale_and_what_it_could_not_check() {
        let report = Report {
            stale: vec!["gptel".to_owned(), "magit".to_owned()],
            unchecked: vec![("ws-butler".to_owned(), "git ls-remote failed".to_owned())],
        };
        let text = report.to_string();
        assert!(text.contains("not refreshed: gptel, magit."), "{text}");
        assert!(
            text.contains("running `emx upgrade` again clears this"),
            "{text}"
        );
        assert!(
            text.contains("a recipe that names its branch (`:branch`)"),
            "{text}"
        );
        assert!(
            text.ends_with("\n  couldn't check ws-butler: git ls-remote failed"),
            "{text}"
        );
    }
}

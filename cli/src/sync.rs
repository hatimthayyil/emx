//! `emx sync`: builds the emx repo's Emacs and packages, shows what the build
//! changes, and after confirmation makes it a new generation of emx's
//! profile.

use std::path::PathBuf;

use crate::build;
use crate::cli::SyncArgs;
use crate::diff;
use crate::error::Error;
use crate::nix;
use crate::profile::Profile;
use crate::prompt;
use crate::source::Source;

/// The flake output holding Emacs with every package (nix/emacs).
const EMACS_OUTPUT: &str = "emacs";

pub fn run(src: Option<PathBuf>, args: &SyncArgs) -> Result<(), Error> {
    let source = Source::open(src)?;
    let profile = Profile::from_env()?;
    prompt::ensure_answerable(args.yes)?;
    profile.create_parent()?;
    let current = profile.current()?;

    let build = build::build(
        &source.installable(EMACS_OUTPUT),
        &profile.pending_link(),
        &args.nix_args,
    )?;
    if let Some(current) = &current
        && current.store_path == build.store_path()
    {
        println!(
            "emx: nothing changed: generation {} is already {}",
            current.number,
            build.store_path().display()
        );
        return Ok(());
    }

    let shown = match &current {
        Some(current) => diff::show(&current.store_path, build.store_path()),
        None => diff::summarize(build.store_path()),
    };
    if let Err(err) = shown {
        eprintln!("emx: cannot show what the build changes: {err}");
    }
    if !args.yes && !prompt::confirm("Switch to this build?")? {
        return Err(Error::Declined {
            generation: current.map(|generation| generation.number),
        });
    }

    nix::run(&nix::set_profile(profile.link(), build.store_path()))?;
    let generation = profile.current()?.ok_or_else(|| Error::ProfileNotCreated {
        path: profile.link().to_owned(),
    })?;
    println!(
        "emx: {} is now generation {}: {}",
        profile.link().display(),
        generation.number,
        generation.store_path.display()
    );
    Ok(())
}

//! `emx rollback`: returns emx's profile to its previous generation, and
//! reloads a running Emacs into it. It moves only the profile, so the emx
//! repo's `lock/` keeps its pins.

use crate::error::Error;
use crate::nix;
use crate::profile::Profile;
use crate::reload;

pub fn run() -> Result<(), Error> {
    let profile = Profile::from_env()?;
    if profile.current()?.is_none() {
        return Err(Error::NoGeneration);
    }
    nix::run(&nix::rollback(profile.link()))?;
    let generation = profile.current()?.ok_or(Error::NoGeneration)?;
    println!(
        "emx: {} is back at generation {}: {}",
        profile.link().display(),
        generation.number,
        generation.store_path.display()
    );
    println!("emx: lock/ is not rolled back, so the next sync builds its pins again");
    reload::running_emacs(&profile)?;
    Ok(())
}

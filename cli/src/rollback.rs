//! `emx rollback`: returns emx's profile to its previous generation. It moves
//! only the profile, so the emx repo's `lock/` keeps its pins.

use crate::daemon;
use crate::error::Error;
use crate::nix;
use crate::profile::Profile;

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
    daemon::print_restart();
    Ok(())
}

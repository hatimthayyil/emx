//! Running programs that might never finish, and might start programs of
//! their own, as git starts `git-remote-https`.

use std::io;
use std::mem;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};
use std::ptr;
use std::sync::Once;
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// The process groups `spawn_group` started that `wait` hasn't reaped, 0
/// marking a free slot. They are atomics because `forward` reads them.
static GROUPS: [AtomicI32; 64] = [const { AtomicI32::new(0) }; 64];

/// The signals a terminal sends its foreground process group, which emx's
/// own groups aren't, so emx passes them on.
const FORWARDED: [libc::c_int; 4] = [libc::SIGINT, libc::SIGQUIT, libc::SIGHUP, libc::SIGTERM];

static FORWARD: Once = Once::new();

/// Spawns `command` as the leader of a process group of its own, so that
/// `wait` can stop whatever it starts too. A signal from the terminal, such
/// as Ctrl-C, reaches that group through emx, unless emx was started with
/// that signal ignored, as `nohup` ignores SIGHUP: then the group inherits
/// the ignore.
pub fn spawn_group(command: &mut Command) -> io::Result<Child> {
    FORWARD.call_once(forward_unignored);
    // A signal that arrives between the spawn and taking a slot waits until
    // the slot is taken, so it reaches the new group too.
    held(|before| {
        let before = *before;
        // The child would inherit the hold, which std leaves in place.
        // SAFETY: sigprocmask is async-signal-safe, as pre_exec requires.
        unsafe {
            command.pre_exec(move || {
                libc::sigprocmask(libc::SIG_SETMASK, &before, ptr::null_mut());
                Ok(())
            });
        }
        let child = command.process_group(0).spawn()?;
        // With every slot taken the group still runs; only a signal misses it.
        swap_slot(0, pid(&child));
        Ok(child)
    })
}

/// Waits for `child`, which `spawn_group` started, for up to `timeout`, then
/// kills its whole process group. Returns its status, or `None` when it ran
/// out of time.
pub fn wait(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = reap(child, Child::try_wait)? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            // SAFETY: kill only reads its arguments.
            if unsafe { libc::kill(-pid(child), libc::SIGKILL) } != 0 {
                let err = io::Error::last_os_error();
                held(|_| swap_slot(pid(child), 0));
                return Err(err);
            }
            reap(child, |child| child.wait().map(Some))?;
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Reaps `child` with `how` and frees its slot once it is reaped. Both
/// happen with the forwarded signals held, so `forward` never signals a
/// group whose ID the system has given to another process since.
fn reap(
    child: &mut Child,
    how: impl FnOnce(&mut Child) -> io::Result<Option<ExitStatus>>,
) -> io::Result<Option<ExitStatus>> {
    held(|_| {
        let reaped = how(child);
        if !matches!(reaped, Ok(None)) {
            swap_slot(pid(child), 0);
        }
        reaped
    })
}

/// Runs `f` with the forwarded signals blocked in this thread, giving it the
/// mask from before, then delivers any that arrived. Another thread can
/// still take a signal meanwhile.
fn held<T>(f: impl FnOnce(&libc::sigset_t) -> T) -> T {
    // SAFETY: both sets are initialised before use, and pthread_sigmask only
    // changes this thread's mask.
    unsafe {
        let mut forwarded: libc::sigset_t = mem::zeroed();
        libc::sigemptyset(&mut forwarded);
        for signal in FORWARDED {
            libc::sigaddset(&mut forwarded, signal);
        }
        let mut before: libc::sigset_t = mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, &forwarded, &mut before);
        let result = f(&before);
        libc::pthread_sigmask(libc::SIG_SETMASK, &before, ptr::null_mut());
        result
    }
}

/// Makes `forward` handle each forwarded signal that emx wasn't started
/// ignoring. One that is ignored stays so, for emx and all it starts.
fn forward_unignored() {
    for signal in FORWARDED {
        // SAFETY: a zeroed sigaction is a valid place for sigaction to write
        // the current one to, and `forward` makes only async-signal-safe
        // calls.
        unsafe {
            let mut current: libc::sigaction = mem::zeroed();
            libc::sigaction(signal, ptr::null(), &mut current);
            if current.sa_sigaction == libc::SIG_IGN {
                continue;
            }
            let mut action: libc::sigaction = mem::zeroed();
            action.sa_sigaction = forward as extern "C" fn(libc::c_int) as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, ptr::null_mut());
        }
    }
}

/// Puts `new` in the first slot of `GROUPS` that holds `old`.
fn swap_slot(old: libc::pid_t, new: libc::pid_t) {
    for slot in &GROUPS {
        if slot
            .compare_exchange(old, new, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return;
        }
    }
}

fn pid(child: &Child) -> libc::pid_t {
    libc::pid_t::try_from(child.id()).expect("a process ID fits pid_t")
}

/// Sends `signal` to every group emx started, then lets it end emx as it
/// would have without this handler.
extern "C" fn forward(signal: libc::c_int) {
    // SAFETY: kill, signal and raise are async-signal-safe, and atomic loads
    // take no lock.
    unsafe {
        for slot in &GROUPS {
            let group = slot.load(Ordering::SeqCst);
            if group != 0 {
                libc::kill(-group, signal);
            }
        }
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    use super::*;

    #[test]
    fn stops_waiting_at_the_timeout() {
        let mut child = spawn_group(Command::new("sleep").arg("30")).unwrap();
        let started = Instant::now();
        assert_eq!(wait(&mut child, Duration::from_millis(100)).unwrap(), None);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(child.try_wait().unwrap().is_some());
    }

    /// `spawn_group` holds the forwarded signals while it spawns, and the
    /// child must not keep them held.
    #[test]
    fn starts_the_child_with_no_signal_held() {
        let mut child = spawn_group(
            Command::new("sh")
                .args(["-c", "grep SigBlk /proc/$$/status"])
                .stdout(Stdio::piped()),
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        wait(&mut child, Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(line.trim(), "SigBlk:\t0000000000000000");
    }

    #[test]
    fn returns_the_status_of_a_child_that_finishes() {
        let mut child = spawn_group(&mut Command::new("true")).unwrap();
        let status = wait(&mut child, Duration::from_secs(10)).unwrap().unwrap();
        assert!(status.success());
    }

    /// What a timed-out child started stops with it, as git's
    /// `git-remote-https` must.
    #[test]
    fn stops_what_the_child_started_at_the_timeout() {
        let mut child = spawn_group(
            Command::new("sh")
                .args(["-c", "sleep 30 & echo $!; wait"])
                .stdout(Stdio::piped()),
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let sleep: libc::pid_t = line.trim().parse().unwrap();
        assert_eq!(wait(&mut child, Duration::from_millis(100)).unwrap(), None);
        // Its new parent reaps it soon after it dies.
        let deadline = Instant::now() + Duration::from_secs(5);
        // SAFETY: signal 0 only checks that the process exists.
        while unsafe { libc::kill(sleep, 0) } == 0 {
            assert!(
                Instant::now() < deadline,
                "sleep {sleep} outlived its group"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

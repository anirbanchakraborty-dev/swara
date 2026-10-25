//! Ending cleanly when the editor stops the server with a signal rather than `exit`. An editor
//! may send SIGTERM to a server that has not exited soon after it quit, and a closed terminal
//! sends SIGHUP. Either one, left alone, ends the process at once, with its LilyPond runs still
//! going and its work folder left behind.

use crossbeam_channel::Receiver;

/// Watches for SIGTERM, SIGHUP and SIGINT, and returns a channel that receives the first one's
/// number, so that the server's loop can stop and clean up. Each signal is caught by a handler
/// that writes its number to a pipe, which a watcher thread reads. A caught signal goes back to
/// its default in every child at `exec`, so LilyPond still ends on its own SIGTERM. Blocking the
/// signals instead would not do that, since the standard library passes a blocked mask on to
/// children. When the process has not ended two seconds after a signal, the watcher ends it, with
/// 128 and the signal's number as its status, as a shell reports a death by signal. When the
/// handlers cannot be set up, the channel never receives, and a signal ends the server as before.
#[cfg(unix)]
pub fn watch() -> Receiver<i32> {
    use std::io::Read;
    let Ok(mut reader) = catch(&[libc::SIGTERM, libc::SIGHUP, libc::SIGINT]) else {
        return crossbeam_channel::never();
    };
    let (sender, receiver) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        let mut signal = [0];
        if reader.read_exact(&mut signal).is_err() {
            return;
        }
        let signal = i32::from(signal[0]);
        let _ = sender.send(signal);
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::process::exit(128 + signal);
    });
    receiver
}

/// On Windows an editor ends a server outright, and there is nothing to catch.
#[cfg(not(unix))]
pub fn watch() -> Receiver<i32> {
    crossbeam_channel::never()
}

/// The pipe's write end, for the handler, which can reach nothing else.
#[cfg(unix)]
static WRITE_END: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Sets a handler for each of `signals` and returns the pipe the handler writes their numbers to.
/// The pipe's write end stays open for the life of the process.
#[cfg(unix)]
fn catch(signals: &[i32]) -> std::io::Result<std::io::PipeReader> {
    use std::os::fd::IntoRawFd;
    // The standard library opens both ends close-on-exec, so no child inherits them.
    let (reader, writer) = std::io::pipe()?;
    let writer = writer.into_raw_fd();
    // SAFETY: `writer` is an open descriptor this function owns. A write end that never blocks
    // keeps the handler from waiting on a full pipe.
    if unsafe { libc::fcntl(writer, libc::F_SETFL, libc::O_NONBLOCK) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    WRITE_END.store(writer, std::sync::atomic::Ordering::Relaxed);
    for &signal in signals {
        // SAFETY: a zeroed sigaction is valid, its mask is emptied before use, and `note` does
        // only what a handler may.
        let set = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = note as extern "C" fn(libc::c_int) as libc::sighandler_t;
            // Calls a signal interrupts in other threads start again rather than fail.
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if set != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(reader)
}

/// The handler: writes the signal's number to the pipe, which is all a handler can safely do.
#[cfg(unix)]
extern "C" fn note(signal: libc::c_int) {
    let byte = signal as u8;
    let fd = WRITE_END.load(std::sync::atomic::Ordering::Relaxed);
    // SAFETY: write is safe in a handler, and `byte` is valid for one byte. A failed write, on a
    // pipe full of earlier signals, loses nothing the watcher still needs.
    unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
}

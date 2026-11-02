//! Runs commands off the MIDI path and reports back when they finish.

use crate::engine::{Job, Outcome, Rendered};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// How long to wait for output after the command itself has exited. A
/// process it started in the background can keep the pipes open long after;
/// we stop waiting for it rather than keeping the control busy.
const OUTPUT_GRACE: Duration = Duration::from_millis(200);

#[cfg(unix)]
fn kill_group(child: &mut Child) {
    // The child leads its own process group (see `process_group(0)` below),
    // so this also stops anything it started, e.g. under `sh -c "a; b"`.
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: kill(2) with a negative pid signals that process group; it
        // has no memory-safety requirements.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_group(child: &mut Child) {
    let _ = child.kill();
}

/// Runs a command to completion (or until its timeout) and logs its output.
pub fn run(job: Job, exec: &Rendered) -> Outcome {
    debug!(control = %exec.control, program = %exec.program, args = ?exec.args, "running");
    let mut cmd = Command::new(&exec.program);
    cmd.args(&exec.args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!(control = %exec.control, program = %exec.program, "could not start command: {e}");
            return Outcome { success: false, stdout: String::new() };
        }
    };

    // Read both pipes on their own threads so a chatty command can't fill a
    // pipe buffer and deadlock while we wait for it. Output is collected
    // incrementally, so whatever arrived before we stop waiting is kept.
    // Once abandoned (a background process still holds the pipe), readers
    // keep draining so that process doesn't block, but stop storing.
    let abandoned = Arc::new(AtomicBool::new(false));
    let (eof_tx, eof_rx) = crossbeam_channel::bounded::<()>(2);
    let reader = |pipe: Option<Box<dyn Read + Send>>| -> Arc<Mutex<Vec<u8>>> {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let (buf2, abandoned, eof_tx) = (Arc::clone(&buf), Arc::clone(&abandoned), eof_tx.clone());
        thread::spawn(move || {
            if let Some(mut p) = pipe {
                let mut chunk = [0u8; 8192];
                loop {
                    match p.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) if !abandoned.load(Ordering::Relaxed) => {
                            if let Ok(mut b) = buf2.lock() {
                                b.extend_from_slice(&chunk[..n]);
                            }
                        }
                        Ok(_) => {}
                    }
                }
            }
            let _ = eof_tx.send(());
        });
        buf
    };
    let out_buf = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_buf = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    drop(eof_tx);

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(e) => {
                warn!(control = %exec.control, "could not wait for command: {e}");
                break None;
            }
        }
        if exec.timeout.is_some_and(|t| started.elapsed() >= t) {
            warn!(control = %exec.control, program = %exec.program, "command timed out; killing it");
            kill_group(&mut child);
            let _ = child.wait();
            return Outcome { success: false, stdout: String::new() };
        }
        thread::sleep(Duration::from_millis(5));
    };

    let deadline = Instant::now() + OUTPUT_GRACE;
    for _ in 0..2 {
        if eof_rx.recv_deadline(deadline).is_err() {
            debug!(control = %exec.control, "output still open after exit (background process?); not waiting");
            break;
        }
    }
    abandoned.store(true, Ordering::Relaxed);
    let take = |b: &Arc<Mutex<Vec<u8>>>| {
        b.lock().map(|mut v| String::from_utf8_lossy(&std::mem::take(&mut *v)).into_owned()).unwrap_or_default()
    };
    let (stdout, stderr) = (take(&out_buf), take(&err_buf));

    let success = status.is_some_and(|s| s.success());
    let quiet = job == Job::State;
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        if quiet {
            debug!(control = %exec.control, "stdout: {line}");
        } else {
            info!(control = %exec.control, "stdout: {line}");
        }
    }
    for line in stderr.lines().filter(|l| !l.trim().is_empty()) {
        if quiet && success {
            debug!(control = %exec.control, "stderr: {line}");
        } else {
            warn!(control = %exec.control, "stderr: {line}");
        }
    }
    if let Some(s) = status
        && !s.success()
    {
        debug!(control = %exec.control, "exited with {s}");
    }
    Outcome { success, stdout }
}

/// Runs `exec` on a new thread and hands the outcome to `done`.
///
/// The engine allows at most one command and one state read per control at
/// a time, so the number of threads is bounded by the number of controls.
/// `done` is always called, even if the thread can't be started.
pub fn spawn<F>(job: Job, exec: Rendered, done: F)
where
    F: FnOnce(Outcome) + Send + 'static,
{
    let done = Arc::new(Mutex::new(Some(done)));
    let in_thread = Arc::clone(&done);
    let name = format!("exec:{}", exec.control);
    let res = thread::Builder::new().name(name).spawn(move || {
        let outcome = run(job, &exec);
        if let Some(f) = in_thread.lock().ok().and_then(|mut g| g.take()) {
            f(outcome);
        }
    });
    if let Err(e) = res {
        warn!("could not start a command thread: {e}");
        if let Some(f) = done.lock().ok().and_then(|mut g| g.take()) {
            f(Outcome { success: false, stdout: String::new() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(program: &str, args: &[&str], timeout_ms: Option<u64>) -> Rendered {
        Rendered {
            control: "t".into(),
            program: program.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            timeout: timeout_ms.map(Duration::from_millis),
            value: None,
        }
    }

    #[test]
    fn captures_stdout_and_status() {
        let o = run(Job::State, &r("sh", &["-c", "echo hello; printf '\\377'"], None));
        assert!(o.success);
        assert!(o.stdout.starts_with("hello"), "non-UTF-8 output is kept lossily");
        assert!(!run(Job::Command, &r("false", &[], None)).success);
        assert!(!run(Job::Command, &r("/nonexistent/program", &[], None)).success);
    }

    #[test]
    fn background_processes_dont_hold_the_command() {
        // Found in review: `sh -c "sleep 3 &"` kept the control busy for 3s.
        let started = Instant::now();
        let o = run(Job::Command, &r("sh", &["-c", "sleep 3 &"], Some(500)));
        assert!(o.success);
        assert!(started.elapsed() < Duration::from_secs(1), "took {:?}", started.elapsed());
    }

    #[test]
    fn output_before_a_background_process_is_kept() {
        // Found in second review: all output was dropped when a background
        // process held the pipe.
        let o = run(Job::State, &r("sh", &["-c", "echo on; sleep 2 &"], None));
        assert!(o.success);
        assert_eq!(o.stdout.trim(), "on");
    }

    #[test]
    fn large_output_is_complete() {
        let o = run(Job::State, &r("sh", &["-c", "head -c 3000000 /dev/zero | tr '\\0' a"], None));
        assert_eq!(o.stdout.len(), 3_000_000);
    }

    #[test]
    fn timeout_kills_the_whole_group() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("marker");
        let script = format!("sleep 1; touch {}", marker.display());
        let o = run(Job::Command, &r("sh", &["-c", &script], Some(100)));
        assert!(!o.success);
        thread::sleep(Duration::from_millis(1500));
        assert!(!marker.exists(), "the grandchild survived the timeout");
    }

    #[test]
    fn timeout_kills_the_command() {
        let started = Instant::now();
        let o = run(Job::Command, &r("sleep", &["5"], Some(100)));
        assert!(!o.success);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

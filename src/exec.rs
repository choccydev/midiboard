//! Runs commands off the MIDI path and reports back when they finish.

use crate::engine::{Job, Outcome, Rendered};
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Runs a command to completion (or until its timeout) and logs its output.
pub fn run(job: Job, exec: &Rendered) -> Outcome {
    debug!(control = %exec.control, program = %exec.program, args = ?exec.args, "running");
    let child = Command::new(&exec.program)
        .args(&exec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            warn!(control = %exec.control, program = %exec.program, "could not start command: {e}");
            return Outcome { success: false, stdout: String::new() };
        }
    };

    // Read both pipes on their own threads so a chatty command can't fill a
    // pipe buffer and deadlock while we wait for it.
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));

    let started = Instant::now();
    let mut timed_out = false;
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
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        thread::sleep(Duration::from_millis(5));
    };
    if timed_out {
        // A grandchild (e.g. under `sh -c`) may still hold the pipes open;
        // don't block on the readers, they finish when it exits.
        return Outcome { success: false, stdout: String::new() };
    }
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();

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
pub fn spawn<F>(job: Job, exec: Rendered, done: F)
where
    F: FnOnce(Outcome) + Send + 'static,
{
    let name = format!("exec:{}", exec.control);
    let res = thread::Builder::new().name(name).spawn(move || done(run(job, &exec)));
    if let Err(e) = res {
        warn!("could not start a command thread: {e}");
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
    fn timeout_kills_the_command() {
        let started = Instant::now();
        let o = run(Job::Command, &r("sleep", &["5"], Some(100)));
        assert!(!o.success);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

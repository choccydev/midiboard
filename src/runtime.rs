//! Wires the engine to real MIDI ports, processes and signals.
//!
//! One thread per device. Each thread owns its engine and an event channel:
//! the MIDI callback, the command threads and the signal handler all post
//! into it, and the thread blocks on it (no spinning) until the next event or
//! engine timer.

use crate::config::Device;
use crate::engine::{Action, Engine, Input, Job, Outcome};
use crate::exec;
use crate::midi::{self, MidiMsg};
use anyhow::{Context, Result, anyhow};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const PORT_CHECK_EVERY: Duration = Duration::from_secs(1);
const RECONNECT_EVERY: Duration = Duration::from_secs(2);
const MIDI_QUEUE: usize = 4096;

pub enum Event {
    Midi(Instant, MidiMsg),
    Done { control: usize, job: Job, outcome: Outcome },
    Shutdown,
}

/// Lists the names of all MIDI input or output ports.
pub fn input_port_names(input: &MidiInput) -> Vec<String> {
    input.ports().iter().filter_map(|p| input.port_name(p).ok()).collect()
}

pub fn output_port_names(output: &MidiOutput) -> Vec<String> {
    output.ports().iter().filter_map(|p| output.port_name(p).ok()).collect()
}

/// Opens an output port by name, using the same matching rules as inputs.
pub fn open_output(wanted: &str, client: &str) -> Result<MidiOutputConnection> {
    let output = MidiOutput::new(client)?;
    let ports = output.ports();
    let names: Vec<String> = ports.iter().filter_map(|p| output.port_name(p).ok()).collect();
    let i = midi::select_port(&names, wanted)?;
    output.connect(&ports[i], "midiboard-out").map_err(|e| anyhow!("could not open output port: {}", e.kind()))
}

pub fn open_input<F>(wanted: &str, client: &str, callback: F) -> Result<MidiInputConnection<()>>
where
    F: FnMut(u64, &[u8], &mut ()) + Send + 'static,
{
    let mut input = MidiInput::new(client)?;
    input.ignore(Ignore::None);
    let ports = input.ports();
    let names: Vec<String> = ports.iter().filter_map(|p| input.port_name(p).ok()).collect();
    let i = midi::select_port(&names, wanted)?;
    input
        .connect(&ports[i], "midiboard-in", callback, ())
        .map_err(|e| anyhow!("could not open input port: {}", e.kind()))
}

pub fn run(devices: Vec<Device>) -> Result<()> {
    let mut senders: Vec<Sender<Event>> = Vec::new();
    let mut handles = Vec::new();
    for device in devices {
        let (tx, rx) = bounded::<Event>(MIDI_QUEUE);
        senders.push(tx.clone());
        let name = device.name.clone();
        let handle = thread::Builder::new()
            .name(format!("device:{name}"))
            .spawn(move || device_thread(device, tx, rx))
            .context("could not start a device thread")?;
        handles.push((name, handle));
    }

    ctrlc::set_handler(move || {
        info!("shutting down");
        for tx in &senders {
            let _ = tx.send(Event::Shutdown);
        }
    })
    .context("could not install the signal handler")?;

    // All devices run at the same time; wait for all of them.
    let mut failed = false;
    for (name, handle) in handles {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                error!(device = %name, "{e:#}");
                failed = true;
            }
            Err(_) => {
                error!(device = %name, "device thread crashed");
                failed = true;
            }
        }
    }
    // A non-zero exit lets systemd/launchd restart the service.
    if failed { Err(anyhow!("one or more devices stopped with an error")) } else { Ok(()) }
}

struct Session {
    _input: MidiInputConnection<()>,
    output: Option<MidiOutputConnection>,
}

fn connect(device: &Device, tx: &Sender<Event>) -> Result<Session> {
    let cb_tx = tx.clone();
    let input = open_input(&device.name, "midiboard", move |_stamp, bytes, _| {
        if let Some(msg) = MidiMsg::parse(bytes)
            && cb_tx.try_send(Event::Midi(Instant::now(), msg)).is_err()
        {
            // Never block the MIDI backend's thread.
            warn!("event queue full; dropping a MIDI message");
        }
    })?;
    let output = match &device.output_port {
        Some(port) => Some(open_output(port, "midiboard")?),
        None => None,
    };
    Ok(Session { _input: input, output })
}

fn device_thread(device: Device, tx: Sender<Event>, rx: Receiver<Event>) -> Result<()> {
    let start = Instant::now();
    let now = || start.elapsed();
    let mut engine = Engine::new(&device);
    // A long-lived client used only to notice when the port goes away.
    let watcher = MidiInput::new("midiboard-watch").context("could not open the MIDI backend")?;
    let mut warned_missing = false;

    loop {
        let mut session = match connect(&device, &tx) {
            Ok(s) => {
                info!(device = %device.name, "connected");
                warned_missing = false;
                s
            }
            Err(e) => {
                if !warned_missing {
                    warn!(device = %device.name, "not connected, retrying every {}s: {e:#}", RECONNECT_EVERY.as_secs());
                    warned_missing = true;
                }
                // Wait for the retry, but keep handling command results and
                // shutdown while the device is away.
                let until = Instant::now() + RECONNECT_EVERY;
                while let Some(left) = until.checked_duration_since(Instant::now()) {
                    match rx.recv_timeout(left) {
                        Ok(Event::Shutdown) => return Ok(()),
                        Ok(Event::Done { control, job, outcome }) => {
                            // No output while disconnected; just keep state.
                            let _ = engine.handle(now(), Input::Done { control, job, outcome });
                        }
                        Ok(Event::Midi(..)) => {}
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }
                continue;
            }
        };

        if let Some(out) = session.output.as_mut() {
            for msg in &device.init_messages {
                send(out, msg, &device.name);
            }
        }
        let actions = engine.startup(now());
        apply(actions, &mut session, &device, &tx);

        let mut next_check = Instant::now() + PORT_CHECK_EVERY;
        let lost = loop {
            let deadline = engine.next_deadline().map(|d| start + d);
            let wake = deadline.map_or(next_check, |d| d.min(next_check));
            let event = match rx.recv_deadline(wake) {
                Ok(e) => Some(e),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            };
            let input = match event {
                Some(Event::Shutdown) => {
                    let actions = engine.shutdown();
                    apply(actions, &mut session, &device, &tx);
                    info!(device = %device.name, "disconnected");
                    return Ok(());
                }
                Some(Event::Midi(at, msg)) => {
                    let t = at.saturating_duration_since(start);
                    engine.handle(t, Input::Midi(msg))
                }
                Some(Event::Done { control, job, outcome }) => {
                    engine.handle(now(), Input::Done { control, job, outcome })
                }
                None => engine.handle(now(), Input::Tick),
            };
            apply(input, &mut session, &device, &tx);

            if Instant::now() >= next_check {
                next_check = Instant::now() + PORT_CHECK_EVERY;
                if midi::select_port(&input_port_names(&watcher), &device.name).is_err() {
                    break true;
                }
            }
        };
        if lost {
            warn!(device = %device.name, "device disconnected; waiting for it to come back");
            warned_missing = true;
        }
        drop(session);
    }
}

fn send(out: &mut MidiOutputConnection, bytes: &[u8], device: &str) {
    debug!(device, bytes = %midi::hex(bytes), "send");
    if let Err(e) = out.send(bytes) {
        warn!(device, "could not send MIDI: {e}");
    }
}

fn apply(actions: Vec<Action>, session: &mut Session, device: &Device, tx: &Sender<Event>) {
    for action in actions {
        match action {
            Action::Send(bytes) => {
                if let Some(out) = session.output.as_mut() {
                    send(out, &bytes, &device.name);
                }
            }
            Action::Run { control, job, exec } => {
                let tx = tx.clone();
                exec::spawn(job, exec, move |outcome| {
                    let _ = tx.send(Event::Done { control, job, outcome });
                });
            }
        }
    }
}

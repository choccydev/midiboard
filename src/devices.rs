//! Device inspection and diagnostics: list, listen/record, send, led.

use crate::beatstep;
use crate::config::LedTarget;
use crate::midi::{self, MidiMsg};
use crate::runtime;
use anyhow::{Context, Result, bail};
use crossbeam_channel::unbounded;
use midir::{MidiInput, MidiOutput};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

pub fn list() -> Result<()> {
    let input = MidiInput::new("midiboard-list")?;
    let output = MidiOutput::new("midiboard-list")?;
    let inputs = runtime::input_port_names(&input);
    let outputs = runtime::output_port_names(&output);
    println!("Inputs (use as a device `name`):");
    if inputs.is_empty() {
        println!("  (none)");
    }
    for n in &inputs {
        println!("  {n}");
    }
    println!("\nOutputs (for feedback):");
    if outputs.is_empty() {
        println!("  (none)");
    }
    for n in &outputs {
        println!("  {n}");
    }
    Ok(())
}

enum Ev {
    Midi(u64, Vec<u8>),
    Stop,
}

/// Prints every message from `device`. With `record`, also writes a trace
/// for `midiboard replay`: one `t_us,hex bytes` line per message.
pub fn listen(device: &str, record: Option<&Path>) -> Result<()> {
    let (tx, rx) = unbounded::<Ev>();
    let start = Instant::now();
    let cb_tx = tx.clone();
    let _conn = runtime::open_input(device, "midiboard-listen", move |_stamp, bytes, _| {
        let t = start.elapsed().as_micros() as u64;
        let _ = cb_tx.send(Ev::Midi(t, bytes.to_vec()));
    })?;

    let mut writer = match record {
        Some(path) => {
            let f = File::create(path).with_context(|| format!("could not create {}", path.display()))?;
            let mut w = BufWriter::new(f);
            writeln!(w, "t_us,bytes")?;
            Some(w)
        }
        None => None,
    };

    let stop_tx = tx.clone();
    ctrlc::set_handler(move || {
        let _ = stop_tx.send(Ev::Stop);
    })?;
    let enter_tx = tx;
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        let _ = enter_tx.send(Ev::Stop);
    });

    eprintln!("Listening to \"{device}\". Press Enter or Ctrl-C to stop.");
    let mut count = 0u64;
    for ev in rx {
        match ev {
            Ev::Stop => break,
            Ev::Midi(t, bytes) => {
                count += 1;
                match MidiMsg::parse(&bytes) {
                    Some(msg) => println!("{msg}"),
                    None if bytes.first().is_some_and(|b| *b >= 0xF8) => continue, // clock etc.
                    None => println!("other: {}", midi::hex(&bytes)),
                }
                if let Some(w) = writer.as_mut() {
                    writeln!(w, "{t},{}", midi::hex(&bytes))?;
                }
            }
        }
    }
    if let Some(mut w) = writer {
        w.flush()?;
        eprintln!("Recorded {count} messages to {}.", record.map(|p| p.display().to_string()).unwrap_or_default());
    }
    Ok(())
}

pub fn send(device: &str, hex: &str) -> Result<()> {
    let bytes = midi::parse_hex(hex).map_err(anyhow::Error::msg)?;
    if bytes.is_empty() {
        bail!("nothing to send");
    }
    if bytes[0] == 0xF0 {
        midi::validate_sysex(&bytes).map_err(anyhow::Error::msg)?;
    } else if MidiMsg::parse(&bytes).is_none() {
        bail!("`{hex}` is not a complete channel message or SysEx");
    }
    let mut out = runtime::open_output(device, "midiboard-send")?;
    out.send(&bytes).context("could not send")?;
    eprintln!("Sent {}", midi::hex(&bytes));
    Ok(())
}

pub fn led(device: &str, pad: Option<u8>, button: Option<String>, color: &str) -> Result<()> {
    let id = beatstep::led_id(&LedTarget { pad, button }).map_err(anyhow::Error::msg)?;
    let c = beatstep::color(color).map_err(anyhow::Error::msg)?;
    send(device, &midi::hex(&beatstep::led_message(id, c)))
}

/// What `beatstep setup` should change.
pub struct BeatstepSetup {
    pub encoders: Vec<u8>,
    pub volume: bool,
    pub behavior: u8,
    pub acceleration: Option<String>,
    pub gate_pads: Option<Vec<u8>>,
}

pub fn beatstep_setup_messages(setup: &BeatstepSetup) -> Result<Vec<Vec<u8>>> {
    let mut msgs = Vec::new();
    let mut controls: Vec<u8> = setup.encoders.iter().map(|e| beatstep::encoder_id(*e)).collect();
    if setup.volume {
        controls.push(beatstep::VOLUME_KNOB);
    }
    for c in controls {
        msgs.extend(beatstep::encoder_setup_messages(c, setup.behavior));
    }
    if let Some(a) = &setup.acceleration {
        msgs.push(beatstep::acceleration_message(a).map_err(anyhow::Error::msg)?);
    }
    for pad in setup.gate_pads.iter().flatten() {
        msgs.push(beatstep::gate_mode_message(beatstep::pad_id(*pad)));
    }
    Ok(msgs)
}

pub fn beatstep_setup(device: &str, setup: &BeatstepSetup, dry_run: bool) -> Result<()> {
    let msgs = beatstep_setup_messages(setup)?;
    if dry_run {
        for m in &msgs {
            println!("{}", midi::hex(m));
        }
        return Ok(());
    }
    let mut out = runtime::open_output(device, "midiboard-setup")?;
    for m in &msgs {
        out.send(m).context("could not send")?;
        // The BeatStep drops messages that arrive back to back.
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let (name, mode) = beatstep::encoder_behavior_name(setup.behavior).unwrap_or(("?", "?"));
    eprintln!(
        "Sent {} messages. Encoders are now {name}; use \"mode\": \"{mode}\" for them in the config.\n\
         The change applies to the BeatStep's current settings. To keep it after a power cycle, \
         store it to a preset on the device or in MIDI Control Center.",
        msgs.len()
    );
    Ok(())
}

/// Reads and prints how each pad and encoder is set up.
pub fn beatstep_show(device: &str) -> Result<()> {
    let (tx, rx) = unbounded::<(u8, u8, u8)>();
    let _input = runtime::open_input(device, "midiboard-show", move |_stamp, bytes, _| {
        if let Some(reply) = beatstep::parse_reply(bytes) {
            let _ = tx.send(reply);
        }
    })?;
    let mut out = runtime::open_output(device, "midiboard-show")?;
    let mut get = |param: u8, control: u8| -> Result<Option<u8>> {
        out.send(&beatstep::get_message(param, control)).context("could not send")?;
        let deadline = Instant::now() + std::time::Duration::from_millis(300);
        while let Ok(reply) = rx.recv_deadline(deadline) {
            if reply.0 == param && reply.1 == control {
                return Ok(Some(reply.2));
            }
        }
        Ok(None)
    };

    let first = get(beatstep::PARAM_MODE, beatstep::encoder_id(1))?;
    if first.is_none() {
        bail!(
            "no reply from \"{device}\". Is it an original BeatStep, and is MIDI Control Center closed? \
             (It can hold the port.)"
        );
    }

    println!("Encoders:");
    let mut encoders: Vec<(String, u8)> = (1..=16).map(|e| (format!("{e:>2}"), beatstep::encoder_id(e))).collect();
    encoders.push(("volume".into(), beatstep::VOLUME_KNOB));
    for (label, id) in encoders {
        let mode = get(beatstep::PARAM_MODE, id)?;
        if mode != Some(beatstep::MODE_CC) {
            println!("  {label:>6}: not in CC mode (mode {})", mode.map_or("?".into(), |m| m.to_string()));
            continue;
        }
        let ch = get(beatstep::PARAM_CHANNEL, id)?.map_or("?".into(), |c| (c + 1).to_string());
        let cc = get(beatstep::PARAM_NUMBER, id)?.map_or("?".into(), |c| c.to_string());
        let behavior = get(beatstep::PARAM_BEHAVIOR, id)?;
        let mode = behavior
            .and_then(beatstep::encoder_behavior_name)
            .map_or("unknown".to_string(), |(n, m)| format!("{n} -> \"mode\": \"{m}\""));
        println!("  {label:>6}: \"type\": \"cc\", \"key\": {cc}, \"channel\": {ch}, {mode}");
    }

    println!("\nPads:");
    for pad in 1..=16u8 {
        let id = beatstep::pad_id(pad);
        let mode = get(beatstep::PARAM_MODE, id)?;
        let ch = get(beatstep::PARAM_CHANNEL, id)?.map_or("?".into(), |c| (c + 1).to_string());
        let num = get(beatstep::PARAM_NUMBER, id)?.map_or("?".into(), |c| c.to_string());
        let behavior = match get(beatstep::PARAM_BEHAVIOR, id)? {
            Some(0) => "toggle",
            Some(1) => "gate",
            _ => "?",
        };
        let kind = match mode {
            Some(9) => "\"type\": \"note\"",
            Some(8) => "\"type\": \"cc\"",
            Some(1) => "silent",
            Some(0) => "off",
            Some(0x0B) => "program change",
            _ => "unknown mode",
        };
        println!("  {pad:>6}: {kind}, \"key\": {num}, \"channel\": {ch}, {behavior}");
    }
    Ok(())
}

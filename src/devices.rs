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

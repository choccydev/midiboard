//! Offline replay of recorded traces through the engine, with a virtual
//! clock. Used by `midiboard replay` and by the encoder regression tests.

use crate::config::Device;
use crate::engine::{Action, Engine, Input, Job, Outcome};
use crate::midi::{self, MidiMsg};
use anyhow::{Context, Result, bail};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    /// An input event for the control: raw value and the position after it.
    Input { raw: u8 },
    /// A command execution, with the `{value}` it used.
    Exec { value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub t: Duration,
    pub kind: RowKind,
    pub position: f64,
}

/// Reads a trace written by `midiboard listen --record`.
pub fn read_trace(path: &Path) -> Result<Vec<(Duration, Vec<u8>)>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if n == 0 && line.starts_with("t_us") || line.trim().is_empty() {
            continue;
        }
        let (t, hex) = line.split_once(',').with_context(|| format!("line {}: expected `t_us,bytes`", n + 1))?;
        let t: u64 = t.trim().parse().with_context(|| format!("line {}: bad timestamp", n + 1))?;
        let bytes = midi::parse_hex(hex).map_err(|e| anyhow::anyhow!("line {}: {e}", n + 1))?;
        out.push((Duration::from_micros(t), bytes));
    }
    Ok(out)
}

/// Feeds `events` through a fresh engine. Commands "take" `exec_time` to
/// finish and always succeed; state reads never complete. Returns one row
/// per input to, and execution of, `control`.
pub fn simulate(
    device: &Device,
    control: &str,
    events: &[(Duration, MidiMsg)],
    exec_time: Duration,
) -> Result<Vec<Row>> {
    let mut engine = Engine::new(device);
    let Some(ci) = engine.control_index(control) else {
        bail!("no control named `{control}` on device \"{}\"", device.name);
    };
    let mut rows = Vec::new();
    // Pending command completions: (time, control).
    let mut pending: BinaryHeap<Reverse<(Duration, usize)>> = BinaryHeap::new();

    let record = |engine: &Engine,
                  t: Duration,
                  actions: Vec<Action>,
                  pending: &mut BinaryHeap<Reverse<(Duration, usize)>>,
                  rows: &mut Vec<Row>| {
        for a in actions {
            if let Action::Run { control, job: Job::Command, exec } = a {
                pending.push(Reverse((t + exec_time, control)));
                if control == ci {
                    rows.push(Row {
                        t,
                        kind: RowKind::Exec { value: exec.value.unwrap_or_default() },
                        position: engine.position(ci).unwrap_or(f64::NAN),
                    });
                }
            }
        }
    };

    let startup = engine.startup(Duration::ZERO);
    record(&engine, Duration::ZERO, startup, &mut pending, &mut rows);

    let mut i = 0;
    loop {
        let next_event = events.get(i).map(|e| e.0);
        let next_done = pending.peek().map(|r| r.0.0);
        let next_timer = engine.next_deadline();
        let Some(t) = [next_event, next_done, next_timer].into_iter().flatten().min() else { break };

        if next_done == Some(t) {
            let Reverse((_, control)) = pending.pop().expect("peeked");
            let a = engine.handle(
                t,
                Input::Done { control, job: Job::Command, outcome: Outcome { success: true, stdout: String::new() } },
            );
            record(&engine, t, a, &mut pending, &mut rows);
        } else if next_event == Some(t) {
            let msg = events[i].1;
            i += 1;
            let a = engine.handle(t, Input::Midi(msg));
            if let MidiMsg::ControlChange { controller, value, .. } = msg
                && controller == device.controls[ci].binding.number
            {
                rows.push(Row {
                    t,
                    kind: RowKind::Input { raw: value },
                    position: engine.position(ci).unwrap_or(f64::NAN),
                });
            }
            record(&engine, t, a, &mut pending, &mut rows);
        } else {
            let a = engine.handle(t, Input::Tick);
            record(&engine, t, a, &mut pending, &mut rows);
        }
    }
    Ok(rows)
}

pub fn run(devices: &[Device], trace: &Path, control: &str, exec_ms: u64, out: Option<&Path>) -> Result<()> {
    let device = devices
        .iter()
        .find(|d| d.controls.iter().any(|c| c.name == control))
        .with_context(|| format!("no control named `{control}` in the config"))?;
    let events: Vec<(Duration, MidiMsg)> =
        read_trace(trace)?.into_iter().filter_map(|(t, b)| MidiMsg::parse(&b).map(|m| (t, m))).collect();
    let rows = simulate(device, control, &events, Duration::from_millis(exec_ms))?;

    let mut w: Box<dyn Write> = match out {
        Some(p) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(p).with_context(|| format!("could not create {}", p.display()))?,
        )),
        None => Box::new(std::io::stdout().lock()),
    };
    writeln!(w, "t_us,event,raw,position,value")?;
    let (mut inputs, mut execs) = (0, 0);
    for r in &rows {
        let t = r.t.as_micros();
        match &r.kind {
            RowKind::Input { raw } => {
                inputs += 1;
                writeln!(w, "{t},input,{raw},{:.4},", r.position)?;
            }
            RowKind::Exec { value } => {
                execs += 1;
                writeln!(w, "{t},exec,,{:.4},{value}", r.position)?;
            }
        }
    }
    w.flush()?;
    eprintln!("{inputs} inputs, {execs} executions");
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Encoder regression tests on synthetic traces. Each checks the
    //! properties the old windowing broke: the final value is applied, the
    //! first output is immediate, sweeps never step backwards, and the
    //! number of executions stays bounded.

    use super::*;
    use crate::config;

    fn device(command: &str) -> Device {
        let text = format!(
            r#"{{"config_version":2,"devices":[{{"name":"x","controls":{{
                "vol":{{"key":7,"type":"cc","command":{{"kind":"Encoder",{command},"execute":{{"cmd":"set","args":["{{value}}"]}}}}}}}}}}]}}"#
        );
        config::compile(&config::parse(&text).unwrap()).unwrap().remove(0)
    }

    fn cc(t_us: u64, v: u8) -> (Duration, MidiMsg) {
        (Duration::from_micros(t_us), MidiMsg::ControlChange { channel: 1, controller: 7, value: v })
    }

    fn execs(rows: &[Row]) -> Vec<(Duration, f64)> {
        rows.iter()
            .filter_map(|r| match &r.kind {
                RowKind::Exec { value } => Some((r.t, value.parse().unwrap())),
                _ => None,
            })
            .collect()
    }

    fn check(rows: &[Row], rate_ms: u64, expect_final: f64, monotonic: Option<bool>) {
        let ex = execs(rows);
        assert!(!ex.is_empty(), "nothing executed");
        let first_input = rows.iter().find(|r| matches!(r.kind, RowKind::Input { .. })).unwrap().t;
        assert_eq!(ex[0].0, first_input, "leading edge must be immediate");
        assert_eq!(ex.last().unwrap().1, expect_final, "final value must be applied (trailing edge)");
        let last_input = rows.iter().rev().find(|r| matches!(r.kind, RowKind::Input { .. })).unwrap().t;
        let span = last_input - first_input;
        let bound = span.as_millis() as usize / rate_ms as usize + 2;
        assert!(ex.len() <= bound, "{} executions > bound {bound}", ex.len());
        for w in ex.windows(2) {
            assert!(w[1].0 - w[0].0 >= Duration::from_millis(rate_ms), "rate limit violated: {w:?}");
            if let Some(up) = monotonic {
                assert!(if up { w[1].1 > w[0].1 } else { w[1].1 < w[0].1 }, "direction reversed: {w:?}");
            }
        }
    }

    #[test]
    fn slow_absolute_sweep() {
        let dev = device(r#""map_min":0,"map_max":100,"rate_limit_ms":30"#);
        // One step every 50ms: every step should run.
        let events: Vec<_> = (1..=127).map(|v| cc(v as u64 * 50_000, v)).collect();
        let rows = simulate(&dev, "vol", &events, Duration::from_millis(5)).unwrap();
        check(&rows, 30, 100.0, Some(true));
    }

    #[test]
    fn fast_absolute_sweep_coalesces_but_lands_exactly() {
        let dev = device(r#""map_min":0,"map_max":100,"rate_limit_ms":30"#);
        // A full sweep in ~130ms (1 message/ms).
        let events: Vec<_> = (1..=127).map(|v| cc(v as u64 * 1_000, v)).collect();
        let rows = simulate(&dev, "vol", &events, Duration::from_millis(5)).unwrap();
        check(&rows, 30, 100.0, Some(true));
        assert!(execs(&rows).len() <= 7);
    }

    #[test]
    fn slow_command_never_piles_up() {
        let dev = device(r#""map_min":0,"map_max":100,"rate_limit_ms":10"#);
        let events: Vec<_> = (1..=127).map(|v| cc(v as u64 * 2_000, v)).collect();
        // Each command takes 100ms; the newest value must win each time.
        let rows = simulate(&dev, "vol", &events, Duration::from_millis(100)).unwrap();
        let ex = execs(&rows);
        for w in ex.windows(2) {
            assert!(w[1].0 - w[0].0 >= Duration::from_millis(100), "overlapping commands: {w:?}");
        }
        assert_eq!(ex.last().unwrap().1, 100.0);
        assert!(ex.len() <= 4);
    }

    #[test]
    fn jittery_pot_with_deadband() {
        let dev = device(r#""map_min":0,"map_max":127,"rate_limit_ms":20,"deadband":1"#);
        // Resting at 64 with ±1 noise, then a real move to 80 and noise again.
        let mut events = Vec::new();
        let mut t = 0;
        for v in [64, 65, 64, 65, 64, 64, 65] {
            events.push(cc(t, v));
            t += 40_000;
        }
        for v in 66..=80 {
            events.push(cc(t, v));
            t += 40_000;
        }
        for v in [79, 80, 79, 80] {
            events.push(cc(t, v));
            t += 40_000;
        }
        let rows = simulate(&dev, "vol", &events, Duration::from_millis(1)).unwrap();
        let values: Vec<f64> = execs(&rows).into_iter().map(|e| e.1).collect();
        // First input at 64, a single +1 step, then the real move; no flapping.
        assert_eq!(values.first(), Some(&64.0));
        assert_eq!(values.last(), Some(&80.0));
        let reversals = values.windows(2).filter(|w| w[1] < w[0]).count();
        assert_eq!(reversals, 0, "noise caused output reversals: {values:?}");
    }

    #[test]
    fn relative_spin_with_direction_flip() {
        let dev = device(r#""mode":"relative_offset64","map_min":0,"map_max":100,"initial":50,"rate_limit_ms":25"#);
        let mut events = Vec::new();
        let mut t = 0;
        for _ in 0..20 {
            events.push(cc(t, 65)); // +1
            t += 8_000;
        }
        t += 200_000;
        for _ in 0..30 {
            events.push(cc(t, 63)); // -1
            t += 8_000;
        }
        let rows = simulate(&dev, "vol", &events, Duration::from_millis(2)).unwrap();
        let ex = execs(&rows);
        assert_eq!(ex.last().unwrap().1, 40.0, "50 + 20 - 30");
        assert!(ex.iter().any(|e| e.1 == 70.0), "the peak must be executed before the flip");
    }

    #[test]
    fn burst_at_1khz_is_bounded() {
        let dev = device(r#""mode":"relative_twos","map_min":0,"map_max":1000,"rate_limit_ms":30"#);
        let events: Vec<_> = (0..1000).map(|i| cc(i * 1_000, 1)).collect();
        let rows = simulate(&dev, "vol", &events, Duration::from_millis(1)).unwrap();
        check(&rows, 30, 1000.0, Some(true));
    }

    #[test]
    fn trace_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.csv");
        std::fs::write(&p, "t_us,bytes\n0,B0 07 00\n1000,B0 07 7F\n1500,F8\n").unwrap();
        let t = read_trace(&p).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t[1], (Duration::from_micros(1000), vec![0xB0, 7, 0x7F]));
    }
}

//! The per-device event engine.
//!
//! The engine is a pure state machine: it takes inputs stamped with a time,
//! and returns actions (run a command, send MIDI bytes). It never touches the
//! clock, processes or MIDI ports itself, so it can be driven by the real
//! runtime, by `midiboard replay`, or by tests with a virtual clock.

use crate::config::{Behavior, Control, Device, Exec, FbTarget, MsgType};
use crate::encoder::{EncoderState, Time};
use crate::midi::MidiMsg;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{debug, info, warn};

const TRIGGER_QUEUE_MAX: u8 = 4;
const ERROR_FLASH: Duration = Duration::from_millis(500);
const STATE_WARN_EVERY: Duration = Duration::from_secs(60);

static FIRST_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?\d+(?:\.\d+)?").expect("static regex is valid"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Command,
    State,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub success: bool,
    pub stdout: String,
}

#[derive(Debug, Clone)]
pub enum Input {
    Midi(MidiMsg),
    Done {
        control: usize,
        job: Job,
        outcome: Outcome,
    },
    /// Nothing happened; process timers that are due.
    Tick,
}

/// A command ready to run: placeholders substituted, shell applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub control: String,
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Option<Duration>,
    /// The `{value}` used, for logs and replay output.
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Run { control: usize, job: Job, exec: Rendered },
    Send(Vec<u8>),
}

/// Wraps a value in single quotes for `sh`.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn render_exec(control: &str, exec: &Exec, vars: &[(&str, &str)]) -> Rendered {
    let value = vars.iter().find(|(k, _)| *k == "value").map(|(_, v)| v.to_string());
    let subst = |text: &str, quote: bool| {
        let mut out = text.to_string();
        for (k, v) in vars {
            let token = format!("{{{k}}}");
            if out.contains(&token) {
                let v = if quote { shell_quote(v) } else { v.to_string() };
                out = out.replace(&token, &v);
            }
        }
        out
    };
    if exec.shell {
        Rendered {
            control: control.to_string(),
            program: "sh".into(),
            args: vec!["-c".into(), subst(&exec.cmd, true)],
            timeout: exec.timeout,
            value,
        }
    } else {
        Rendered {
            control: control.to_string(),
            program: subst(&exec.cmd, false),
            args: exec.args.iter().map(|a| subst(a, false)).collect(),
            timeout: exec.timeout,
            value,
        }
    }
}

#[derive(Debug)]
enum Kind {
    Encoder(EncoderState),
    /// `on` is the confirmed state; `target` is what the running command
    /// will switch to if it succeeds.
    Switch {
        on: Option<bool>,
        target: bool,
        pending_toggle: bool,
    },
    Trigger {
        queued: u8,
    },
}

#[derive(Debug)]
struct Slot {
    ctl: Control,
    kind: Kind,
    in_flight: bool,
    state_in_flight: bool,
    last_press: Option<Time>,
    cc_last: Option<u8>,
    flash_off_at: Option<Time>,
    next_poll: Option<Time>,
    last_echo: Option<u8>,
    last_state_warn: Option<Time>,
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Press(u8),
    Release,
    Value(u8),
}

pub struct Engine {
    slots: Vec<Slot>,
    by_input: HashMap<(MsgType, u8), Vec<usize>>,
}

impl Engine {
    pub fn new(device: &Device) -> Self {
        let mut by_input: HashMap<(MsgType, u8), Vec<usize>> = HashMap::new();
        let slots = device
            .controls
            .iter()
            .enumerate()
            .map(|(i, c)| {
                by_input.entry((c.binding.msg_type, c.binding.number)).or_default().push(i);
                let kind = match &c.behavior {
                    Behavior::Encoder(spec) => Kind::Encoder(EncoderState::new(spec)),
                    Behavior::Switch(spec) => Kind::Switch { on: spec.initial, target: false, pending_toggle: false },
                    Behavior::Trigger(_) => Kind::Trigger { queued: 0 },
                };
                Slot {
                    ctl: c.clone(),
                    kind,
                    in_flight: false,
                    state_in_flight: false,
                    last_press: None,
                    cc_last: None,
                    flash_off_at: None,
                    next_poll: None,
                    last_echo: None,
                    last_state_warn: None,
                }
            })
            .collect();
        Engine { slots, by_input }
    }

    pub fn control_index(&self, name: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.ctl.name == name)
    }

    /// Current encoder position, for replay output and tests.
    pub fn position(&self, control: usize) -> Option<f64> {
        match &self.slots.get(control)?.kind {
            Kind::Encoder(st) => Some(st.pos),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn switch_state(&self, control: usize) -> Option<bool> {
        match &self.slots.get(control)?.kind {
            Kind::Switch { on, .. } => *on,
            _ => None,
        }
    }

    /// Call after every (re)connect: paints feedback and reads state.
    pub fn startup(&mut self, now: Time) -> Vec<Action> {
        let mut out = Vec::new();
        for i in 0..self.slots.len() {
            self.slots[i].last_echo = None;
            self.paint(i, &mut out);
            if self.slots[i].ctl.state.is_some() {
                self.start_state_read(i, now, &mut out);
            }
        }
        out
    }

    /// Call on shutdown: turns every LED off.
    pub fn shutdown(&self) -> Vec<Action> {
        self.slots
            .iter()
            .filter_map(|s| match s.ctl.feedback.as_ref().map(|f| &f.target) {
                Some(FbTarget::Messages { off, .. }) => Some(Action::Send(off.clone())),
                _ => None,
            })
            .collect()
    }

    /// The earliest time a timer needs [`Input::Tick`], if any.
    pub fn next_deadline(&self) -> Option<Time> {
        self.slots
            .iter()
            .flat_map(|s| {
                let enc = match (&s.kind, &s.ctl.behavior) {
                    (Kind::Encoder(st), Behavior::Encoder(spec)) if !s.in_flight => st.deadline(spec),
                    _ => None,
                };
                let poll = if s.in_flight || s.state_in_flight { None } else { s.next_poll };
                [enc, s.flash_off_at, poll]
            })
            .flatten()
            .min()
    }

    pub fn handle(&mut self, now: Time, input: Input) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Midi(msg) => self.on_midi(now, msg, &mut out),
            Input::Done { control, job, outcome } => self.on_done(now, control, job, outcome, &mut out),
            Input::Tick => {}
        }
        self.run_timers(now, &mut out);
        out
    }

    fn on_midi(&mut self, now: Time, msg: MidiMsg, out: &mut Vec<Action>) {
        let (msg_type, number, channel, signal) = match msg {
            MidiMsg::NoteOn { channel, note, velocity } => (MsgType::Note, note, channel, Signal::Press(velocity)),
            MidiMsg::NoteOff { channel, note, .. } => (MsgType::Note, note, channel, Signal::Release),
            MidiMsg::ControlChange { channel, controller, value } => {
                (MsgType::Cc, controller, channel, Signal::Value(value))
            }
            _ => return,
        };
        let Some(candidates) = self.by_input.get(&(msg_type, number)) else {
            return;
        };
        let Some(&i) = candidates.iter().find(|&&i| self.slots[i].ctl.binding.channel.is_none_or(|c| c == channel))
        else {
            return;
        };
        debug!(control = %self.slots[i].ctl.name, %msg, "input");

        let is_encoder = matches!(self.slots[i].kind, Kind::Encoder(_));
        if is_encoder {
            if let Signal::Value(v) = signal {
                let slot = &mut self.slots[i];
                if let (Kind::Encoder(st), Behavior::Encoder(spec)) = (&mut slot.kind, &slot.ctl.behavior) {
                    st.input(spec, now, v);
                }
                // Execution, if due, happens in run_timers.
            }
            return;
        }

        // Buttons: turn note on/off and CC values into press edges.
        let press = match signal {
            Signal::Press(v) => Some(v),
            Signal::Release => None,
            Signal::Value(v) => {
                let prev = self.slots[i].cc_last.replace(v);
                // A CC button usually sends 127 then 0. Some never send 0 and
                // repeat the same value; treat a repeat as a new press too.
                (v > 0 && prev.is_none_or(|p| p == 0 || p == v)).then_some(v)
            }
        };
        let Some(velocity) = press else { return };

        let debounce = match &self.slots[i].ctl.behavior {
            Behavior::Switch(s) => s.debounce,
            Behavior::Trigger(t) => t.debounce,
            Behavior::Encoder(_) => unreachable!(),
        };
        if self.slots[i].last_press.is_some_and(|t| now.saturating_sub(t) < debounce) {
            debug!(control = %self.slots[i].ctl.name, "press ignored (debounce)");
            return;
        }
        self.slots[i].last_press = Some(now);

        let busy = self.slots[i].in_flight;
        match self.slots[i].kind {
            Kind::Switch { ref mut pending_toggle, .. } => {
                if busy {
                    // Two presses while busy cancel out.
                    *pending_toggle = !*pending_toggle;
                } else {
                    self.start_toggle(i, out);
                }
            }
            Kind::Trigger { ref mut queued } => {
                if busy {
                    *queued = (*queued + 1).min(TRIGGER_QUEUE_MAX);
                } else {
                    self.start_trigger(i, now, velocity, out);
                }
            }
            Kind::Encoder(_) => unreachable!(),
        }
    }

    fn start_toggle(&mut self, i: usize, out: &mut Vec<Action>) {
        let slot = &mut self.slots[i];
        let (Kind::Switch { on, target, .. }, Behavior::Switch(spec)) = (&mut slot.kind, &slot.ctl.behavior) else {
            return;
        };
        *target = !on.unwrap_or(false);
        let exec = if *target { &spec.on } else { &spec.off };
        let rendered = render_exec(&slot.ctl.name, exec, &[]);
        info!(control = %slot.ctl.name, to = if *target { "on" } else { "off" }, "switch");
        slot.in_flight = true;
        out.push(Action::Run { control: i, job: Job::Command, exec: rendered });
    }

    fn start_trigger(&mut self, i: usize, now: Time, velocity: u8, out: &mut Vec<Action>) {
        let slot = &mut self.slots[i];
        let Behavior::Trigger(spec) = &slot.ctl.behavior else { return };
        let v = velocity.to_string();
        let rendered = render_exec(&slot.ctl.name, &spec.exec, &[("velocity", &v)]);
        info!(control = %slot.ctl.name, "trigger");
        slot.in_flight = true;
        out.push(Action::Run { control: i, job: Job::Command, exec: rendered });
        if let Some(fb) = &slot.ctl.feedback
            && let FbTarget::Messages { on, .. } = &fb.target
        {
            if fb.while_running {
                out.push(Action::Send(on.clone()));
            } else if let Some(flash) = fb.flash {
                out.push(Action::Send(on.clone()));
                slot.flash_off_at = Some(now + flash);
            }
        }
    }

    fn start_state_read(&mut self, i: usize, now: Time, out: &mut Vec<Action>) {
        let slot = &mut self.slots[i];
        let Some(src) = &slot.ctl.state else { return };
        if slot.state_in_flight {
            return;
        }
        slot.state_in_flight = true;
        slot.next_poll = src.poll.map(|p| now + p);
        out.push(Action::Run { control: i, job: Job::State, exec: render_exec(&slot.ctl.name, &src.exec, &[]) });
    }

    fn on_done(&mut self, now: Time, i: usize, job: Job, outcome: Outcome, out: &mut Vec<Action>) {
        if i >= self.slots.len() {
            return;
        }
        match job {
            Job::Command => self.on_command_done(now, i, outcome, out),
            Job::State => self.on_state_done(now, i, outcome, out),
        }
    }

    fn on_command_done(&mut self, now: Time, i: usize, outcome: Outcome, out: &mut Vec<Action>) {
        self.slots[i].in_flight = false;
        let name = self.slots[i].ctl.name.clone();
        if !outcome.success {
            warn!(control = %name, "command failed");
        }
        let error_msg = match self.slots[i].ctl.feedback.as_ref().map(|f| &f.target) {
            Some(FbTarget::Messages { error: Some(e), .. }) if !outcome.success => Some(e.clone()),
            _ => None,
        };

        let has_state = self.slots[i].ctl.state.is_some();
        match self.slots[i].kind {
            Kind::Switch { ref mut on, target, ref mut pending_toggle } => {
                if outcome.success {
                    // The state only changes once the command worked, so the
                    // LED never claims a state the system isn't in.
                    *on = Some(target);
                }
                let again = std::mem::take(pending_toggle);
                if let Some(e) = error_msg {
                    out.push(Action::Send(e));
                    self.slots[i].flash_off_at = Some(now + ERROR_FLASH);
                } else {
                    self.paint(i, out);
                }
                if has_state {
                    // Confirm what the command actually did.
                    self.start_state_read(i, now, out);
                }
                if again {
                    self.start_toggle(i, out);
                }
            }
            Kind::Trigger { ref mut queued } => {
                let again = *queued > 0;
                if again {
                    *queued -= 1;
                }
                let fb = self.slots[i].ctl.feedback.clone();
                if let Some(e) = error_msg {
                    out.push(Action::Send(e));
                    let flash = fb.as_ref().and_then(|f| f.flash).unwrap_or(ERROR_FLASH);
                    self.slots[i].flash_off_at = Some(now + flash.max(ERROR_FLASH));
                } else if let Some(f) = &fb
                    && f.while_running
                    && let FbTarget::Messages { off, .. } = &f.target
                {
                    out.push(Action::Send(off.clone()));
                }
                if again {
                    self.start_trigger(i, now, 127, out);
                }
            }
            Kind::Encoder(_) => {
                // A pending value, if any, runs from run_timers.
            }
        }
    }

    fn on_state_done(&mut self, now: Time, i: usize, outcome: Outcome, out: &mut Vec<Action>) {
        let slot = &mut self.slots[i];
        slot.state_in_flight = false;
        let Some(src) = slot.ctl.state.clone() else { return };
        if let Some(p) = src.poll {
            slot.next_poll = Some(now + p);
        }
        let name = slot.ctl.name.clone();
        let warn_once = |slot: &mut Slot, msg: &str| {
            if slot.last_state_warn.is_none_or(|t| now.saturating_sub(t) >= STATE_WARN_EVERY) {
                warn!(control = %name, "{msg}");
                slot.last_state_warn = Some(now);
            }
        };
        match (&mut slot.kind, &slot.ctl.behavior) {
            (Kind::Switch { on, .. }, _) => {
                let value = match &src.pattern {
                    Some(re) if outcome.success => re.is_match(&outcome.stdout),
                    Some(_) => {
                        warn_once(slot, "state command failed; keeping the last known state");
                        return;
                    }
                    None => outcome.success,
                };
                if slot.in_flight {
                    return; // the running command will settle the state
                }
                if *on != Some(value) {
                    debug!(control = %name, state = value, "state changed externally");
                    *on = Some(value);
                    self.paint(i, out);
                }
            }
            (Kind::Encoder(st), Behavior::Encoder(spec)) => {
                if !outcome.success {
                    warn_once(slot, "state command failed; keeping the last known value");
                    return;
                }
                let number = match &src.pattern {
                    Some(re) => re
                        .captures(&outcome.stdout)
                        .and_then(|c| c.get(1).or_else(|| c.get(0)))
                        .map(|m| m.as_str().to_string()),
                    None => FIRST_NUMBER.find(&outcome.stdout).map(|m| m.as_str().to_string()),
                };
                let Some(value) = number.and_then(|n| n.trim().parse::<f64>().ok()) else {
                    warn_once(slot, "state command printed no number; keeping the last known value");
                    return;
                };
                if slot.in_flight || st.recently_active(spec, now) {
                    return; // don't fight the user's hand
                }
                st.sync(spec, value);
                self.echo(i, out);
            }
            _ => {}
        }
    }

    fn run_timers(&mut self, now: Time, out: &mut Vec<Action>) {
        for i in 0..self.slots.len() {
            if self.slots[i].flash_off_at.is_some_and(|t| now >= t) {
                self.slots[i].flash_off_at = None;
                self.paint(i, out);
            }

            let slot = &mut self.slots[i];
            if !slot.in_flight
                && let (Kind::Encoder(st), Behavior::Encoder(spec)) = (&mut slot.kind, &slot.ctl.behavior)
                && st.due(spec, now)
                && let Some(em) = st.take(spec, now)
            {
                let exec = render_exec(&slot.ctl.name, &spec.exec, &[("value", &em.value), ("delta", &em.delta)]);
                debug!(control = %slot.ctl.name, value = %em.value, delta = %em.delta, "encoder");
                slot.in_flight = true;
                out.push(Action::Run { control: i, job: Job::Command, exec });
                self.echo(i, out);
            }

            let slot = &self.slots[i];
            if !slot.in_flight && !slot.state_in_flight && slot.next_poll.is_some_and(|t| now >= t) {
                self.start_state_read(i, now, out);
            }
        }
    }

    /// Sends the feedback that matches the control's current state.
    fn paint(&mut self, i: usize, out: &mut Vec<Action>) {
        let slot = &self.slots[i];
        let Some(fb) = &slot.ctl.feedback else { return };
        match &fb.target {
            FbTarget::Messages { on, off, .. } => {
                let lit = match slot.kind {
                    Kind::Switch { on: state, .. } => state.unwrap_or(false),
                    Kind::Trigger { .. } => fb.while_running && slot.in_flight,
                    Kind::Encoder(_) => false,
                };
                out.push(Action::Send(if lit { on.clone() } else { off.clone() }));
            }
            FbTarget::CcEcho { .. } => self.echo(i, out),
        }
    }

    fn echo(&mut self, i: usize, out: &mut Vec<Action>) {
        let slot = &mut self.slots[i];
        let (Some(fb), Kind::Encoder(st), Behavior::Encoder(spec)) =
            (&slot.ctl.feedback, &slot.kind, &slot.ctl.behavior)
        else {
            return;
        };
        if let FbTarget::CcEcho { channel, number } = fb.target {
            let v = st.cc_value(spec);
            if slot.last_echo != Some(v) {
                slot.last_echo = Some(v);
                out.push(Action::Send(vec![0xB0 | (channel - 1), number, v]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn device(controls: &str) -> Device {
        let text = format!(r#"{{"config_version":2,"devices":[{{"name":"x","controls":{{{controls}}}}}]}}"#);
        config::compile(&config::parse(&text).unwrap()).unwrap().remove(0)
    }

    fn ms(n: u64) -> Time {
        Duration::from_millis(n)
    }

    fn note_on(n: u8) -> Input {
        Input::Midi(MidiMsg::NoteOn { channel: 1, note: n, velocity: 100 })
    }
    fn note_off(n: u8) -> Input {
        Input::Midi(MidiMsg::NoteOff { channel: 1, note: n, velocity: 0 })
    }
    fn done(control: usize, success: bool) -> Input {
        Input::Done { control, job: Job::Command, outcome: Outcome { success, stdout: String::new() } }
    }
    fn runs(actions: &[Action]) -> Vec<&Rendered> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Run { job: Job::Command, exec, .. } => Some(exec),
                _ => None,
            })
            .collect()
    }
    fn sends(actions: &[Action]) -> Vec<&Vec<u8>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Send(b) => Some(b),
                _ => None,
            })
            .collect()
    }

    const TRIGGER: &str =
        r#""t":{"key":36,"type":"note","command":{"kind":"Trigger","execute":{"cmd":"echo","args":["{velocity}"]}}}"#;

    #[test]
    fn trigger_fires_on_every_press() {
        // The old runtime swallowed the first press and every other one after.
        let dev = device(TRIGGER);
        let mut e = Engine::new(&dev);
        e.startup(ms(0));
        for k in 0..5u64 {
            let t = 1000 * k;
            let a = e.handle(ms(t), note_on(36));
            assert_eq!(runs(&a).len(), 1, "press {k}");
            assert_eq!(runs(&a)[0].args, vec!["100"]);
            assert!(runs(&e.handle(ms(t + 20), note_off(36))).is_empty(), "release must not fire");
            e.handle(ms(t + 30), done(0, true));
        }
    }

    #[test]
    fn trigger_debounce_and_queue() {
        let dev = device(TRIGGER);
        let mut e = Engine::new(&dev);
        assert_eq!(runs(&e.handle(ms(0), note_on(36))).len(), 1);
        assert!(runs(&e.handle(ms(10), note_on(36))).is_empty(), "within debounce");
        assert!(runs(&e.handle(ms(100), note_on(36))).is_empty(), "queued while running");
        assert_eq!(runs(&e.handle(ms(200), done(0, true))).len(), 1, "queued press runs");
    }

    const SWITCH: &str = r#""s":{"key":40,"type":"note","command":{"kind":"Switch","initial_state":"off",
        "on":{"cmd":"on"},"off":{"cmd":"off"}},"feedback":{"note":{"number":40},"error":64}}"#;

    #[test]
    fn switch_first_press_goes_opposite_of_initial_state() {
        // The old runtime always ran `off` first.
        let dev = device(SWITCH);
        let mut e = Engine::new(&dev);
        let a = e.startup(ms(0));
        assert_eq!(sends(&a), vec![&vec![0x80, 40, 0]], "painted off at startup");
        let a = e.handle(ms(0), note_on(40));
        assert_eq!(runs(&a)[0].program, "on");
        let a = e.handle(ms(10), done(0, true));
        assert_eq!(sends(&a), vec![&vec![0x90, 40, 127]]);
        assert_eq!(e.switch_state(0), Some(true));
        let a = e.handle(ms(100), note_on(40));
        assert_eq!(runs(&a)[0].program, "off");
    }

    #[test]
    fn switch_keeps_state_when_command_fails() {
        let dev = device(SWITCH);
        let mut e = Engine::new(&dev);
        e.handle(ms(0), note_on(40));
        let a = e.handle(ms(10), done(0, false));
        assert_eq!(e.switch_state(0), Some(false));
        assert_eq!(sends(&a), vec![&vec![0x90, 40, 64]], "error color");
        let a = e.handle(ms(600), Input::Tick);
        assert_eq!(sends(&a), vec![&vec![0x80, 40, 0]], "back to the real state");
        let a = e.handle(ms(700), note_on(40));
        assert_eq!(runs(&a)[0].program, "on", "retries the same transition");
    }

    #[test]
    fn switch_presses_while_busy_cancel_out() {
        let dev = device(SWITCH);
        let mut e = Engine::new(&dev);
        e.handle(ms(0), note_on(40));
        e.handle(ms(100), note_on(40));
        e.handle(ms(200), note_on(40));
        let a = e.handle(ms(300), done(0, true));
        assert!(runs(&a).is_empty());
        e.handle(ms(400), note_on(40));
        let a = e.handle(ms(500), done(0, true));
        assert!(runs(&a).is_empty());
        let a = e.handle(ms(600), note_on(40));
        assert_eq!(runs(&a)[0].program, "on", "the two busy presses cancelled out");
    }

    #[test]
    fn cc_buttons_use_press_edges() {
        let dev =
            device(r#""t":{"key":20,"type":"cc","command":{"kind":"Trigger","debounce_ms":0,"execute":{"cmd":"x"}}}"#);
        let mut e = Engine::new(&dev);
        let cc = |v| Input::Midi(MidiMsg::ControlChange { channel: 1, controller: 20, value: v });
        assert_eq!(runs(&e.handle(ms(0), cc(127))).len(), 1);
        e.handle(ms(1), done(0, true));
        assert!(runs(&e.handle(ms(2), cc(0))).is_empty());
        assert_eq!(runs(&e.handle(ms(3), cc(127))).len(), 1);
        e.handle(ms(4), done(0, true));
        assert_eq!(runs(&e.handle(ms(5), cc(127))).len(), 1, "repeat without 0 is a new press");
    }

    #[test]
    fn channel_and_type_are_matched() {
        let dev = device(
            r#""a":{"key":36,"type":"note","channel":2,"command":{"kind":"Trigger","debounce_ms":0,"execute":{"cmd":"a"}}},
               "b":{"key":36,"type":"cc","command":{"kind":"Trigger","debounce_ms":0,"execute":{"cmd":"b"}}}"#,
        );
        let mut e = Engine::new(&dev);
        assert!(runs(&e.handle(ms(0), note_on(36))).is_empty(), "channel 1 doesn't match");
        let a = e.handle(ms(1), Input::Midi(MidiMsg::NoteOn { channel: 2, note: 36, velocity: 1 }));
        assert_eq!(runs(&a)[0].program, "a");
        let a = e.handle(ms(2), Input::Midi(MidiMsg::ControlChange { channel: 5, controller: 36, value: 9 }));
        assert_eq!(runs(&a)[0].program, "b");
    }

    #[test]
    fn trigger_flash_and_while_running() {
        let dev = device(
            r#""f":{"key":1,"type":"note","command":{"kind":"Trigger","execute":{"cmd":"a"}},
                    "feedback":{"note":{"number":1},"flash_ms":100}},
               "w":{"key":2,"type":"note","command":{"kind":"Trigger","execute":{"cmd":"b"}},
                    "feedback":{"note":{"number":2},"while_running":true}}"#,
        );
        let mut e = Engine::new(&dev);
        let a = e.handle(ms(0), note_on(1));
        assert_eq!(sends(&a), vec![&vec![0x90, 1, 127]]);
        assert_eq!(e.next_deadline(), Some(ms(100)));
        let a = e.handle(ms(100), Input::Tick);
        assert_eq!(sends(&a), vec![&vec![0x80, 1, 0]]);

        let a = e.handle(ms(200), note_on(2));
        assert_eq!(sends(&a), vec![&vec![0x90, 2, 127]]);
        let a = e.handle(ms(900), done(1, true));
        assert_eq!(sends(&a), vec![&vec![0x80, 2, 0]]);
    }

    #[test]
    fn shell_placeholders_are_quoted() {
        let exec = Exec { cmd: "echo {value} | cat".into(), args: vec![], shell: true, timeout: None };
        let r = render_exec("c", &exec, &[("value", "5; rm -rf /")]);
        assert_eq!(r.program, "sh");
        assert_eq!(r.args, vec!["-c", "echo '5; rm -rf /' | cat"]);
        let exec = Exec { cmd: "pactl".into(), args: vec!["{value}%".into()], shell: false, timeout: None };
        assert_eq!(render_exec("c", &exec, &[("value", "42")]).args, vec!["42%"]);
    }

    const MUTE_WITH_STATE: &str = r#""m":{"key":5,"type":"note","command":{"kind":"Switch","on":{"cmd":"on"},"off":{"cmd":"off"}},
        "state":{"cmd":"probe","match":"yes","poll_ms":1000},"feedback":{"note":{"number":5}}}"#;

    fn state_done(control: usize, stdout: &str) -> Input {
        Input::Done { control, job: Job::State, outcome: Outcome { success: true, stdout: stdout.into() } }
    }

    #[test]
    fn switch_follows_external_state() {
        let dev = device(MUTE_WITH_STATE);
        let mut e = Engine::new(&dev);
        let a = e.startup(ms(0));
        assert!(a.iter().any(|x| matches!(x, Action::Run { job: Job::State, .. })));
        let a = e.handle(ms(10), state_done(0, "Mute: yes"));
        assert_eq!(e.switch_state(0), Some(true));
        assert_eq!(sends(&a), vec![&vec![0x90, 5, 127]]);
        // Polls again after poll_ms.
        assert_eq!(e.next_deadline(), Some(ms(1010)));
        let a = e.handle(ms(1010), Input::Tick);
        assert!(a.iter().any(|x| matches!(x, Action::Run { job: Job::State, .. })));
        let a = e.handle(ms(1020), state_done(0, "Mute: no"));
        assert_eq!(e.switch_state(0), Some(false));
        assert_eq!(sends(&a), vec![&vec![0x80, 5, 0]]);
        // A press now goes to "on" based on the synced state.
        let a = e.handle(ms(1100), note_on(5));
        assert_eq!(runs(&a)[0].program, "on");
    }

    #[test]
    fn shutdown_turns_leds_off() {
        let dev = device(SWITCH);
        let e = Engine::new(&dev);
        assert_eq!(e.shutdown(), vec![Action::Send(vec![0x80, 40, 0])]);
    }
}

//! Config file format (version 2), loading, and validation.
//!
//! The raw serde types mirror the JSON file. [`compile`] checks every
//! semantic rule and turns them into the runtime types the engine uses, so a
//! config that passes `config --validate` can't fail later at runtime.

use crate::beatstep;
use crate::midi;
use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CONFIG_VERSION: u64 = 2;
#[cfg(test)]
pub const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/choccydev/midiboard/master/schema/midiboard.schema.json";
pub const MIGRATION_URL: &str = "https://github.com/choccydev/midiboard/blob/master/docs/migration-0.5.md";
pub const SKELETON: &str = include_str!("../schema/midiboard.json");

// ---------------------------------------------------------------------------
// Raw file format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    // Read so `deny_unknown_fields` accepts them; checked in `parse`.
    #[serde(rename = "$schema", default)]
    #[allow(dead_code)]
    pub schema: Option<String>,
    #[allow(dead_code)]
    pub config_version: u64,
    #[serde(default)]
    pub log_level: LogLevel,
    pub devices: Vec<DeviceConfig>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    #[default]
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_filter(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    /// Input port to listen to. See [`midi::select_port`] for matching rules.
    pub name: String,
    #[serde(default)]
    pub output: Option<OutputConfig>,
    pub controls: BTreeMap<String, ControlConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    /// Output port; defaults to the device `name`.
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub profile: Option<Profile>,
    /// BeatStep only: switch pads with LED feedback to gate mode on connect,
    /// so the device stops driving those LEDs itself.
    #[serde(default)]
    pub take_over_pads: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Beatstep,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlConfig {
    pub key: u8,
    #[serde(rename = "type")]
    pub msg_type: MsgType,
    #[serde(default)]
    pub channel: Option<u8>,
    pub command: CommandConfig,
    #[serde(default)]
    pub feedback: Option<FeedbackConfig>,
    #[serde(default)]
    pub state: Option<StateConfig>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MsgType {
    Note,
    Cc,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum CommandConfig {
    Encoder(EncoderConfig),
    Switch(SwitchConfig),
    Trigger(TriggerConfig),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ExecConfig {
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EncoderMode {
    #[default]
    Absolute,
    /// 64 = no change, 65.. = +1.., 63.. = -1.. (BeatStep "Relative #1").
    RelativeOffset64,
    /// Two's complement: 1..63 = +1..+63, 127..65 = -1..-63 ("Relative #2").
    RelativeTwos,
    /// Sign bit: 1..63 = +1..+63, 65..127 = -1..-63.
    RelativeSignbit,
    /// 16 = no change, 17.. = +1.., 15.. = -1.. ("Relative #3").
    RelativeOffset16,
}

fn d_min() -> f64 {
    0.0
}
fn d_max() -> f64 {
    127.0
}
fn d_step() -> f64 {
    1.0
}
fn d_rate() -> u64 {
    30
}
fn d_debounce() -> u64 {
    50
}
fn d_curve() -> f64 {
    1.5
}
fn d_gain() -> f64 {
    4.0
}
fn d_slow() -> u64 {
    80
}
fn d_channel() -> u8 {
    1
}
fn d_state_timeout() -> u64 {
    2000
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncoderConfig {
    #[serde(default)]
    pub mode: EncoderMode,
    #[serde(default = "d_min")]
    pub map_min: f64,
    #[serde(default = "d_max")]
    pub map_max: f64,
    #[serde(default = "d_step")]
    pub step: f64,
    #[serde(default)]
    pub acceleration: Option<AccelConfig>,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default = "d_rate")]
    pub rate_limit_ms: u64,
    #[serde(default)]
    pub deadband: u8,
    #[serde(default)]
    pub initial: Option<f64>,
    #[serde(default)]
    pub precision: u8,
    pub execute: ExecConfig,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct AccelConfig {
    #[serde(default = "d_curve")]
    pub curve: f64,
    #[serde(default = "d_gain")]
    pub max_gain: f64,
    /// Ticks further apart than this get no acceleration.
    #[serde(default = "d_slow")]
    pub slow_ms: u64,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OnOff {
    #[serde(alias = "ON")]
    On,
    #[serde(alias = "OFF")]
    Off,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchConfig {
    pub on: ExecConfig,
    pub off: ExecConfig,
    #[serde(default)]
    pub initial_state: Option<OnOff>,
    #[serde(default = "d_debounce")]
    pub debounce_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerConfig {
    pub execute: ExecConfig,
    #[serde(default = "d_debounce")]
    pub debounce_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackConfig {
    #[serde(default)]
    pub led: Option<LedTarget>,
    #[serde(default)]
    pub note: Option<MidiTarget>,
    #[serde(default)]
    pub cc: Option<MidiTarget>,
    #[serde(default)]
    pub sysex: Option<SysexTarget>,
    #[serde(default)]
    pub on: Option<FbValue>,
    #[serde(default)]
    pub off: Option<FbValue>,
    #[serde(default)]
    pub error: Option<FbValue>,
    #[serde(default)]
    pub flash_ms: Option<u64>,
    #[serde(default)]
    pub while_running: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LedTarget {
    #[serde(default)]
    pub pad: Option<u8>,
    #[serde(default)]
    pub button: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct MidiTarget {
    pub number: u8,
    #[serde(default = "d_channel")]
    pub channel: u8,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SysexTarget {
    pub on: String,
    pub off: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum FbValue {
    Raw(u8),
    Name(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub shell: bool,
    #[serde(default = "d_state_timeout")]
    pub timeout_ms: u64,
    /// Switch: regex that means "on" when it matches stdout (otherwise exit
    /// status decides). Encoder: regex whose first capture group (or whole
    /// match) is the number; defaults to the first number in stdout.
    #[serde(rename = "match", default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub poll_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Runtime (compiled) types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    /// Output port to open, if any control needs one.
    pub output_port: Option<String>,
    /// Sent once after every (re)connect, before LEDs are painted.
    pub init_messages: Vec<Vec<u8>>,
    pub controls: Vec<Control>,
}

#[derive(Debug, Clone)]
pub struct Control {
    pub name: String,
    pub binding: Binding,
    pub behavior: Behavior,
    pub feedback: Option<Feedback>,
    pub state: Option<StateSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub msg_type: MsgType,
    pub number: u8,
    /// `None` matches any channel.
    pub channel: Option<u8>,
}

impl Binding {
    fn overlaps(&self, other: &Binding) -> bool {
        self.msg_type == other.msg_type
            && self.number == other.number
            && match (self.channel, other.channel) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }
}

#[derive(Debug, Clone)]
pub enum Behavior {
    Encoder(EncoderSpec),
    Switch(SwitchSpec),
    Trigger(TriggerSpec),
}

#[derive(Debug, Clone)]
pub struct Exec {
    pub cmd: String,
    pub args: Vec<String>,
    pub shell: bool,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct EncoderSpec {
    pub mode: EncoderMode,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub accel: Option<AccelConfig>,
    pub wrap: bool,
    pub rate_limit: Duration,
    pub deadband: u8,
    pub initial: f64,
    pub precision: u8,
    pub exec: Exec,
}

#[derive(Debug, Clone)]
pub struct SwitchSpec {
    pub on: Exec,
    pub off: Exec,
    pub initial: Option<bool>,
    pub debounce: Duration,
}

#[derive(Debug, Clone)]
pub struct TriggerSpec {
    pub exec: Exec,
    pub debounce: Duration,
}

#[derive(Debug, Clone)]
pub struct Feedback {
    pub target: FbTarget,
    pub flash: Option<Duration>,
    pub while_running: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FbTarget {
    /// Fixed messages for the lit, unlit and error states.
    Messages { on: Vec<u8>, off: Vec<u8>, error: Option<Vec<u8>> },
    /// Encoder value echoed back as a CC (0..=127).
    CcEcho { channel: u8, number: u8 },
}

#[derive(Debug, Clone)]
pub struct StateSource {
    pub exec: Exec,
    pub pattern: Option<Regex>,
    pub poll: Option<Duration>,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// `$XDG_CONFIG_HOME/midiboard/config.json`, or `~/.config/midiboard/config.json`.
/// The same location is used on Linux and macOS.
pub fn default_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home::home_dir().context("could not determine the home directory")?.join(".config"),
    };
    Ok(base.join("midiboard").join("config.json"))
}

fn legacy_path() -> Option<PathBuf> {
    home::home_dir().map(|h| h.join("midiboard.json"))
}

pub fn resolve_path(path: Option<&Path>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(p.to_path_buf()),
        None => default_path(),
    }
}

/// Reads and parses a config file, without semantic validation.
pub fn load(path: &Path) -> Result<ConfigFile> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut msg = format!("config file not found at {}", path.display());
            if let Some(legacy) = legacy_path().filter(|p| p.exists()) {
                msg.push_str(&format!(
                    "\nFound a legacy config at {}. Configs from midiboard 0.4 and older need to be \
                     rewritten for 0.5; see {MIGRATION_URL}",
                    legacy.display()
                ));
            } else {
                msg.push_str("\nCreate one with `midiboard config --generate`.");
            }
            bail!(msg);
        }
        Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
    };
    parse(&text).with_context(|| format!("invalid config file {}", path.display()))
}

pub fn parse(text: &str) -> Result<ConfigFile> {
    // Look at the version first, so old configs get a migration hint instead
    // of a confusing "unknown field" error.
    let value: serde_json::Value = serde_json::from_str(text)?;
    match value.get("config_version").and_then(|v| v.as_u64()) {
        Some(CONFIG_VERSION) => {}
        Some(other) => {
            bail!("config_version {other} is not supported; this midiboard reads config_version {CONFIG_VERSION}")
        }
        None => bail!(
            "this config has no `config_version`, so it was written for midiboard 0.4 or older. \
             The format changed in 0.5; see {MIGRATION_URL}"
        ),
    }
    Ok(serde_json::from_str(text)?)
}

// ---------------------------------------------------------------------------
// Validation and compilation
// ---------------------------------------------------------------------------

struct Errors {
    list: Vec<String>,
}

impl Errors {
    fn push(&mut self, at: &str, msg: impl AsRef<str>) {
        self.list.push(format!("{at}: {}", msg.as_ref()));
    }
}

fn valid_control_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('a'..='z')) && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn compile_exec(e: &ExecConfig, at: &str, errs: &mut Errors) -> Exec {
    if e.cmd.trim().is_empty() {
        errs.push(at, "`cmd` must not be empty");
    }
    if e.shell && !e.args.is_empty() {
        errs.push(at, "with `shell: true`, put the whole command line in `cmd` and leave `args` empty");
    }
    Exec { cmd: e.cmd.clone(), args: e.args.clone(), shell: e.shell, timeout: e.timeout_ms.map(Duration::from_millis) }
}

fn note_msg(on: bool, t: &MidiTarget, velocity: u8) -> Vec<u8> {
    let ch = t.channel - 1;
    if on || velocity > 0 { vec![0x90 | ch, t.number, velocity] } else { vec![0x80 | ch, t.number, 0] }
}

fn raw_value(v: &Option<FbValue>, default: u8, field: &str, at: &str, errs: &mut Errors) -> u8 {
    match v {
        None => default,
        Some(FbValue::Raw(n)) if *n < 128 => *n,
        Some(FbValue::Raw(n)) => {
            errs.push(at, format!("`{field}` must be 0..=127, got {n}"));
            default
        }
        Some(FbValue::Name(name)) => {
            errs.push(at, format!("`{field}` must be a number 0..=127 for note/cc feedback, got \"{name}\""));
            default
        }
    }
}

fn compile_feedback(
    fb: &FeedbackConfig,
    behavior: &Behavior,
    profile: Option<Profile>,
    at: &str,
    errs: &mut Errors,
) -> Option<Feedback> {
    let at = &format!("{at}.feedback");
    let targets =
        [fb.led.is_some(), fb.note.is_some(), fb.cc.is_some(), fb.sysex.is_some()].iter().filter(|b| **b).count();
    if targets != 1 {
        errs.push(at, "set exactly one of `led`, `note`, `cc`, or `sysex`");
        return None;
    }
    let is_encoder = matches!(behavior, Behavior::Encoder(_));
    let is_trigger = matches!(behavior, Behavior::Trigger(_));
    if (fb.flash_ms.is_some() || fb.while_running) && !is_trigger {
        errs.push(at, "`flash_ms` and `while_running` only apply to Trigger controls");
    }
    if fb.flash_ms.is_some() && fb.while_running {
        errs.push(at, "use either `flash_ms` or `while_running`, not both");
    }
    if let Some(t) = fb.note.as_ref().or(fb.cc.as_ref()) {
        if t.number > 127 {
            errs.push(at, "`number` must be 0..=127");
        }
        if !(1..=16).contains(&t.channel) {
            errs.push(at, "`channel` must be 1..=16");
        }
    }

    let target = if is_encoder {
        match &fb.cc {
            Some(t) => {
                if fb.on.is_some() || fb.off.is_some() || fb.error.is_some() {
                    errs.push(at, "encoder feedback echoes the value; `on`/`off`/`error` don't apply");
                }
                FbTarget::CcEcho { channel: t.channel.clamp(1, 16), number: t.number & 0x7F }
            }
            None => {
                errs.push(at, "Encoder controls only support `cc` feedback (value echo)");
                return None;
            }
        }
    } else if let Some(led) = &fb.led {
        let Some(Profile::Beatstep) = profile else {
            errs.push(at, "`led` feedback needs a device profile, e.g. `\"output\": { \"profile\": \"beatstep\" }`");
            return None;
        };
        let id = match beatstep::led_id(led) {
            Ok(id) => id,
            Err(e) => {
                errs.push(at, e);
                return None;
            }
        };
        let mut color = |v: &Option<FbValue>, default: &str, field: &str| -> Option<Vec<u8>> {
            let name = match v {
                None => default.to_string(),
                Some(FbValue::Name(n)) => n.clone(),
                Some(FbValue::Raw(n)) => {
                    errs.push(at, format!("`{field}` must be a color name for `led` feedback, got {n}"));
                    return None;
                }
            };
            match beatstep::color(&name) {
                Ok(c) => Some(beatstep::led_message(id, c)),
                Err(e) => {
                    errs.push(at, format!("`{field}`: {e}"));
                    None
                }
            }
        };
        let on = color(&fb.on, "red", "on")?;
        let off = color(&fb.off, "off", "off")?;
        let error = match &fb.error {
            Some(_) => Some(color(&fb.error, "", "error")?),
            None => None,
        };
        FbTarget::Messages { on, off, error }
    } else if let Some(t) = &fb.note {
        let on_v = raw_value(&fb.on, 127, "on", at, errs);
        let off_v = raw_value(&fb.off, 0, "off", at, errs);
        let error = fb.error.as_ref().map(|_| note_msg(true, t, raw_value(&fb.error, 127, "error", at, errs)));
        FbTarget::Messages { on: note_msg(true, t, on_v), off: note_msg(false, t, off_v), error }
    } else if let Some(t) = &fb.cc {
        let ch = (t.channel.clamp(1, 16)) - 1;
        let msg = |v: u8| vec![0xB0 | ch, t.number & 0x7F, v];
        let on = msg(raw_value(&fb.on, 127, "on", at, errs));
        let off = msg(raw_value(&fb.off, 0, "off", at, errs));
        let error = fb.error.as_ref().map(|_| msg(raw_value(&fb.error, 127, "error", at, errs)));
        FbTarget::Messages { on, off, error }
    } else {
        let sx = fb.sysex.as_ref().expect("exactly one target is set");
        if fb.on.is_some() || fb.off.is_some() || fb.error.is_some() {
            errs.push(at, "for `sysex` feedback, put the messages inside `sysex` (on/off/error)");
        }
        let mut parse = |s: &str, field: &str| -> Option<Vec<u8>> {
            match midi::parse_hex(s).and_then(|b| midi::validate_sysex(&b).map(|_| b)) {
                Ok(b) => Some(b),
                Err(e) => {
                    errs.push(at, format!("sysex.{field}: {e}"));
                    None
                }
            }
        };
        let on = parse(&sx.on, "on")?;
        let off = parse(&sx.off, "off")?;
        let error = match &sx.error {
            Some(s) => Some(parse(s, "error")?),
            None => None,
        };
        FbTarget::Messages { on, off, error }
    };

    Some(Feedback { target, flash: fb.flash_ms.map(Duration::from_millis), while_running: fb.while_running })
}

fn compile_control(
    name: &str,
    c: &ControlConfig,
    profile: Option<Profile>,
    at: &str,
    errs: &mut Errors,
) -> Option<Control> {
    if !valid_control_name(name) {
        errs.push(at, "control names must start with a-z and contain only a-z, 0-9 and `_`");
    }
    if c.key > 127 {
        errs.push(at, format!("`key` must be 0..=127, got {}", c.key));
    }
    if let Some(ch) = c.channel
        && !(1..=16).contains(&ch)
    {
        errs.push(at, format!("`channel` must be 1..=16, got {ch}"));
    }
    let behavior = match &c.command {
        CommandConfig::Encoder(e) => {
            let at = &format!("{at}.command");
            if c.msg_type != MsgType::Cc {
                errs.push(at, "Encoder controls must use `\"type\": \"cc\"`");
            }
            if !e.map_min.is_finite() || !e.map_max.is_finite() || e.map_min >= e.map_max {
                errs.push(at, "`map_min` must be smaller than `map_max`");
            }
            if !e.step.is_finite() || e.step <= 0.0 {
                errs.push(at, "`step` must be a positive number");
            }
            if e.rate_limit_ms > 10_000 {
                errs.push(at, "`rate_limit_ms` must be at most 10000");
            }
            if e.deadband > 32 {
                errs.push(at, "`deadband` must be at most 32");
            }
            if e.deadband > 0 && e.mode != EncoderMode::Absolute {
                errs.push(at, "`deadband` only applies to `absolute` mode");
            }
            if e.wrap && e.mode == EncoderMode::Absolute {
                errs.push(at, "`wrap` only applies to relative modes");
            }
            if e.precision > 6 {
                errs.push(at, "`precision` must be at most 6");
            }
            if let Some(a) = &e.acceleration {
                if e.mode == EncoderMode::Absolute {
                    errs.push(at, "`acceleration` only applies to relative modes");
                }
                if !a.curve.is_finite()
                    || a.curve <= 0.0
                    || !a.max_gain.is_finite()
                    || a.max_gain < 1.0
                    || a.slow_ms == 0
                {
                    errs.push(at, "`acceleration` needs curve > 0, max_gain >= 1 and slow_ms > 0");
                }
            }
            let initial = e.initial.unwrap_or(e.map_min);
            if !(e.map_min..=e.map_max).contains(&initial) {
                errs.push(at, "`initial` must lie between `map_min` and `map_max`");
            }
            let exec = compile_exec(&e.execute, &format!("{at}.execute"), errs);
            Behavior::Encoder(EncoderSpec {
                mode: e.mode,
                min: e.map_min,
                max: e.map_max,
                step: e.step,
                accel: e.acceleration,
                wrap: e.wrap,
                rate_limit: Duration::from_millis(e.rate_limit_ms),
                deadband: e.deadband,
                initial,
                precision: e.precision,
                exec,
            })
        }
        CommandConfig::Switch(s) => {
            let at = &format!("{at}.command");
            if s.initial_state.is_none() && c.state.is_none() {
                errs.push(at, "`initial_state` is required unless the control has a `state` source");
            }
            Behavior::Switch(SwitchSpec {
                on: compile_exec(&s.on, &format!("{at}.on"), errs),
                off: compile_exec(&s.off, &format!("{at}.off"), errs),
                initial: s.initial_state.map(|v| v == OnOff::On),
                debounce: Duration::from_millis(s.debounce_ms),
            })
        }
        CommandConfig::Trigger(t) => {
            let at = &format!("{at}.command");
            if c.state.is_some() {
                errs.push(at, "Trigger controls have no state; remove `state`");
            }
            Behavior::Trigger(TriggerSpec {
                exec: compile_exec(&t.execute, &format!("{at}.execute"), errs),
                debounce: Duration::from_millis(t.debounce_ms),
            })
        }
    };

    let feedback = c.feedback.as_ref().and_then(|f| compile_feedback(f, &behavior, profile, at, errs));

    let state = c.state.as_ref().map(|s| {
        let sat = &format!("{at}.state");
        let exec = compile_exec(
            &ExecConfig { cmd: s.cmd.clone(), args: s.args.clone(), shell: s.shell, timeout_ms: Some(s.timeout_ms) },
            sat,
            errs,
        );
        let pattern = s.pattern.as_ref().and_then(|p| match Regex::new(p) {
            Ok(r) => Some(r),
            Err(e) => {
                errs.push(sat, format!("invalid `match` regex: {e}"));
                None
            }
        });
        if let Some(p) = s.poll_ms
            && p < 100
        {
            errs.push(sat, "`poll_ms` must be at least 100");
        }
        StateSource { exec, pattern, poll: s.poll_ms.map(Duration::from_millis) }
    });

    Some(Control {
        name: name.to_string(),
        binding: Binding { msg_type: c.msg_type, number: c.key, channel: c.channel },
        behavior,
        feedback,
        state,
    })
}

/// Validates the whole file and compiles it. On failure, returns every
/// problem found, each prefixed with where it is in the file.
pub fn compile(file: &ConfigFile) -> Result<Vec<Device>, Vec<String>> {
    let mut errs = Errors { list: Vec::new() };
    if file.devices.is_empty() {
        errs.push("devices", "at least one device is required");
    }
    let mut devices = Vec::new();
    for (i, d) in file.devices.iter().enumerate() {
        let at = format!("devices[{i}]");
        if d.name.trim().is_empty() {
            errs.push(&at, "`name` must not be empty");
        }
        if d.controls.is_empty() {
            errs.push(&at, "at least one control is required");
        }
        let profile = d.output.as_ref().and_then(|o| o.profile);
        let mut controls: Vec<Control> = Vec::new();
        for (name, c) in &d.controls {
            let cat = format!("{at}.controls.{name}");
            if let Some(ctrl) = compile_control(name, c, profile, &cat, &mut errs) {
                if let Some(other) = controls.iter().find(|o| o.binding.overlaps(&ctrl.binding)) {
                    errs.push(&cat, format!("binds the same MIDI input as `{}`", other.name));
                }
                controls.push(ctrl);
            }
        }

        let wants_output = controls.iter().any(|c| c.feedback.is_some());
        let take_over = d.output.as_ref().is_some_and(|o| o.take_over_pads);
        let mut init_messages = Vec::new();
        if take_over {
            if profile != Some(Profile::Beatstep) {
                errs.push(&format!("{at}.output"), "`take_over_pads` needs `\"profile\": \"beatstep\"`");
            } else {
                for c in &d.controls {
                    if let Some(FeedbackConfig { led: Some(led), .. }) = &c.1.feedback
                        && let Ok(id) = beatstep::led_id(led)
                        && beatstep::is_pad(id)
                    {
                        init_messages.push(beatstep::gate_mode_message(id));
                    }
                }
            }
        }
        let output_port = match &d.output {
            Some(o) => Some(o.port.clone().unwrap_or_else(|| d.name.clone())),
            None if wants_output => Some(d.name.clone()),
            None => None,
        };
        devices.push(Device { name: d.name.clone(), output_port, init_messages, controls });
    }
    if errs.list.is_empty() { Ok(devices) } else { Err(errs.list) }
}

/// Loads, parses and compiles in one go, folding validation errors into one.
pub fn load_compiled(path: &Path) -> Result<(ConfigFile, Vec<Device>)> {
    let file = load(path)?;
    match compile(&file) {
        Ok(devices) => Ok((file, devices)),
        Err(list) => bail!("invalid config file {}:\n  - {}", path.display(), list.join("\n  - ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_str(s: &str) -> Result<Vec<Device>, Vec<String>> {
        compile(&parse(s).map_err(|e| vec![e.to_string()])?)
    }

    #[test]
    fn skeleton_is_valid() {
        let devices = compile_str(SKELETON).expect("skeleton must validate");
        assert!(!devices.is_empty());
        assert_eq!(parse(SKELETON).unwrap().schema.as_deref(), Some(SCHEMA_URL));
    }

    #[test]
    fn old_configs_get_a_migration_hint() {
        let old = r#"{"log_level":"Warn","config":[{"device":"x","thresholds":{},"controls":{}}]}"#;
        let err = parse(old).unwrap_err().to_string();
        assert!(err.contains("migration"), "{err}");
        let err = parse(r#"{"config_version": 3, "devices": []}"#).unwrap_err().to_string();
        assert!(err.contains("not supported"), "{err}");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let s = r#"{"config_version":2,"devices":[{"name":"x","controls":{"a":{"key":1,"type":"note",
            "command":{"kind":"Trigger","execute":{"cmd":"true"},"typo":1}}}}]}"#;
        assert!(parse(s).is_err());
    }

    #[test]
    fn duplicate_bindings_are_rejected() {
        let s = r#"{"config_version":2,"devices":[{"name":"x","controls":{
            "a":{"key":1,"type":"note","command":{"kind":"Trigger","execute":{"cmd":"true"}}},
            "b":{"key":1,"type":"note","channel":3,"command":{"kind":"Trigger","execute":{"cmd":"true"}}},
            "c":{"key":1,"type":"cc","command":{"kind":"Trigger","execute":{"cmd":"true"}}}}}]}"#;
        let errs = compile_str(s).unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("same MIDI input"));
    }

    #[test]
    fn semantic_errors_are_reported_together() {
        let s = r#"{"config_version":2,"devices":[{"name":"x","controls":{
            "Bad":{"key":1,"type":"note","command":{"kind":"Encoder","map_min":5,"map_max":1,"execute":{"cmd":""}}},
            "sw":{"key":2,"type":"note","command":{"kind":"Switch","on":{"cmd":"a"},"off":{"cmd":"b"}},
                  "feedback":{"led":{"pad":1}}}}}]}"#;
        let errs = compile_str(s).unwrap_err();
        let all = errs.join("\n");
        assert!(all.contains("control names"), "{all}");
        assert!(all.contains("\"type\": \"cc\""), "{all}");
        assert!(all.contains("map_min"), "{all}");
        assert!(all.contains("`cmd` must not be empty"), "{all}");
        assert!(all.contains("initial_state"), "{all}");
        assert!(all.contains("profile"), "{all}");
    }

    #[test]
    fn beatstep_led_feedback_compiles_to_sysex() {
        let s = r#"{"config_version":2,"devices":[{"name":"Arturia BeatStep",
            "output":{"profile":"beatstep","take_over_pads":true},"controls":{
            "mute":{"key":36,"type":"note","command":{"kind":"Switch","initial_state":"off",
                    "on":{"cmd":"a"},"off":{"cmd":"b"}},
                    "feedback":{"led":{"pad":3},"on":"blue","off":"off","error":"magenta"}}}}]}"#;
        let dev = &compile_str(s).unwrap()[0];
        assert_eq!(dev.output_port.as_deref(), Some("Arturia BeatStep"));
        assert_eq!(dev.init_messages, vec![beatstep::gate_mode_message(0x72)]);
        let fb = dev.controls[0].feedback.as_ref().unwrap();
        assert_eq!(
            fb.target,
            FbTarget::Messages {
                on: vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x10, 0x72, 0x10, 0xF7],
                off: vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x10, 0x72, 0x00, 0xF7],
                error: Some(vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x10, 0x72, 0x11, 0xF7]),
            }
        );
    }

    #[test]
    fn note_feedback_uses_note_off_for_zero() {
        let s = r#"{"config_version":2,"devices":[{"name":"x","controls":{
            "t":{"key":36,"type":"note","command":{"kind":"Trigger","execute":{"cmd":"a"}},
                 "feedback":{"note":{"number":36,"channel":10},"flash_ms":100}}}}]}"#;
        let dev = &compile_str(s).unwrap()[0];
        let fb = dev.controls[0].feedback.as_ref().unwrap();
        assert_eq!(fb.target, FbTarget::Messages { on: vec![0x99, 36, 127], off: vec![0x89, 36, 0], error: None });
        assert_eq!(fb.flash, Some(Duration::from_millis(100)));
    }

    #[test]
    fn switch_with_state_needs_no_initial_state() {
        let s = r#"{"config_version":2,"devices":[{"name":"x","controls":{
            "m":{"key":1,"type":"note","command":{"kind":"Switch","on":{"cmd":"a"},"off":{"cmd":"b"}},
                 "state":{"cmd":"pactl","args":["get-sink-mute","@DEFAULT_SINK@"],"match":"yes","poll_ms":2000}}}}]}"#;
        let dev = &compile_str(s).unwrap()[0];
        assert!(dev.controls[0].state.as_ref().unwrap().pattern.is_some());
    }
}

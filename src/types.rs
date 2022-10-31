use anyhow::Error;
use rust_embed::RustEmbed;
use serde::Deserialize;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(RustEmbed, Debug)]
#[folder = "schema/"]
pub struct Asset;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub config: Vec<Config>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Config {
    pub device: String,
    pub controls: ControlList,
    pub thresholds: Thresholds,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub encoder: TimeThreshold,
    pub switch: TimeThreshold,
    pub trigger: TimeThreshold,
}
// TODO:Minor Make `detection` threshold an Option<> or create a new struct for detection-less controls
#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub struct TimeThreshold {
    pub activation: u64,
    pub detection: u64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Input {
    pub key: u8,
    pub command: Command,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum InitialSwitchState {
    ON,
    OFF,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum Command {
    Encoder(Encoder),
    Switch(Switch),
    Trigger(Trigger),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Encoder {
    pub increase: CommandData,
    pub decrease: CommandData,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Switch {
    pub on: CommandData,
    pub off: CommandData,
    pub initial_state: InitialSwitchState,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Trigger {
    pub execute: CommandData,
}

// TODO:Minor Add threshold override per-control
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CommandData {
    pub cmd: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Encoder,
    Switch,
    Trigger,
}

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub initialized: bool,
    pub state: KeyState,
    pub kind: CommandKind,
    pub elapsed: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct Activation {
    pub valid: bool,
    pub kind: Option<ActivationKind>,
}

#[derive(Debug, Clone)]
pub enum ActivationKind {
    Encoder { increase: bool },
    Switch { on: bool },
    Trigger,
}

impl ActivationKind {
    pub fn get_kind(self: &Self) -> CommandKind {
        match self {
            Self::Encoder { increase: _ } => CommandKind::Encoder,
            Self::Switch { on: _ } => CommandKind::Switch,
            Self::Trigger => CommandKind::Trigger,
        }
    }
}

impl Command {
    pub fn get_kind(self: &Self) -> CommandKind {
        match self {
            Self::Encoder(_) => CommandKind::Encoder,
            Self::Switch(_) => CommandKind::Switch,
            Self::Trigger(_) => CommandKind::Trigger,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyState {
    // Target control of the detected key
    pub control: String,
    // Minimum time threshold in ms from last key detection to add here
    pub time_threshold: Duration,
    // Elapsed time since first activation to consider activation finished
    pub activation_threshold: Duration,
    // value recorded at each detection
    pub detections: Vec<u8>,
    pub start: Instant,
    pub initial_state: Option<InitialSwitchState>,
}

pub type ControlListByKey = HashMap<u8, String>; // HashMap<key code, control name>
pub type ControlList = HashMap<String, Input>;

impl Config {
    pub fn get_control(self: &Self, control: &String) -> Result<&Input, Error> {
        self.controls.get(control).ok_or(Error::msg(format!(
            "Control {} not found in the loaded config",
            control
        )))
    }

    pub fn get_controls_by_key(self: &Self) -> ControlListByKey {
        let mut list = HashMap::new();

        for control in self.controls.clone() {
            list.insert(control.1.key, control.0);
        }
        list
    }

    pub fn get_threshold(self: &Self, key: u8) -> Result<(CommandKind, TimeThreshold), Error> {
        let by_key = self.get_controls_by_key();
        let control = by_key.get(&key).ok_or(Error::msg(format!(
            "Key {} not found for any control listed in the configuration.",
            key
        )))?;
        let selection = self.get_control(control)?;
        match selection.command.get_kind() {
            CommandKind::Encoder => {
                return Ok((CommandKind::Encoder, self.thresholds.encoder));
            }
            CommandKind::Switch => {
                return Ok((CommandKind::Switch, self.thresholds.switch));
            }
            CommandKind::Trigger => {
                return Ok((CommandKind::Trigger, self.thresholds.trigger));
            }
        };
    }
}

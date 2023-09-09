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
    pub log_level: LogLevel,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Config {
    pub device: String,
    pub controls: ControlList,
    pub thresholds: Thresholds,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub encoder: FullTimeThreshold,
    pub switch: TimeThreshold,
    pub trigger: TimeThreshold,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(untagged)]
pub enum Threshold {
    Full(FullTimeThreshold),
    Base(TimeThreshold),
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub struct FullTimeThreshold {
    pub activation: u64,
    pub detection: u64,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub struct TimeThreshold {
    pub activation: u64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Input {
    pub key: u8,
    pub command: Command,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct InputOverride {
    pub key: u8,
    pub threshold: Threshold,
    pub command: Command,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, PartialOrd)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum InitialSwitchState {
    ON,
    OFF,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum InputOption {
    Overrode(InputOverride),
    Normal(Input),
}

impl InputOption {
    pub fn key(self: &Self) -> u8 {
        match self {
            Self::Overrode(data) => data.key,
            Self::Normal(data) => data.key,
        }
    }

    pub fn command(self: &Self) -> Command {
        match self {
            Self::Overrode(data) => data.command.clone(),
            Self::Normal(data) => data.command.clone(),
        }
    }

    pub fn threshold(self: &Self) -> Option<Threshold> {
        match self {
            Self::Overrode(data) => Some(data.threshold),
            Self::Normal(_) => None,
        }
    }
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
    pub execute: CommandData,
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

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CommandData {
    pub cmd: String,
    pub args: Vec<String>,
    pub replace: Option<String>,
    pub map_max: Option<i32>,
    pub map_min: Option<i32>,
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

impl Activation {
    pub fn encoder(valid: bool, increase: bool) -> Self {
        Activation {
            valid: valid,
            kind: Some(ActivationKind::Encoder),
        }
    }
    pub fn switch(valid: bool, on: bool) -> Self {
        Activation {
            valid: valid,
            kind: Some(ActivationKind::switch(on)),
        }
    }
    pub fn trigger(valid: bool) -> Self {
        Activation {
            valid: valid,
            kind: Some(ActivationKind::Trigger),
        }
    }
    pub fn failed() -> Self {
        Activation {
            valid: false,
            kind: None,
        }
    }
    pub fn as_ok(self: Self) -> Result<Self, Error> {
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub enum ActivationKind {
    Encoder,
    Switch { on: bool },
    Trigger,
}

impl ActivationKind {
    pub fn get_kind(self: &Self) -> CommandKind {
        match self {
            Self::Encoder => CommandKind::Encoder,
            Self::Switch { on: _ } => CommandKind::Switch,
            Self::Trigger => CommandKind::Trigger,
        }
    }

    pub fn switch(on: bool) -> Self {
        ActivationKind::Switch { on: on }
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
    pub detection_threshold: Option<Duration>,
    // Elapsed time since first activation to consider activation finished
    pub activation_threshold: Duration,
    // value recorded at each detection
    pub detections: Vec<u8>,
    pub start: Instant,
    pub initial_state: Option<InitialSwitchState>,
}

pub type ControlListByKey = HashMap<u8, String>; // HashMap<key code, control name>

pub type ControlList = HashMap<String, InputOption>;

impl Config {
    pub fn get_control(self: &Self, control: &String) -> Result<&InputOption, Error> {
        self.controls.get(control).ok_or(Error::msg(format!(
            "Control {} not found in the loaded config",
            control
        )))
    }

    pub fn get_controls_by_key(self: &Self) -> ControlListByKey {
        let mut list = HashMap::new();

        for control in self.controls.clone() {
            list.insert(control.1.key(), control.0);
        }
        list
    }

    pub fn get_threshold(self: &Self, key: u8) -> Result<(CommandKind, Threshold), Error> {
        let by_key = self.get_controls_by_key();
        let control = by_key.get(&key).ok_or(Error::msg(format!(
            "Key {} not found for any control listed in the configuration.",
            key
        )))?;
        let selection = self.get_control(control)?;
        match selection.command().get_kind() {
            CommandKind::Encoder => {
                return Ok((
                    CommandKind::Encoder,
                    Threshold::Full(self.thresholds.encoder),
                ));
            }
            CommandKind::Switch => {
                return Ok((CommandKind::Switch, Threshold::Base(self.thresholds.switch)));
            }
            CommandKind::Trigger => {
                return Ok((
                    CommandKind::Trigger,
                    Threshold::Base(self.thresholds.trigger),
                ));
            }
        };
    }
}

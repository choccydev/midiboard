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

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub device: String,
    pub controls: HashMap<String, Input>,
    pub thresholds: Thresholds,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Thresholds {
    pub encoder: TimeThreshold,
    pub switch: TimeThreshold,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TimeThreshold {
    pub activation: u64,
    pub detection: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Input {
    pub key: u8,
    pub command: Command,
}

#[derive(Debug, Deserialize, Clone)]
pub enum InitialSwitchState {
    ON,
    OFF,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "kind")]
pub enum Command {
    Encoder,
    Switch,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Encoder {
    pub increase: String,
    pub decrease: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Switch {
    pub on: String,
    pub off: String,
    pub initial_state: InitialSwitchState,
}

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub initialized: bool,
    pub state: KeyState,
    pub kind: Command,
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
}

pub type ControlList = HashMap<u8, String>; // HashMap<key code, control name>

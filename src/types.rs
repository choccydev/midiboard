use rust_embed::RustEmbed;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(RustEmbed, Debug)]
#[folder = "schema/"]
pub struct Asset;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub config: Vec<Config>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub device: String,
    pub controls: HashMap<String, Input>,
    pub thresholds: Thresholds,
}

#[derive(Debug, Deserialize)]
pub struct Thresholds {
    encoder: TimeThreshold,
    switch: TimeThreshold,
}

#[derive(Debug, Deserialize)]
pub struct TimeThreshold {
    activation: u32,
    detection: u32,
}

#[derive(Debug, Deserialize)]
pub struct Input {
    pub key: u8,
    pub command: Command,
}

#[derive(Debug, Deserialize)]
pub enum InitialSwitchState {
    ON,
    OFF,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum Command {
    Encoder {
        increase: String,
        decrease: String,
    },
    Switch {
        on: String,
        off: String,
        initial_state: InitialSwitchState,
    },
}

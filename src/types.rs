use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub schema: String,
    pub config: Vec<Config>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub device: String,
    pub controls: HashMap<String, Input>,
}

#[derive(Debug, Deserialize)]
pub struct Input {
    pub key: u8,
    pub kind: InputType,
    pub command: Command,
}

#[derive(Debug, Deserialize)]
pub struct EncoderCommand {
    pub increase: String,
    pub decrease: String,
}

#[derive(Debug, Deserialize)]
pub struct SwitchCommand {
    pub on: String,
    pub off: String,
    pub initial_state: InitialSwitchState,
}

#[derive(Debug, Deserialize)]
pub enum InitialSwitchState {
    ON,
    OFF,
}

#[derive(Debug, Deserialize)]
pub enum Command {
    EncoderCommand,
    SwitchCommand,
}

#[derive(Debug, Deserialize)]
pub enum InputType {
    Encoder,
    Switch,
}

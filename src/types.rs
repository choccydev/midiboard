use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub device: String,
    pub controls: Controls,
}

#[derive(Debug, Deserialize)]
pub struct Controls {
    pub light: Light,
    pub audio: Audio,
    pub monitor: Monitor,
    pub system: System,
}

#[derive(Debug, Deserialize)]
pub struct Light {
    pub intensity: Input,
    pub hue: Input,
    pub mode: Input,
}

#[derive(Debug, Deserialize)]
pub struct Audio {
    pub volume: Input,
    pub song: Input,
    pub pause: Input,
    pub mute: Input,
}

#[derive(Debug, Deserialize)]
pub struct Monitor {
    pub brightness: Input,
    pub mode: Input,
}

#[derive(Debug, Deserialize)]
pub struct System {
    pub suspend: Input,
    pub lock: Input,
}

#[derive(Debug, Deserialize)]
pub struct Input {
    pub key: u8,
    pub kind: InputType,
}

#[derive(Debug, Deserialize)]
pub enum InputType {
    Encoder,
    Switch,
}

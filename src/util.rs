use super::types::{self, LogLevel};
use anyhow::Error;
use chrono;
use colored::*;
use config::{Config, ConfigError};
use core::fmt::Debug;
use home::home_dir;
use midir::{Ignore, MidiInput, MidiInputPort};
use std::path::PathBuf;

pub fn read_user_config(path: Option<&String>) -> Result<types::ConfigFile, ConfigError> {
    let mut fullpath = PathBuf::new();

    match path {
        None => {
            fullpath.push(
                home_dir().ok_or(ConfigError::Message(String::from("Could not parse path")))?,
            );
            fullpath.push("midiboard");
            fullpath.set_extension("json");
        }
        Some(path) => fullpath.push(path),
    }

    // load and return the config
    let config = Config::builder()
        .add_source(config::File::new(
            fullpath
                .as_os_str()
                .to_str()
                .ok_or(ConfigError::Message(String::from("Could not parse path")))?,
            config::FileFormat::Json,
        ))
        .build();
    let parsed_config = config?.try_deserialize::<types::ConfigFile>();
    return parsed_config;
}

// From https://stackoverflow.com/a/52367953/16134348
pub fn string_to_sstr(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

pub fn get_input_port(device: &str, log: Logger) -> Result<MidiInputPort, Error> {
    let mut input = MidiInput::new("Midiboard: Port Reader")?;
    input.ignore(Ignore::None);
    match input.ports().len() {
        0 => return Err(Error::msg("No ports detected. Is ALSA Seq running?")),
        _ => {
            log.trace("Ports detected", "");
            let mut selected_port: Option<usize> = None;
            let mut port_name_list = Vec::new();

            for (index, port) in input.ports().iter().enumerate() {
                log.trace(format!("Testing port {}", &index).as_str(), "");
                let raw_name = input.port_name(port)?;
                port_name_list.push(raw_name);
            }

            for (index, _) in input.ports().iter().enumerate() {
                let port_name: &str = port_name_list[index].split(':').collect::<Vec<&str>>()[0];

                let cleaned_name = port_name.to_lowercase().replace(" ", "");
                let cleaned_device_name = &device.to_lowercase().replace(" ", "");

                if cleaned_name.eq(cleaned_device_name) {
                    log.trace(
                        format!("Port {} matches device {}", &index, &device).as_str(),
                        "",
                    );
                    selected_port = Some(index);
                }
            }
            match selected_port {
                Some(correct_port) => match input.ports().clone().get(correct_port) {
                    Some(port_connector) => Ok(port_connector.clone()),
                    None => Err(Error::msg("No valid port found. Probably the device was disconnected or the ports changed mid-connection.")),
                },
                None => {
                    log.warn("Failed to connect to selected device. Selected device:");
                    log.default(device);
                    log.warn("Available devices:");
                    for name in port_name_list {
                        log.default(&name.split(':').collect::<Vec<&str>>()[0]);
                    }
                    Err(Error::msg("No valid port found. Probably the device wasn't found or the ports changed mid-connection."))
                },
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub struct Logger {
    current_level: LogLevel,
}

impl Logger {
    pub fn new(level: LogLevel) -> Self {
        Logger {
            current_level: level,
        }
    }

    pub fn change_level(mut self: Self, level: LogLevel) {
        self.current_level = level
    }

    fn get_time(self: Self) -> String {
        chrono::offset::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    pub fn dynamic(self: Self, message: &str, selector: &str, child: Option<&str>) {
        match selector {
            "info" => self.info(message),
            "debug" => self.debug(message),
            "error" => self.error(message),
            "success" => self.success(message),
            "fatal" => self.fatal(message),
            "warn" => self.warn(message),
            "message" => self.message(message, if let Some(name) = child { name } else { "" }),
            &_ => self.default(message),
        }
    }

    pub fn info(self: Self, message: &str) {
        if self.current_level >= LogLevel::Info {
            println!(
                "{} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[INFO]".bright_blue().bold(),
                message.bright_blue().italic()
            );
        }
    }

    pub fn debug(self: Self, message: &str) {
        if self.current_level >= LogLevel::Debug {
            println!(
                "{} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[DEBUG]".yellow().bold(),
                message.yellow().italic()
            );
        }
    }

    pub fn trace<T: Debug>(self: Self, message: &str, dump: T) {
        if self.current_level >= LogLevel::Trace {
            let dump_formatted = format!("{:?}", dump);

            println!(
                "{} {} {}{}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[TRACE]".bold(),
                message.italic(),
                if dump_formatted != "\"\"" {
                    format!("\n{}{}", "DUMP:".bold().magenta(), dump_formatted.as_str())
                } else {
                    String::new()
                }
                .as_str()
            );
        }
    }

    pub fn message(self: Self, message: &str, child: &str) {
        if self.current_level >= LogLevel::Error {
            let child_name = child.to_uppercase().magenta();
            println!(
                "{} {} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[MESSAGE]".yellow().bold(),
                format!("{}{}{}", "[FROM ".magenta(), child_name, "]".magenta()).as_str(),
                message.italic()
            );
        }
    }

    pub fn fatal(self: Self, message: &str) {
        if self.current_level >= LogLevel::Error {
            eprintln!(
                "{} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[FATAL]".bright_purple().bold(),
                message.bright_red().bold()
            );
            // FIXME:Patch Make it so the assert doesn't return a panic data.
            assert!(false);
        }
    }

    pub fn error(self: Self, message: &str) {
        if self.current_level >= LogLevel::Error {
            eprintln!(
                "{} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[ERROR]".bright_red().bold(),
                message.bright_red().italic()
            );
        }
    }

    pub fn warn(self: Self, message: &str) {
        if self.current_level >= LogLevel::Warn {
            eprintln!(
                "{} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[WARN]".yellow().bold(),
                message.yellow().italic()
            );
        }
    }

    pub fn success(self: Self, message: &str) {
        if self.current_level >= LogLevel::Error {
            println!(
                "{} {} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                "[SUCCESS]".bright_green().bold(),
                message.bright_green().italic()
            );
        }
    }

    pub fn default(self: Self, message: &str) {
        if self.current_level >= LogLevel::Error {
            println!(
                "{} {}",
                format!("[{}]", self.get_time()).as_str().magenta(),
                message.normal().italic()
            );
        }
    }
}

impl Default for Logger {
    fn default() -> Self {
        Logger {
            current_level: LogLevel::Debug,
        }
    }
}

#[test]
fn stdout_common() {
    let message = "test string";

    let log = Logger::new(LogLevel::Debug);

    log.debug(message);
    log.info(message);
    log.warn(message);
    log.error(message);
    log.success(message);
    log.message(message, "test");
}

#[test]
#[should_panic]
fn stdout_fatal() {
    let message = "test string";

    let log = Logger::new(LogLevel::Debug);

    log.fatal(message);
}

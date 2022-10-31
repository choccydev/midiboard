use super::types::{self, LogLevel};
use chrono;
use colored::*;
use config::{Config, ConfigError};
use home::home_dir;
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

    pub fn set_level(mut self: Self, level: LogLevel) {
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

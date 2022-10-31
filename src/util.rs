use super::types;
use chrono;
use colored::*;
use config::{Config, ConfigError};
use home::home_dir;
use std::path::PathBuf;
use std::process;

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

pub fn stdout(selector: &str, message: &str) {
    let time = chrono::offset::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    // TODO:Minor implement debug level in the config
    // TODO:Patch implement StdIO error handling
    // TODO:Major implement "[FROM: {}]" to refer to spawned children messages. D this by adding an Option<> as last requirement with the cmd value
    match selector {
        "info" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[INFO]".bright_blue().bold(),
                message.bright_blue().italic()
            );
        }
        "debug" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[DEBUG]".yellow().bold(),
                message.yellow().italic()
            );
        }
        "message" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[MESSAGE]".yellow().bold(),
                message.italic()
            );
        }
        "fatal" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[FATAL]".bright_purple().bold(),
                message.bright_red().bold()
            );
            process::exit(1);
        }
        "error" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[ERROR]".bright_red().bold(),
                message.bright_red().italic()
            );
        }
        "warning" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[WARN]".yellow().bold(),
                message.yellow().italic()
            );
        }
        "success" => {
            println!(
                "{} {} {}",
                format!("[{}]", time).as_str().magenta(),
                "[SUCCESS]".bright_green().bold(),
                message.bright_green().italic()
            );
        }
        _ => {
            println!(
                "{} {}",
                format!("[{}]", time).as_str().magenta(),
                message.normal().italic()
            );
        }
    }
}

#[test]
fn stdout_common() {
    let message = "test string";
    let selectors = vec![
        "info", "debug", "message", "success", "warning", "error", "",
    ];

    for selector in selectors {
        let out = stdout(selector, message);

        assert_eq!(out, ());
    }
}

#[test]
#[should_panic]
fn stdout_fatal() {
    let message = "test string";
    let selector = "fatal";
    stdout(selector, message);
}

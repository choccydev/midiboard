use super::types;
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
    // TODO implement IO error handling
    match selector {
        "info" => {
            println!(
                "{}{} {}",
                "[control-board]".bright_blue().bold(),
                "[info]".bright_blue(),
                message.bright_blue().italic()
            );
        }
        "debug" => {
            println!(
                "{}{} {}",
                "[control-board]".bright_purple().bold(),
                "[debug]".yellow(),
                message.yellow().italic()
            );
        }
        "fatal" => {
            println!(
                "{} {} {}",
                "[control-board]".bright_red().bold(),
                "[fatal]".bright_purple().bold(),
                message.bright_red().bold()
            );
            process::exit(1);
        }
        "error" => {
            println!(
                "{}{} {}",
                "[control-board]".bright_red().bold(),
                "[error]".bright_red(),
                message.bright_red().italic()
            );
        }
        "warning" => {
            println!(
                "{}{} {}",
                "[control-board]".yellow().bold(),
                "[warn]".yellow(),
                message.yellow().italic()
            );
        }
        "success" => {
            println!(
                "{}{} {}",
                "[control-board]".bright_green().bold(),
                "[success]".bright_green(),
                message.bright_green().italic()
            );
        }
        _ => {
            println!(
                "{} {}",
                "[control-board]".normal().bold(),
                message.normal().italic()
            );
        }
    }
}

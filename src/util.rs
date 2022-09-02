use super::types;
use colored::*;
use config::{Config, ConfigError};
use home::home_dir;
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::error::Error;
use std::path::PathBuf;
use std::{env, fs, io, process};

pub fn read_user_config() -> Result<Config, ConfigError> {
    let mut path = PathBuf::new();

    // TODO Uncomment this line after all main features are added and this won't change further
    //path.push(home_dir().ok_or(ConfigError::Message(String::from("Could not parse path")))?);
    path.push("control-board");
    path.set_extension("json5");

    // load and return the config
    let config = Config::builder()
        .add_source(config::File::new(
            path.as_os_str()
                .to_str()
                .ok_or(ConfigError::Message(String::from("Could not parse path")))?,
            config::FileFormat::Json5,
        ))
        .build();
    return config;
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

pub fn load_midi() -> Result<MidiInputConnection<()>, Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    let config = read_user_config()?.try_deserialize::<types::Config>()?;

    // Get an input port (read from config file)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err("No input port found".into()),
        _ => {
            let mut selected_port = 0;

            for (i, p) in in_ports.iter().enumerate() {
                if midi_in
                    .port_name(p)
                    .unwrap()
                    .as_str()
                    .to_lowercase()
                    .contains(&config.device.to_lowercase())
                {
                    selected_port = i;
                }
            }
            in_ports.get(selected_port).ok_or("Device not found")?
        }
    };

    println!("[Info] Opening connection");
    let in_port_name = midi_in.port_name(in_port)?;

    // _conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let conn_in = midi_in.connect(
        in_port,
        "midir-read-input",
        move |_stamp, message, _| {
            println!("[Debug] key: {}, value: {}", message[1], message[2]);
        },
        (),
    )?;

    return Ok(conn_in);

    // println!(
    //     "[Info] Connection open, reading input from '{}'",
    //     in_port_name
    // );

    // println!("\n[Debug] Config: \n {:?}\n", config);
    // loop {}
}

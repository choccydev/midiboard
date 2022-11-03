use super::types::LogLevel;
use super::util;
use anyhow::Error;
use midir::{Ignore, MidiInput};
use std::io::stdin;

pub fn run(cli: &clap::ArgMatches) -> Result<(), Error> {
    let list = cli
        .get_one::<bool>("list")
        .ok_or(Error::msg("Bad --list command."))?;
    let listen = cli.get_one::<String>("listen");

    if *list {
        return list_devices();
    }

    if listen.is_some() {
        let device = cli
            .get_one::<String>("listen")
            .ok_or(Error::msg("No device name provided"))?
            .to_string();
        return listen_to_device(device);
    }

    panic!("No valid argument provided to the config subcommand.")
}

fn list_devices() -> Result<(), Error> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    let log = util::Logger::new(LogLevel::Debug);

    log.info("Currently connected devices:\n");

    let in_ports = midi_in.ports();
    match in_ports.len() {
        0 => return Err(Error::msg("No devices found.")),
        _ => {
            for (_i, p) in in_ports.iter().enumerate() {
                log.default(
                    &midi_in
                        .port_name(p)?
                        .split_terminator(':')
                        .collect::<Vec<&str>>()[0],
                )
            }
        }
    };
    return Ok(());
}

fn listen_to_device(device: String) -> Result<(), Error> {
    let log = util::Logger::new(LogLevel::Debug);
    let mut user_input = String::new();
    let mut midi_input = MidiInput::new("Midiboard: Device Listener")?;

    midi_input.ignore(Ignore::None);

    let in_port = util::get_input_port(&device, log)?;

    log.info("Opening connection...");

    let _conn = match midi_input.connect(
        &in_port,
        "midir-read-input",
        move |_stamp, message, _| {
            let closure_log = util::Logger::new(LogLevel::Debug);
            closure_log.default(util::string_to_sstr(format!(
                "key: {}, value: {}",
                message[1], message[2]
            )));
        },
        (),
    ) {
        Ok(connect) => connect,
        Err(error) => return Err(Error::msg(error.to_string())),
    };

    log.success(util::string_to_sstr(format!(
        "Connection open, listening events from {}",
        device
    )));

    log.info("Press any key to stop listening\n");

    user_input.clear();
    stdin().read_line(&mut user_input)?; // wait for next enter key press

    log.info("Connection closed.");
    Ok(())
}

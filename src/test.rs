use super::util;
use midir::{Ignore, MidiInput};
use std::error::Error;
use std::io::stdin;

pub fn run(cli: &clap::ArgMatches) -> Result<(), Box<dyn Error>> {
    let list = cli.contains_id("list");
    let listen = cli.contains_id("listen");

    if list {
        return list_devices();
    }

    if listen {
        let device = cli
            .get_one::<String>("listen")
            .ok_or("No device name provided")?
            .to_string();
        return listen_to_device(device);
    }

    panic!("No valid argument provided to the config subcommand.")
}

pub fn list_devices() -> Result<(), Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    util::stdout("info", "Currently connected devices:\n");

    let in_ports = midi_in.ports();
    match in_ports.len() {
        0 => return Err("No input port found".into()),
        _ => {
            for (_i, p) in in_ports.iter().enumerate() {
                util::stdout(
                    "",
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

pub fn listen_to_device(device: String) -> Result<(), Box<dyn Error>> {
    let mut input = String::new();
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);
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
                    .contains(&device.to_lowercase())
                {
                    selected_port = i;
                }
            }
            in_ports.get(selected_port).ok_or("Device not found")?
        }
    };

    util::stdout("info", "Opening connection...");

    let _conn = midi_in.connect(
        in_port,
        "midir-read-input",
        move |_stamp, message, _| {
            util::stdout(
                "",
                util::string_to_sstr(format!("key: {}, value: {}", message[1], message[2])),
            );
        },
        (),
    )?;

    util::stdout(
        "success",
        util::string_to_sstr(format!("Connection open, listening events from {}", device)),
    );

    util::stdout("info", "Press any key to stop listening\n");

    input.clear();
    stdin().read_line(&mut input)?; // wait for next enter key press

    util::stdout("info", "Connection closed.");
    Ok(())
}

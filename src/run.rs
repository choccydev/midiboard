use super::types;
use super::util;
use anyhow::Error;
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::thread;

pub fn run(cli: &clap::ArgMatches) -> Result<(), Error> {
    let path = cli.get_one::<String>("path");

    let config_data = util::read_user_config(path)?;

    for config in config_data.config {
        thread::spawn(|| handle_device(config.device));
    }
    Ok(())
}

pub fn open_midi(device: String) -> Result<MidiInputConnection<()>, Error> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    // Get an input port (read from config file)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err(Error::msg("No devices found.")),
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
            in_ports
                .get(selected_port)
                .ok_or(Error::msg("Device not found."))?
        }
    };

    println!("[Info] Opening connection");
    let conn_in = midi_in
        .connect(
            in_port,
            "midir-read-input",
            move |_stamp, message, _| {
                // TODO add event sending here
                //println!("[Debug] key: {}, value: {}", message[1], message[2]);
            },
            (),
        )
        .expect("Connection error."); // HACK bubble Err

    return Ok(conn_in);
}

pub fn handle_device(device: String) -> Result<(), Error> {
    match open_midi(device) {
        Ok(connection) => {
            todo!();
            // TODO implement events and shit using the midi connection
            // TODO find a way to manage errors
            // TODO collect all keys and assign each to an event
            // TODO add event listener for Switch event
            // TODO add event listener for Encoder event
            // TODO add debounce for encoders and switches
            // TODO add command execution
            // TODO add state handling to store the data to manage the Switch and Encoder events
        }
        Err(error) => return Err(error),
    }
}

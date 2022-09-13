use super::types;
use super::util;
use anyhow::Error;
use midir::{Ignore, MidiInput};
use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

pub fn run(cli: &clap::ArgMatches) -> Result<(), Error> {
    let path = cli.get_one::<String>("path");

    let config_data = util::read_user_config(path)?;

    for config in config_data.config {
        thread::spawn(move || handle_device(config.device.clone(), config));
    }
    Ok(())
}

pub fn handle_device(device: String, config: types::Config) -> Result<(), Error> {
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

    let controls = get_controls(config.clone());

    let mut states: HashMap<u8, Option<types::KeyState>> = HashMap::new();

    for control in controls.clone() {
        states.insert(control.0, None);
    }

    util::stdout("info", "Opening connection...");

    let _conn_in = midi_in
        .connect(
            in_port,
            &device,
            move |_stamp, message, _| {
                let key = message[1];
                let value = message[2];

                match states.get(&key) {
                    Some(state) => {
                        util::stdout(
                            "",
                            format!("Control {} detected.", &controls.get(&key).unwrap()).as_str(),
                        );
                        let key_combo_state =
                            on_key_event(key, state.clone(), &config, &controls, value);

                        match key_combo_state.0 {
                            true => {
                                match call_command(key.clone()) {
                                    Ok(command) => util::stdout(
                                        "info",
                                        format!("Executed command {}", command).as_str(),
                                    ),
                                    Err(error) => util::stdout("error", &error.to_string()),
                                };
                                states.remove(&key);
                                states.insert(key, None);
                            }
                            false => {}
                        }
                    }
                    None => {}
                }
            },
            (),
        )
        .expect("Connection error."); // HACK bubble Err

    util::stdout("info", "Connection closed.");

    Ok(())
}

pub fn call_command(key: u8) -> Result<String, Error> {
    todo!();
}

pub fn get_controls(config: types::Config) -> types::ControlList {
    let mut list = HashMap::new();

    for control in config.controls.clone() {
        list.insert(control.1.key, control.0);
    }
    list
}

pub fn on_key_event(
    key: u8,
    state: Option<types::KeyState>,
    config: &types::Config,
    controls: &types::ControlList,
    value: u8,
) -> (bool, Option<types::KeyState>) {
    match controls.get(&key) {
        None => return (false, None),
        Some(somekey) => match state {
            None => {
                let new_state = Some(types::KeyState {
                    control: somekey.clone(),
                    time_threshold: Duration::from_millis(
                        get_threshold_from_key(key, config, controls).detection,
                    ),
                    activation_threshold: Duration::from_millis(
                        get_threshold_from_key(key, config, controls).activation,
                    ),
                    detections: Vec::new(),
                    start: Instant::now(),
                });
                return (false, new_state);
            }
            Some(state) => {
                // TODO do stuff with the state to detect new values and check if there is a new activation

                // TODO debounce

                // TODO add branches for encoder and for switch
                todo!()
            }
        },
    }
}

pub fn get_threshold_from_key(
    key: u8,
    config: &types::Config,
    controls: &types::ControlList,
) -> types::TimeThreshold {
    todo!();
    // TODO cycle and collect config to determine if the key is encoder or switch and return the correct TimeThreshold
}

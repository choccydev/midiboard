use super::types::{
    Activation, ActivationKind, Command, CommandData, CommandKind, Config, ControlConfig,
    ControlList, KeyEvent, KeyState, TimeThreshold,
};
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
        let builder = thread::Builder::new();
        let handle = builder.spawn(move || handle_device(config.device.clone(), config))?;
        match handle.join() {
            Ok(_) => {}
            Err(error) => {
                println!("\n{:?}\n", error);
                util::stdout(
                    "fatal",
                    "There has been a fatal error in a connection thread.",
                )
            }
        };
    }
    Ok(())
}

pub fn handle_device(device: String, config: Config) -> Result<(), Error> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    // Get an input port (read from config file)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err(Error::msg("No devices found.")),
        _ => {
            let mut selected_port = 0;

            for (index, port) in in_ports.iter().enumerate() {
                if midi_in
                    .port_name(port)
                    .unwrap()
                    .as_str()
                    .to_lowercase()
                    .contains(&device.to_lowercase())
                {
                    selected_port = index;
                }
            }
            in_ports
                .get(selected_port)
                .ok_or(Error::msg("Device not found."))?
        }
    };

    let controls = get_controls(config.clone());

    let mut states: HashMap<u8, Option<KeyState>> = HashMap::new();

    for control in controls.clone() {
        states.insert(control.0, None);
    }

    util::stdout("info", "Opening connection...");

    let conn = midi_in.connect(
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
                    match on_key_event(key, state.clone(), &config, &controls, value) {
                        Ok(mut key_event) => match key_event.initialized {
                            true => {
                                let activation = debounce(&mut key_event);
                                if activation.valid {
                                    match call_command(&key_event, &activation, &config.controls) {
                                        Ok(command) => util::stdout(
                                            "info",
                                            format!("Executed command {}", command).as_str(),
                                        ),
                                        Err(error) => util::stdout("error", &error.to_string()),
                                    };
                                }
                                states.remove(&key);
                                states.insert(key, None);
                            }
                            false => {
                                states.remove(&key);
                                states.insert(key, Some(key_event.state));
                            }
                        },
                        Err(error) => {
                            util::stdout("warn", &error.to_string());
                        }
                    };
                }
                None => {}
            }
        },
        (),
    );

    match conn {
        Ok(_) => loop {},
        Err(error) => {
            let error_kind = error.kind();
            util::stdout("info", "Connection closed.");
            return Err(Error::msg(error_kind));
        }
    }
}

pub fn call_command(key: u8) -> Result<String, Error> {
    //todo!();
    return Ok(String::from("called!"));
}

pub fn get_controls(config: Config) -> ControlList {
    let mut list = HashMap::new();

    for control in config.controls.clone() {
        list.insert(control.1.key, control.0);
    }
    list
}

pub fn on_key_event(
    key: u8,
    state: Option<KeyState>,
    config: &Config,
    controls: &ControlList,
    value: u8,
) -> Result<KeyEvent, Error> {
    match controls.get(&key) {
        None => {
            return Err(Error::msg(format!(
                "key {} not found in control list.",
                key
            )));
        }
        Some(somekey) => {
            let threshold_data = get_threshold(key, config, controls)?;
            let threshold = threshold_data.1;
            match state {
                None => {
                    let mut new_state = KeyState {
                        control: somekey.clone(),
                        time_threshold: Duration::from_millis(threshold.detection),
                        activation_threshold: Duration::from_millis(threshold.activation),
                        detections: Vec::new(),
                        start: Instant::now(),
                    };
                    new_state.detections.push(value);

                    return Ok(KeyEvent {
                        initialized: false,
                        state: new_state,
                        kind: threshold_data.0,
                        elapsed: None,
                    });
                }
                Some(state) => {
                    let mut new_state = state;
                    let start = new_state.start;
                    new_state.detections.push(value);

                    return Ok(KeyEvent {
                        initialized: true,
                        state: new_state,
                        kind: threshold_data.0,
                        elapsed: Some(Instant::now().duration_since(start)),
                    });
                }
            }
        }
    }
}

pub fn debounce(event: &mut KeyEvent) -> Activation {
    let activation_threshold = event.state.activation_threshold;
    let time_threshold = event.state.time_threshold;
    let elapsed = event.elapsed.unwrap();

    match event.kind {
        CommandKind::Encoder => {
            return if elapsed.gt(&activation_threshold) {
                // get last two detections to be able to compare

                let previous_val: i16 = event
                    .state
                    .detections
                    .remove(event.state.detections.len() - 2)
                    .into();
                let last_val: i16 = event
                    .state
                    .detections
                    .remove(event.state.detections.len() - 1)
                    .into();

                let is_increase = last_val.gt(&previous_val);

                // then reset the detection vec to account for a new detection next time
                event.state.detections = Vec::new();

                Activation {
                    valid: true,
                    kind: Some(ActivationKind::Encoder {
                        increase: is_increase,
                    }),
                }
            } else {
                if elapsed.lt(&time_threshold) {
                    Activation {
                        valid: false,
                        kind: None,
                    }
                } else {
                    let mut accumulator: i16 = 0;

                    for (index, detection) in event.state.detections.iter().enumerate() {
                        if index % 2 == 0 {
                            accumulator += Into::<i16>::into(*detection);
                        } else {
                            accumulator -= Into::<i16>::into(*detection);
                        }
                    }

                    let is_increase = accumulator.lt(&0);

                    Activation {
                        valid: true,
                        kind: Some(ActivationKind::Encoder {
                            increase: is_increase,
                        }),
                    }
                }
            };
        }
        CommandKind::Switch => {
            todo!()
        }
    }
}

pub fn get_threshold(
    key: u8,
    config: &Config,
    controls: &ControlList,
) -> Result<(CommandKind, TimeThreshold), Error> {
    let commands = &config.controls;
    let control = controls.get(&key).ok_or(Error::msg(format!(
        "Key {} not found for any control listed in the configuration.",
        key
    )))?;
    let selection = commands.get(control).ok_or(Error::msg(format!(
        "Configuration missing for control {}. (how? are you messing with the memory?)",
        control
    )))?;
    match selection.command.get_kind() {
        CommandKind::Encoder => {
            return Ok((CommandKind::Encoder, config.thresholds.encoder.clone()));
        }
        CommandKind::Switch => {
            return Ok((CommandKind::Switch, config.thresholds.switch.clone()));
        }
    };
}

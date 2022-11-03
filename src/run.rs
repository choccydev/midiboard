use super::types::{
    Activation, ActivationKind, Command, CommandData, CommandKind, Config, ControlList,
    ControlListByKey, InitialSwitchState, KeyEvent, KeyState,
};
use super::util::{self, Logger};
use anyhow::Error;
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::collections::HashMap;
use std::process;
use std::str::from_utf8;
use std::thread;
use std::time::{Duration, Instant};

pub fn run(cli: &clap::ArgMatches) -> Result<(), Error> {
    let path = cli.get_one::<String>("path");

    let config_data = util::read_user_config(path)?;

    let log_level = config_data.log_level.clone();

    let log = Logger::new(log_level);

    log.trace(
        "configuration file loaded correctly, log level set.",
        Some(&config_data),
    );

    for config in config_data.config {
        let builder = thread::Builder::new();

        log.trace("Built new thread", &builder);

        log.trace("Passing current config to device handler", &config);

        let handle =
            builder.spawn(
                move || match handle_device(config.device.clone(), config, log) {
                    Ok(_) => (),
                    Err(error) => {
                        log.error(error.to_string().as_str());
                        log.warn("Probably exiting now.");
                    }
                },
            )?;

        log.trace("Thread started and handle set", &handle);

        match handle.join() {
            Ok(_) => {}
            Err(error) => {
                log.default(format!("\n{:?}\n", error).as_str());
                log.fatal("There has been a fatal error in a connection thread.");
            }
        };
    }
    Ok(())
}

pub fn handle_device(device: String, config: Config, log: Logger) -> Result<(), Error> {
    //FIXME:Patch check what's the deal with alsa_seq() leaking memory

    //TODO:Minor Add error handling in case of dropped connection or device error (maybe with a heartbeat? The midir lib sucks)

    let controls = config.get_controls_by_key();

    log.trace("Gotr controls list indexed by key", &controls);

    let mut states: HashMap<u8, Option<KeyState>> = HashMap::new();

    for control in controls.clone() {
        states.insert(control.0, None);
    }

    log.trace("State set and populated", &states);

    log.info("Opening connection...");

    let conn = create_connection(&device, states, controls, config, log);

    match conn {
        Ok(_) => {
            log.trace("Connection created correctly, starting main loop", "");
            loop {}
        }
        Err(error) => {
            log.warn("Something went wrong. Connection closed.");
            return Err(error);
        }
    }
}

pub fn create_connection(
    device: &String,
    mut states: HashMap<u8, Option<KeyState>>,
    controls: HashMap<u8, String>,
    config: Config,
    log: Logger,
) -> Result<MidiInputConnection<()>, Error> {
    let mut midi_input = MidiInput::new("Midiboard: Runtime")?;
    midi_input.ignore(Ignore::None);

    log.trace(
        "Connecting and Waiting for messages to execute callback",
        "",
    );

    let port = util::get_input_port(&device, log)?;
    match midi_input.connect(
        &port.clone(),
        &device,
        move |_stamp, message, _| {
            let key = message[1];
            let value = message[2];

            log.trace(
                "Callback reached, testing if it's a valid control",
                format!("key: {}, velocity: {}", key, value).as_str(),
            );

            match states.get(&key) {
                Some(state) => {
                    log.debug(
                        format!("Control {} detected.", &controls.get(&key).unwrap()).as_str(),
                    );
                    log.trace("Testing for state initialization", &state);
                    match on_key_event(key, state.clone(), &config, &controls, value) {
                        Ok(mut key_event) => match key_event.initialized {
                            true => {
                                log.trace("State is initialized, starting debounce", &key_event);
                                match debounce(&mut key_event, log) {
                                    Ok(activation) => {
                                        log.trace("Detection data passed", &activation);
                                        if activation.valid {
                                            log.trace("Activation valid, calling commands", "");
                                            match call_command(
                                                &key_event,
                                                &activation,
                                                &config.controls,
                                                log,
                                            ) {
                                                Ok(command) => log.info(
                                                    format!("Executed command {}", command)
                                                        .as_str(),
                                                ),
                                                Err(error) => log.error(&error.to_string()),
                                            };
                                            log.trace(
                                                "Managing current state",
                                                &states.get(&key).unwrap(),
                                            );
                                            states.remove(&key);
                                            match &key_event.kind {
                                                CommandKind::Switch => {
                                                    log.trace(
                                                        "Event is from a Switch, state is kept",
                                                        &key_event.state,
                                                    );
                                                    // Persist state for switches
                                                    states.insert(key, Some(key_event.state));
                                                }
                                                _ => {
                                                    log.trace("State is discarded", "");
                                                    states.insert(key, None);
                                                }
                                            }
                                        } else {
                                            log.trace("Activation invalid", &activation);
                                        }
                                    }
                                    Err(error) => log.error(&error.to_string()),
                                }
                            }
                            false => {
                                log.trace(
                                    "State is not initialized, populating it",
                                    &key_event.state,
                                );
                                states.remove(&key);
                                states.insert(key, Some(key_event.state));
                            }
                        },
                        Err(error) => log.error(&error.to_string()),
                    };
                }
                None => {
                    log.trace("Not a valid control", "");
                }
            }
        },
        (),
    ) {
        Ok(connection) => Ok(connection),
        Err(error) => Err(Error::msg(error.kind().clone().to_string())),
    }
}

pub fn call_command(
    event: &KeyEvent,
    activation: &Activation,
    config_data: &ControlList,
    log: Logger,
) -> Result<String, Error> {
    let command = &config_data
        .get(&event.state.control)
        .ok_or(Error::msg(
            "Missing config data or wrong control name provided at command call",
        ))?
        .command;

    let activation_data = activation.kind.as_ref().ok_or(Error::msg(
        "Missing activation kind for registered activation at command call",
    ))?;

    if activation_data.get_kind() == command.get_kind() {
        match command {
            Command::Encoder(data) => {
                if let ActivationKind::Encoder {
                    increase: is_increase,
                } = activation_data
                {
                    let command_data: &CommandData;
                    if *is_increase {
                        command_data = &data.increase;
                    } else {
                        command_data = &data.decrease;
                    }

                    spawn_command(&event.state.control, command_data, log)
                } else {
                    return Err(Error::msg(
                        "Mismatched command types in activation and config at command call",
                    ));
                }
            }
            Command::Switch(data) => {
                if let ActivationKind::Switch { on: is_on } = activation_data {
                    let command_data: &CommandData;
                    if *is_on {
                        command_data = &data.on;
                    } else {
                        command_data = &data.off;
                    }

                    spawn_command(&event.state.control, command_data, log)
                } else {
                    return Err(Error::msg(
                        "Mismatched command types in activation and config at command call",
                    ));
                }
            }
            Command::Trigger(data) => {
                if let ActivationKind::Trigger = activation_data {
                    spawn_command(&event.state.control, &data.execute, log)
                } else {
                    return Err(Error::msg(
                        "Mismatched command types in activation and config at command call",
                    ));
                }
            }
        }
    } else {
        Err(Error::msg(
            "Mismatched command types between activation and recorded config at command call",
        ))
    }
}

pub fn spawn_command(control: &String, data: &CommandData, log: Logger) -> Result<String, Error> {
    let child = process::Command::new(data.cmd.clone())
        .args(data.args.clone())
        .output()?;

    if child.stdout.len() > 0 {
        log.message(from_utf8(child.stdout.as_slice())?, data.cmd.as_str());
    }

    if child.stderr.len() > 0 {
        log.message(from_utf8(child.stderr.as_slice())?, data.cmd.as_str());
    }

    let success = child.status.success();

    if success {
        return Ok(format!("{} successfully.", control));
    } else {
        return Err(Error::msg(format!("{} failed to execute.", control)));
    }
}

pub fn on_key_event(
    key: u8,
    state: Option<KeyState>,
    config: &Config,
    controls: &ControlListByKey,
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
            let threshold_data = config.get_threshold(key)?;
            let threshold = threshold_data.1;
            match state {
                None => {
                    let command_data = &config.get_control(somekey)?.command;
                    let mut new_state = KeyState {
                        control: somekey.clone(),
                        time_threshold: Duration::from_millis(threshold.detection),
                        activation_threshold: Duration::from_millis(threshold.activation),
                        detections: Vec::new(),
                        start: Instant::now(),
                        initial_state: match command_data {
                            Command::Switch(data) => Some(data.initial_state),
                            _ => None,
                        },
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
                    let mut new_state = state.clone();
                    new_state.detections.push(value);

                    return Ok(KeyEvent {
                        initialized: true,
                        state: new_state,
                        kind: threshold_data.0,
                        elapsed: Some(Instant::now().duration_since(state.start)),
                    });
                }
            }
        }
    }
}

pub fn debounce(event: &mut KeyEvent, log: Logger) -> Result<Activation, Error> {
    let activation_threshold = event.state.activation_threshold;
    let time_threshold = event.state.time_threshold;
    let elapsed = event.elapsed.unwrap();

    // TODO:Minor Add proportional reading of increases to actually modify data using percentuals
    // TODO:Minor Add easing to the controls reaction
    // TODO:Minor Register detections and use the composite delta/derivative to gauge activations
    // TODO:Major Add a way to tell the Midi Out that on reaching max value on Encoders, it should set the velocity as 0, that way it can wrap around

    match event.kind {
        CommandKind::Encoder => {
            if elapsed.gt(&activation_threshold) {
                // TODO:Patch Refactor this accumulator function stuff into its own function
                let mut accumulator: i16 = 0;

                log.trace("Encoder debounce: Correct activation", &event);

                // FIXME:Patch This encoder accumulator function is kinda weird, sums weirdly at high values
                for (index, detection) in event.state.detections.iter().enumerate() {
                    if index % 2 == 0 {
                        accumulator += Into::<i16>::into(*detection);
                    } else {
                        accumulator -= Into::<i16>::into(*detection);
                    }
                }

                log.trace("Encoder debounce: Accumulator", &accumulator);

                let is_increase = accumulator.lt(&0);

                // then reset the detection vec to account for a new detection next time
                event.state.detections = Vec::new();

                let activation = Activation {
                    valid: true,
                    kind: Some(ActivationKind::Encoder {
                        increase: is_increase,
                    }),
                };

                log.trace("Encoder debounce: Activation data", &activation);

                Ok(activation)
            } else {
                if elapsed.lt(&time_threshold) {
                    // remove detection from pool
                    event.state.detections.pop();

                    log.trace(
                        "Encoder debounce: Spurious activation over detecion threshold",
                        Some(&event),
                    );

                    Ok(Activation {
                        valid: false,
                        kind: None,
                    })
                } else {
                    log.trace(
                        "Encoder debounce: Spurious activation under detecion threshold",
                        Some(&event),
                    );
                    Ok(Activation {
                        valid: false,
                        kind: None,
                    })
                }
            }
        }
        CommandKind::Switch => {
            if elapsed.gt(&activation_threshold) {
                // Reset elapsed time
                event.state.start = Instant::now();
                event.elapsed = Some(Duration::from_millis(0));

                if event.state.detections.len() == 2 {
                    // HACK:Minor we use 255 as OFF and anything else as ON.
                    // We can do that because the MIDI lib only supports MIDI 1.0, which limits velocities to 7 bits.

                    if let Some(initial_state) = event.state.initial_state {
                        // This is first activation so we gotta check the initial state in the config
                        match initial_state {
                            InitialSwitchState::OFF => {
                                event.state.detections = vec![255, 255];
                                Ok(Activation {
                                    valid: true,
                                    kind: Some(ActivationKind::Switch { on: false }),
                                })
                            }
                            InitialSwitchState::ON => {
                                event.state.detections = vec![200, 200]; // 200 is an arbitrary choice, it does not matter.
                                Ok(Activation {
                                    valid: true,
                                    kind: Some(ActivationKind::Switch { on: false }),
                                })
                            }
                        }
                    } else {
                        Err(Error::msg(format!(
                            "Initial state for control {} not found in the config.",
                            event.state.control
                        )))
                    }
                } else {
                    let is_currently_on: bool;
                    event.state.detections.pop(); //remove actual new value
                                                  // redefine detections to represent states
                    if event.state.detections.last().unwrap() == &255 {
                        is_currently_on = false;
                        event.state.detections.push(200); // set as now on
                    } else {
                        is_currently_on = true;
                        event.state.detections.push(255); // Set as now off
                    }

                    // ? truncates the vec if it's too large, to avoid massive potential leaks (the MIDI lib closure possible is leaking this) on long run times
                    if event.state.detections.len() > 50 {
                        event.state.detections =
                            event.state.detections[event.state.detections.len() - 3..].to_vec()
                    }

                    Ok(Activation {
                        valid: true,
                        kind: Some(ActivationKind::Switch {
                            on: !is_currently_on,
                        }),
                    })
                }
            } else {
                event.state.detections.pop();
                Ok(Activation {
                    valid: false,
                    kind: None,
                })
            }
        }
        CommandKind::Trigger => {
            if elapsed.gt(&activation_threshold) {
                // Reset elapsed time and detections
                event.state.start = Instant::now();
                event.state.detections = Vec::new();
                event.elapsed = Some(Duration::from_millis(0));

                Ok(Activation {
                    valid: true,
                    kind: Some(ActivationKind::Trigger),
                })
            } else {
                Ok(Activation {
                    valid: false,
                    kind: None,
                })
            }
        }
    }
}

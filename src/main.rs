use std::error::Error;

mod types;
mod util;

use midir::{Ignore, MidiInput};

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("[Error] {}", err),
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    let config = util::read_user_config()?.try_deserialize::<types::Config>()?;

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
    let _conn_in = midi_in.connect(
        in_port,
        "midir-read-input",
        move |_stamp, message, _| {
            println!("key: {}, value: {}", message[1], message[2]);
        },
        (),
    )?;

    println!(
        "[Info] Connection open, reading input from '{}'",
        in_port_name
    );

    println!("\n[Debug] Config: \n {:?}\n", config);
    loop {}
}

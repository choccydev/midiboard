use clap::{arg, value_parser, Arg, Command};
use colored::*;
use std::error::Error;
use std::{env, fs, io, process};

mod config;
mod run;
mod test;
mod types;
mod util;

use midir::{Ignore, MidiInput, MidiInputConnection};

// TODO Update this shit please
fn main() {
    let matches = Command::new("midiboard")
        .version("0.3.0")
        .author(util::string_to_sstr(format!("{}", "Agata Ordano - aordano@protonmail.com".bright_cyan())))
        .about("Utility that allows using an arbitrary MIDI controller as a control board.")
        .long_about(concat! ("This utility helps with the execution of frequent or specific tasks to be done using a MIDI controller to execute user-provided commands.\n", 
        "It can be used to control audio, system resoruces, or anything that runs off a shell command.\n\n"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("test")
                .alias("active")
            .about("Detects and tests currently active MIDI devices.")
            .long_about(util::string_to_sstr(
                format!("{}\n {}",
                    "This command lets you know what devices you have active, their names, and check if they're working correctly. ".yellow(),
                    concat!("It can provide a list of devices, and with a device selected it can output any MIDI event registered. ",
                    "This is useful to know what channels, keys and type of values your device outputs, making easy to fill the config file. ",
                    "Check the help on the arguments for more information.")
                )
            ))
            .arg_required_else_help(true)
            .arg(
                Arg::new("list")
                .alias("devices")
                .short('d')
                .long("list")
                .takes_value(false)
                .help("Lists active MIDI devices and outputs them to stdout.")
                .conflicts_with("listen")
            )
            .arg(
                Arg::new("listen")
                .alias("inputs")
                .short('l')
                .long("listen")
                .multiple_values(false)
                .value_name("DEVICE")
                .takes_value(true)
                .conflicts_with("list")
                .help("Listens to the given MIDI device and outputs all events to stdout.")
            )
        )
        // TODO
        .subcommand(
            Command::new("config")
                .alias("settings")
            .about("Manages the configuration file.")
            .long_about(util::string_to_sstr(
                format!("{}\n {} {} {}", 
                    "This command allows you to generate a skeleton for the config file, or test validity of an existing one.".yellow(), 
                    "By default the configuration file will be generated and read from ", 
                    "$HOME".bright_purple(),
                    concat!(", but you can select an alternative path if desired.")
                )
            ))
            .arg_required_else_help(true)
            .arg(
                Arg::new("validate")
                .alias("test")
                .short('v')
                .long("validate")
                .takes_value(false)
                .help("Validates the config file.")
                .conflicts_with("generate")
            )
            .arg(
                Arg::new("generate")
                .short('g')
                .long("generate")
                .alias("skeleton")
                .alias("blueprint")
                .takes_value(false)
                .conflicts_with("validate")
                .help("Generates a skeleton config file.")
            )
        )
        // TODO
        .subcommand(
            Command::new("run")
                .alias("listen")
            .about("Runs the service, listening to incoming events and executing the given configuration.")
            .long_about(util::string_to_sstr(
                format!("{}\n {}{}{}", 
                "Executes given commands on defined MIDI events according to the config file.".yellow(), 
                "By default the configuration file will be generated and read from ", 
                "$HOME".bright_purple(),
                concat!(", but you can select an alternative path if desired.")
            )
            ))
            .arg_required_else_help(false)
        )
        .arg(
            Arg::new("path")
            .short('p')
            .long("path")
            .value_name("CONFIG FILE")
            .alias("file")
            .takes_value(true)
            .multiple_values(false)
            .help("Selects a custom path for the config file.")
        )
        .get_matches();
    if let Err(error) = run(matches) {
        println!("Application error: {}", error);
        process::exit(1);
    }
}

fn run(cli: clap::ArgMatches) -> Result<(), String> {
    // TODO do stuff

    return match cli.subcommand() {
        Some(("test", sub_m)) => test::run(sub_m),
        Some(("run", sub_m)) => run::run(sub_m),
        Some(("config", sub_m)) => config::run(sub_m),
        _ => Ok(()), //Some(("", sub_m)) => util::stdout("warning", "Please provide a subcommand. You can call this tool without arguments or with the --help flag for more information.")
    };
}

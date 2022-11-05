use anyhow::Error;
use clap::{Arg, Command};
use colored::*;

mod config;
mod devices;
mod run;
#[cfg(test)]
mod tests;
mod types;
mod util;

fn main() {
    let matches = Command::new("midiboard")
        .version("0.3.3")
        .author(util::string_to_sstr(format!("{}", "Agata Ordano - aordano@protonmail.com".bright_cyan())))
        .about("Utility that allows using an arbitrary MIDI controller as a control board.")
        .long_about(concat! ("This utility helps with the execution of frequent or specific tasks to be done using a MIDI controller to execute user-provided commands.\n", 
        "It can be used to control audio, system resoruces, or anything that runs off a shell command.\n\n"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("devices")
                .alias("active")
            .about("Detects or listens to currently active MIDI devices.")
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
                .alias("print")
                .short('l')
                .long("list")
                .num_args(0)
                .help("Lists active MIDI devices and outputs them to stdout.")
                .conflicts_with("listen")
            )
            .arg(
                Arg::new("listen")
                .alias("input")
                .short('i')
                .long("input")
                .num_args(1)
                .value_name("DEVICE")
                .conflicts_with("list")
                .help("Listens to the given MIDI device and outputs all events to stdout.")
            )
        )
        .subcommand(
            Command::new("config")
                .alias("settings")
            .about("Manages the configuration file.")
            .long_about(util::string_to_sstr(
                format!("{}\n {} {} {}", 
                    "This command allows you to generate a skeleton for the config file, or devices validity of an existing one.".yellow(), 
                    "By default the configuration file will be generated and read from ", 
                    "$HOME".bright_purple(),
                    concat!(", but you can select an alternative path if desired.")
                )
            ))
            .arg_required_else_help(true)
            .arg(
                Arg::new("validate")
                .alias("devices")
                .short('v')
                .long("validate")
                .num_args(0)
                .help("Validates the config file.")
                .conflicts_with("generate")
            )
            .arg(
                Arg::new("generate")
                .short('g')
                .long("generate")
                .alias("skeleton")
                .alias("blueprint")
                .num_args(0)
                .conflicts_with("validate")
                .help("Generates a skeleton config file.")
            )
            .arg(
                Arg::new("path")
                .short('p')
                .long("path")
                .value_name("CONFIG FILE")
                .alias("file")
                .num_args(1)
                .help("Selects a custom path for the config file.")
            )
        )
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
            .arg(
                Arg::new("path")
                .short('p')
                .long("path")
                .value_name("CONFIG FILE")
                .alias("file")
                .num_args(1)
                .help("Selects a custom path for the config file.")
            )
            .arg_required_else_help(false)
        )
        .get_matches();
    let runtime = run(matches);
    match runtime {
        Ok(()) => {}
        Err(error) => util::Logger::fatal(Default::default(), &error.to_string()),
    }
}

fn run(cli: clap::ArgMatches) -> Result<(), Error> {
    return match cli.subcommand() {
        Some(("devices", sub_m)) => devices::run(sub_m),
        Some(("run", sub_m)) => run::run(sub_m),
        Some(("config", sub_m)) => config::run(sub_m),
        _ => Ok(()),
    };
}

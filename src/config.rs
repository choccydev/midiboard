use super::types;
use super::util;
use anyhow::Error;
use config::ConfigError;
use home::home_dir;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
// TODO:Major Add interactive system and sub-subcommands to interactively generate and modify the config files
//   Replace the `--generate` for a `new` subcommand. Make `new` generate a empty file unless it uses the flag `--skeleton`
//   Add a `add` subcommand
//   Add a `add device` subcommand to generate an entry on the `config` array
//   Add a `add control` subcommand to insert a new control for the selected device
//   Make the commands interactive via interaction with stdin

pub fn run(cli: &clap::ArgMatches) -> Result<(), Error> {
    let generate = cli
        .get_one::<bool>("generate")
        .ok_or(Error::msg("Bad --generate command."))?;
    let validate = cli
        .get_one::<bool>("validate")
        .ok_or(Error::msg("Bad --validate command."))?;
    let path = cli.get_one::<String>("path");

    if *generate {
        return generate_config(path);
    }

    if *validate {
        return validate_config(path);
    }

    panic!("No valid argument provided to the config subcommand.")
}

fn generate_config(path: Option<&String>) -> Result<(), Error> {
    let skeleton = types::Asset::get("midiboard.json")
        .ok_or(Error::msg("Could not load the skeleton file"))?;
    let mut fullpath = PathBuf::new();

    match path {
        None => {
            fullpath.push(
                home_dir().ok_or(ConfigError::Message(String::from("Could not parse path")))?,
            );

            fullpath.push("midiboard");
            fullpath.set_extension("json");
        }
        Some(path) => fullpath.push(path),
    }
    return match Path::try_exists(&fullpath) {
        Ok(exists) => match exists {
            true => Err(Error::msg(util::string_to_sstr(format!(
                "File already exists in path {:?}",
                fullpath
            )))),
            false => Ok(fs::write(fullpath, skeleton.data)?),
        },
        Err(_) => Err(Error::msg(util::string_to_sstr(format!(
            "Cannot access path {:?}",
            fullpath
        )))),
    };
}

fn validate_config(path: Option<&String>) -> Result<(), Error> {
    let config = util::read_user_config(path);

    return match config {
        Ok(data) => {
            let log = util::Logger::new(data.log_level);
            log.dynamic(
                format!(
                    "Log level set at {:?}. Messages will be shown according to it.",
                    data.log_level
                )
                .as_str(),
                format!("{:?}", data.log_level).to_lowercase().as_str(),
                None,
            );
            log.debug(format!("{:#?}", data).as_str());
            log.success("Config file validated correctly.");
            Ok(())
        }
        Err(error) => Err(Error::from(error)),
    };
}

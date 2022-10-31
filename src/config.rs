use super::types;
use super::util;
use anyhow::Error;
use config::ConfigError;
use home::home_dir;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub fn run(cli: &clap::ArgMatches) -> Result<(), Error> {
    let generate = cli.is_present("generate");
    let validate = cli.is_present("validate");
    let path = cli.get_one::<String>("path");

    if generate {
        return generate_config(path);
    }

    if validate {
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
        Ok(_) => {
            util::Logger::success(Default::default(), "Config file validated correctly.");
            Ok(())
        }
        Err(error) => Err(Error::from(error)),
    };
}

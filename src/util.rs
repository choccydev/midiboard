use config::{Config, ConfigError};
use home::home_dir;
use std::path::PathBuf;

pub fn read_user_config() -> Result<Config, ConfigError> {
    let mut path = PathBuf::new();

    // TODO Uncomment this line after all main features are added and this won't change further
    //path.push(home_dir().ok_or(ConfigError::Message(String::from("Could not parse path")))?);
    path.push("control-board");
    path.set_extension("json5");

    // load and return the config
    let config = Config::builder()
        .add_source(config::File::new(
            path.as_os_str()
                .to_str()
                .ok_or(ConfigError::Message(String::from("Could not parse path")))?,
            config::FileFormat::Json5,
        ))
        .build();
    return config;
}

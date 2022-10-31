use super::*;
use ::config;
use std::{fs::write, str::from_utf8};
use tempfile::tempdir;

#[test]
fn read_user_config_paths() {
    let dir = tempdir().unwrap();
    let mut asset = types::Asset::get("midiboard.json").unwrap();
    let skeleton = from_utf8(asset.data.to_mut().as_slice()).unwrap();
    let config_path = dir.path().join("midiboard.json");
    let _file = write(&config_path, skeleton).unwrap();

    let read_config = util::read_user_config(Some(&String::from(
        config_path.as_os_str().to_str().unwrap(),
    )))
    .unwrap();

    let parsed_config = config_from_str(skeleton);

    for (index, config) in read_config.config.into_iter().enumerate() {
        assert_eq!(config, parsed_config.config[index])
    }
    drop(parsed_config);
    dir.close().unwrap()
}

#[test]
fn generate_config() {}

fn config_from_str(config_str: &str) -> types::ConfigFile {
    let mut config_asset: config::Config = config::Config::default();
    config_asset
        .merge(config::File::from_str(config_str, config::FileFormat::Json))
        .unwrap();

    config_asset.try_deserialize::<types::ConfigFile>().unwrap()
}

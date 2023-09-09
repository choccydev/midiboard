use super::*;
use ::config;
use chrono::Duration;
use rand::{self, Rng};
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

#[test]
fn ease_input_test() {
    let runs = 5;
    let detections = 10;
    let min_threshold = 300;
    let max_threshold = 2000;
    let threshold = min_threshold..max_threshold;
    let value_displacement = 4..15;
    let time_displacement = min_threshold / detections..max_threshold / detections;

    for _run in 0..runs {
        let threshold_duration =
            Duration::milliseconds(rand::thread_rng().gen_range(threshold.clone()));
        let mut elapsed = Duration::milliseconds(10);
        let mut value = 62; // put it in the middle to make things easier
        let mut results_pos: Vec<u8> = Vec::new();
        let mut results_neg: Vec<u8> = Vec::new();

        // Positive runs
        for _detection in 0..detections {
            results_pos.push(util::ease_input(&threshold_duration, &elapsed, value));
            elapsed = elapsed
                + Duration::milliseconds(rand::thread_rng().gen_range(time_displacement.clone()));
            value += rand::thread_rng().gen_range(value_displacement.clone());
        }

        elapsed = Duration::milliseconds(10);
        let mut value = 62;

        // Negative runs
        for _detection in 0..detections {
            results_neg.push(util::ease_input(&threshold_duration, &elapsed, value));
            elapsed = elapsed
                + Duration::milliseconds(rand::thread_rng().gen_range(time_displacement.clone()));
            let value_sub: i16 = rand::thread_rng()
                .gen_range(value_displacement.clone())
                .into();

            value = i16::clamp(Into::<i16>::into(value) - value_sub, 0, 127)
                .try_into()
                .unwrap();
        }

        println!(
            "positive run: {:#?}, negative run: {:#?}",
            results_pos, results_neg
        );
    }
}

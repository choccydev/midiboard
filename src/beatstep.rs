//! Arturia BeatStep (original, non-Pro) device profile.
//!
//! Arturia doesn't publish this protocol; it comes from community reverse
//! engineering (see docs/feedback.md for sources). Messages have the form
//!
//! ```text
//! set: F0 00 20 6B 7F 42 02 00 <param> <control> <value> F7
//! get: F0 00 20 6B 7F 42 01 00 <param> <control> F7
//! ```
//!
//! Param 0x10 is the LED color. Pads 1-16 are controls 0x70-0x7F, and the
//! function buttons are 0x58-0x5F (Stop has no LED). Encoders 1-16 are
//! controls 0x20-0x2F and the big volume knob is 0x30; for them param 0x01
//! is the mode (1 = CC), 0x02 the channel (0-based), 0x03 the CC number and
//! 0x06 the behavior (0 = absolute, 1-3 = relative #1-#3). For pads, 0x01 is
//! the mode (9 = note, 8 = CC switch), 0x03 the note and 0x06 the behavior
//! (0 = toggle, 1 = gate). Knob acceleration is param 0x41, control 0x04.

use crate::config::LedTarget;

const HEADER: [u8; 8] = [0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00];
const GET_HEADER: [u8; 8] = [0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00];
const PARAM_LED: u8 = 0x10;
pub const PARAM_MODE: u8 = 0x01;
pub const PARAM_CHANNEL: u8 = 0x02;
pub const PARAM_NUMBER: u8 = 0x03;
/// Encoders: 0 absolute, 1-3 relative #1-#3. Pads: 0 toggle, 1 gate.
pub const PARAM_BEHAVIOR: u8 = 0x06;
const PARAM_ACCELERATION: u8 = 0x41;
const CONTROL_ACCELERATION: u8 = 0x04;
pub const MODE_CC: u8 = 0x01;
const PAD_FIRST: u8 = 0x70;
pub const ENCODER_FIRST: u8 = 0x20;
pub const VOLUME_KNOB: u8 = 0x30;

const BUTTONS: [(&str, u8); 7] =
    [("play", 0x58), ("seq", 0x5A), ("sync", 0x5B), ("recall", 0x5C), ("store", 0x5D), ("shift", 0x5E), ("chan", 0x5F)];

const COLORS: [(&str, u8); 5] = [("off", 0x00), ("red", 0x01), ("blue", 0x10), ("magenta", 0x11), ("pink", 0x11)];

pub fn set_message(param: u8, control: u8, value: u8) -> Vec<u8> {
    let mut msg = HEADER.to_vec();
    msg.extend_from_slice(&[param, control, value, 0xF7]);
    msg
}

pub fn led_message(control: u8, color: u8) -> Vec<u8> {
    set_message(PARAM_LED, control, color)
}

pub fn gate_mode_message(pad_control: u8) -> Vec<u8> {
    set_message(PARAM_BEHAVIOR, pad_control, 0x01)
}

pub fn get_message(param: u8, control: u8) -> Vec<u8> {
    let mut msg = GET_HEADER.to_vec();
    msg.extend_from_slice(&[param, control, 0xF7]);
    msg
}

/// Parses a parameter reply (`F0 00 20 6B 7F 42 02 00 <param> <control> <value> F7`).
pub fn parse_reply(bytes: &[u8]) -> Option<(u8, u8, u8)> {
    match bytes {
        [h @ .., param, control, value, 0xF7] if h == HEADER => Some((*param, *control, *value)),
        _ => None,
    }
}

pub fn pad_id(pad: u8) -> u8 {
    PAD_FIRST + pad - 1
}

pub fn encoder_id(encoder: u8) -> u8 {
    ENCODER_FIRST + encoder - 1
}

/// Encoder behaviors: (CLI name, param value, matching midiboard `mode`).
pub const ENCODER_BEHAVIORS: [(&str, u8, &str); 4] = [
    ("absolute", 0, "absolute"),
    ("relative1", 1, "relative_offset64"),
    ("relative2", 2, "relative_twos"),
    ("relative3", 3, "relative_offset16"),
];

pub fn encoder_behavior(name: &str) -> Result<u8, String> {
    ENCODER_BEHAVIORS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, v, _)| *v)
        .ok_or_else(|| format!("unknown encoder mode \"{name}\"; expected absolute, relative1, relative2 or relative3"))
}

pub fn encoder_behavior_name(value: u8) -> Option<(&'static str, &'static str)> {
    ENCODER_BEHAVIORS.iter().find(|(_, v, _)| *v == value).map(|(n, _, m)| (*n, *m))
}

pub fn acceleration_message(name: &str) -> Result<Vec<u8>, String> {
    let v = match name {
        "slow" => 0,
        "medium" => 1,
        "fast" => 2,
        _ => return Err(format!("unknown acceleration \"{name}\"; expected slow, medium or fast")),
    };
    Ok(set_message(PARAM_ACCELERATION, CONTROL_ACCELERATION, v))
}

/// Messages that switch an encoder to CC mode with the given behavior,
/// keeping its channel and CC number.
pub fn encoder_setup_messages(control: u8, behavior: u8) -> Vec<Vec<u8>> {
    vec![set_message(PARAM_MODE, control, MODE_CC), set_message(PARAM_BEHAVIOR, control, behavior)]
}

/// Parses a list like `all`, `1,2,5-8` into numbers within `1..=max`.
pub fn parse_selection(input: &str, max: u8) -> Result<Vec<u8>, String> {
    if input.trim() == "all" {
        return Ok((1..=max).collect());
    }
    let mut out = Vec::new();
    for part in input.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let parse = |s: &str| -> Result<u8, String> {
            s.trim()
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=max).contains(n))
                .ok_or_else(|| format!("`{s}` is not a number 1-{max}"))
        };
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (parse(a)?, parse(b)?);
                if a > b {
                    return Err(format!("`{part}` is an empty range"));
                }
                out.extend(a..=b);
            }
            None => out.push(parse(part)?),
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err("select at least one".into());
    }
    Ok(out)
}

pub fn is_pad(control: u8) -> bool {
    (PAD_FIRST..PAD_FIRST + 16).contains(&control)
}

/// Resolves a `led` target to the BeatStep control ID.
pub fn led_id(target: &LedTarget) -> Result<u8, String> {
    match (target.pad, target.button.as_deref()) {
        (Some(pad), None) if (1..=16).contains(&pad) => Ok(pad_id(pad)),
        (Some(pad), None) => Err(format!("BeatStep pads are numbered 1-16, got {pad}")),
        (None, Some("stop")) => Err("the BeatStep Stop button has no LED".into()),
        (None, Some(name)) => BUTTONS.iter().find(|(n, _)| *n == name).map(|(_, id)| *id).ok_or_else(|| {
            let names: Vec<&str> = BUTTONS.iter().map(|(n, _)| *n).collect();
            format!("unknown BeatStep button \"{name}\"; expected one of {}", names.join(", "))
        }),
        _ => Err("`led` needs exactly one of `pad` or `button`".into()),
    }
}

pub fn color(name: &str) -> Result<u8, String> {
    COLORS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
        .ok_or_else(|| format!("unknown BeatStep color \"{name}\"; expected off, red, blue or magenta"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(n: u8) -> LedTarget {
        LedTarget { pad: Some(n), button: None }
    }

    #[test]
    fn pad_ids() {
        assert_eq!(led_id(&pad(1)), Ok(0x70));
        assert_eq!(led_id(&pad(16)), Ok(0x7F));
        assert!(led_id(&pad(0)).is_err());
        assert!(led_id(&pad(17)).is_err());
    }

    #[test]
    fn button_ids() {
        let b = |s: &str| LedTarget { pad: None, button: Some(s.into()) };
        assert_eq!(led_id(&b("play")), Ok(0x58));
        assert!(led_id(&b("stop")).unwrap_err().contains("no LED"));
        assert!(led_id(&b("nope")).is_err());
        assert!(led_id(&LedTarget { pad: Some(1), button: Some("play".into()) }).is_err());
    }

    #[test]
    fn encoder_setup() {
        assert_eq!(encoder_id(1), 0x20);
        assert_eq!(encoder_id(16), 0x2F);
        assert_eq!(
            encoder_setup_messages(0x20, encoder_behavior("relative1").unwrap()),
            vec![
                vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x01, 0x20, 0x01, 0xF7],
                vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x06, 0x20, 0x01, 0xF7],
            ]
        );
        assert_eq!(encoder_behavior_name(1), Some(("relative1", "relative_offset64")));
        assert!(encoder_behavior("relative4").is_err());
        assert_eq!(acceleration_message("fast").unwrap()[8..11], [0x41, 0x04, 0x02]);
    }

    #[test]
    fn get_and_reply() {
        assert_eq!(get_message(0x06, 0x30), vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x01, 0x00, 0x06, 0x30, 0xF7]);
        assert_eq!(parse_reply(&set_message(0x03, 0x70, 36)), Some((0x03, 0x70, 36)));
        assert_eq!(parse_reply(&get_message(0x03, 0x70)), None);
        assert_eq!(parse_reply(&[0x90, 1, 2]), None);
    }

    #[test]
    fn selections() {
        assert_eq!(parse_selection("all", 16).unwrap().len(), 16);
        assert_eq!(parse_selection("3, 1,5-7,6", 16).unwrap(), vec![1, 3, 5, 6, 7]);
        assert!(parse_selection("0", 16).is_err());
        assert!(parse_selection("17", 16).is_err());
        assert!(parse_selection("5-2", 16).is_err());
        assert!(parse_selection("", 16).is_err());
    }

    #[test]
    fn messages() {
        assert_eq!(
            led_message(0x70, color("red").unwrap()),
            vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00, 0x10, 0x70, 0x01, 0xF7]
        );
        assert_eq!(color("Pink"), color("magenta"));
        assert!(color("green").is_err());
        assert_eq!(gate_mode_message(0x70)[8..11], [0x06, 0x70, 0x01]);
    }
}

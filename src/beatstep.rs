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
//! function buttons are 0x58-0x5F (Stop has no LED).

use crate::config::LedTarget;

const HEADER: [u8; 8] = [0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x00];
const PARAM_LED: u8 = 0x10;
/// Pad behavior: 0x00 toggle, 0x01 gate.
const PARAM_PAD_BEHAVIOR: u8 = 0x06;
const PAD_FIRST: u8 = 0x70;

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
    set_message(PARAM_PAD_BEHAVIOR, pad_control, 0x01)
}

pub fn is_pad(control: u8) -> bool {
    (PAD_FIRST..PAD_FIRST + 16).contains(&control)
}

/// Resolves a `led` target to the BeatStep control ID.
pub fn led_id(target: &LedTarget) -> Result<u8, String> {
    match (target.pad, target.button.as_deref()) {
        (Some(pad), None) if (1..=16).contains(&pad) => Ok(PAD_FIRST + pad - 1),
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

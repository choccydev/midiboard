//! MIDI message parsing and port selection.
//!
//! Everything here is pure and free of any MIDI backend, so it can be unit
//! tested without ALSA or CoreMIDI.

use std::fmt;

/// A decoded channel-voice message. Channels are 1-based (1..=16), as they
/// are written in the config file and shown by most controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMsg {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    PolyAftertouch { channel: u8, note: u8, pressure: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    ChannelAftertouch { channel: u8, pressure: u8 },
    PitchBend { channel: u8, value: u16 },
}

impl MidiMsg {
    /// Parses one complete MIDI message as delivered by midir.
    ///
    /// Returns `None` for anything that is not a well-formed channel-voice
    /// message: realtime bytes (clock, active sensing), SysEx, system common
    /// messages, or truncated input. Note-on with velocity 0 is reported as
    /// note-off, which is what it means on the wire.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let status = *bytes.first()?;
        if !(0x80..0xF0).contains(&status) {
            return None;
        }
        let channel = (status & 0x0F) + 1;
        let data = |i: usize| -> Option<u8> {
            let b = *bytes.get(i)?;
            (b < 0x80).then_some(b)
        };
        Some(match status & 0xF0 {
            0x80 => MidiMsg::NoteOff { channel, note: data(1)?, velocity: data(2)? },
            0x90 => {
                let (note, velocity) = (data(1)?, data(2)?);
                if velocity == 0 {
                    MidiMsg::NoteOff { channel, note, velocity }
                } else {
                    MidiMsg::NoteOn { channel, note, velocity }
                }
            }
            0xA0 => MidiMsg::PolyAftertouch { channel, note: data(1)?, pressure: data(2)? },
            0xB0 => MidiMsg::ControlChange { channel, controller: data(1)?, value: data(2)? },
            0xC0 => MidiMsg::ProgramChange { channel, program: data(1)? },
            0xD0 => MidiMsg::ChannelAftertouch { channel, pressure: data(1)? },
            0xE0 => {
                let value = u16::from(data(1)?) | (u16::from(data(2)?) << 7);
                MidiMsg::PitchBend { channel, value }
            }
            _ => unreachable!("status nibble is always 0x8..=0xE here"),
        })
    }
}

impl fmt::Display for MidiMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            MidiMsg::NoteOn { channel, note, velocity } => {
                write!(f, "type: note  channel: {channel:>2}  key: {note:>3}  value: {velocity:>3}  (note on)")
            }
            MidiMsg::NoteOff { channel, note, velocity } => {
                write!(f, "type: note  channel: {channel:>2}  key: {note:>3}  value: {velocity:>3}  (note off)")
            }
            MidiMsg::ControlChange { channel, controller, value } => {
                write!(f, "type: cc    channel: {channel:>2}  key: {controller:>3}  value: {value:>3}")
            }
            MidiMsg::PolyAftertouch { channel, note, pressure } => {
                write!(
                    f,
                    "poly aftertouch  channel: {channel:>2}  note: {note:>3}  pressure: {pressure:>3}  (not bindable)"
                )
            }
            MidiMsg::ProgramChange { channel, program } => {
                write!(f, "program change  channel: {channel:>2}  program: {program:>3}  (not bindable)")
            }
            MidiMsg::ChannelAftertouch { channel, pressure } => {
                write!(f, "channel aftertouch  channel: {channel:>2}  pressure: {pressure:>3}  (not bindable)")
            }
            MidiMsg::PitchBend { channel, value } => {
                write!(f, "pitch bend  channel: {channel:>2}  value: {value:>5}  (not bindable)")
            }
        }
    }
}

/// Formats raw bytes as space-separated upper-case hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Parses space- or comma-separated hex bytes, with or without `0x`.
pub fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|tok| {
            let t = tok.trim_start_matches("0x").trim_start_matches("0X");
            u8::from_str_radix(t, 16).map_err(|_| format!("`{tok}` is not a hex byte"))
        })
        .collect()
}

/// Checks that `bytes` is a single, well-formed SysEx message.
pub fn validate_sysex(bytes: &[u8]) -> Result<(), String> {
    match bytes {
        [0xF0, body @ .., 0xF7] if body.iter().all(|b| *b < 0x80) => Ok(()),
        [0xF0, .., 0xF7] => Err("SysEx data bytes must be below 0x80".into()),
        _ => Err("SysEx must start with F0 and end with F7".into()),
    }
}

fn normalize(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect()
}

/// The client part of a port name. On ALSA, midir names ports
/// `"<client>:<port> <client id>:<port id>"`; on CoreMIDI they are plain.
pub fn client_name(port_name: &str) -> &str {
    port_name.split(':').next().unwrap_or(port_name)
}

#[derive(Debug, PartialEq, Eq)]
pub enum PortMatchError {
    NotFound { wanted: String, available: Vec<String> },
    Ambiguous { wanted: String, matches: Vec<String> },
}

impl fmt::Display for PortMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortMatchError::NotFound { wanted, available } => {
                write!(f, "no MIDI port matches \"{wanted}\".")?;
                if available.is_empty() {
                    write!(f, " No ports are available at all.")
                } else {
                    write!(f, " Available ports:")?;
                    for name in available {
                        write!(f, "\n  - {name}")?;
                    }
                    Ok(())
                }
            }
            PortMatchError::Ambiguous { wanted, matches } => {
                write!(f, "\"{wanted}\" matches more than one MIDI port; use the full port name:")?;
                for name in matches {
                    write!(f, "\n  - {name}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for PortMatchError {}

/// Picks the port `wanted` refers to, trying progressively looser rules and
/// stopping at the first rule that matches anything:
///
/// 1. exact full port name,
/// 2. client name (ignoring case and whitespace),
/// 3. full name (ignoring case and whitespace),
/// 4. case-insensitive substring of the full name.
///
/// More than one match under the winning rule is an error, so a device with
/// several ports is never picked at random.
pub fn select_port(names: &[String], wanted: &str) -> Result<usize, PortMatchError> {
    let w = normalize(wanted);
    let rules: [&dyn Fn(&str) -> bool; 4] =
        [&|n| n == wanted, &|n| normalize(client_name(n)) == w, &|n| normalize(n) == w, &|n| normalize(n).contains(&w)];
    for rule in rules {
        let hits: Vec<usize> = names.iter().enumerate().filter(|(_, n)| rule(n)).map(|(i, _)| i).collect();
        match hits.len() {
            0 => continue,
            1 => return Ok(hits[0]),
            _ => {
                return Err(PortMatchError::Ambiguous {
                    wanted: wanted.to_string(),
                    matches: hits.into_iter().map(|i| names[i].clone()).collect(),
                });
            }
        }
    }
    Err(PortMatchError::NotFound { wanted: wanted.to_string(), available: names.to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_channel_voice_messages() {
        assert_eq!(MidiMsg::parse(&[0x90, 36, 100]), Some(MidiMsg::NoteOn { channel: 1, note: 36, velocity: 100 }));
        assert_eq!(MidiMsg::parse(&[0x9F, 36, 0]), Some(MidiMsg::NoteOff { channel: 16, note: 36, velocity: 0 }));
        assert_eq!(
            MidiMsg::parse(&[0xB2, 7, 64]),
            Some(MidiMsg::ControlChange { channel: 3, controller: 7, value: 64 })
        );
        assert_eq!(MidiMsg::parse(&[0xC0, 5]), Some(MidiMsg::ProgramChange { channel: 1, program: 5 }));
        assert_eq!(MidiMsg::parse(&[0xE0, 0, 64]), Some(MidiMsg::PitchBend { channel: 1, value: 8192 }));
    }

    #[test]
    fn rejects_short_realtime_and_sysex() {
        // These used to panic the callback by indexing message[1]/[2].
        assert_eq!(MidiMsg::parse(&[]), None);
        assert_eq!(MidiMsg::parse(&[0xF8]), None); // clock
        assert_eq!(MidiMsg::parse(&[0xFE]), None); // active sensing
        assert_eq!(MidiMsg::parse(&[0x90, 36]), None); // truncated
        assert_eq!(MidiMsg::parse(&[0xF0, 0x7E, 0xF7]), None); // sysex
        assert_eq!(MidiMsg::parse(&[0x40, 0x40]), None); // running status data
        assert_eq!(MidiMsg::parse(&[0x90, 0x80, 1]), None); // bad data byte
    }

    #[test]
    fn hex_roundtrip_and_sysex_validation() {
        let bytes = parse_hex("F0 00 20 6b 7F 42 02 00 10 70 01 F7").unwrap();
        assert_eq!(hex(&bytes), "F0 00 20 6B 7F 42 02 00 10 70 01 F7");
        assert!(validate_sysex(&bytes).is_ok());
        assert!(parse_hex("0xF0,0xF7").is_ok());
        assert!(parse_hex("zz").is_err());
        assert!(validate_sysex(&[0x90, 1, 2]).is_err());
        assert!(validate_sysex(&[0xF0, 0x80, 0xF7]).is_err());
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn port_selection_alsa_names() {
        let ports = names(&["Midi Through:Midi Through Port-0 14:0", "Arturia BeatStep:Arturia BeatStep MIDI 1 20:0"]);
        assert_eq!(select_port(&ports, "Arturia BeatStep"), Ok(1));
        assert_eq!(select_port(&ports, "arturiabeatstep"), Ok(1));
        assert_eq!(select_port(&ports, "Arturia BeatStep:Arturia BeatStep MIDI 1 20:0"), Ok(1));
        assert_eq!(select_port(&ports, "beatstep"), Ok(1));
        assert!(matches!(select_port(&ports, "launchpad"), Err(PortMatchError::NotFound { .. })));
    }

    #[test]
    fn port_selection_coremidi_names() {
        let ports = names(&["IAC Driver Bus 1", "Arturia BeatStep"]);
        assert_eq!(select_port(&ports, "Arturia BeatStep"), Ok(1));
        assert_eq!(select_port(&ports, "ARTURIA beatstep"), Ok(1));
    }

    #[test]
    fn port_selection_ambiguity_is_an_error() {
        let ports = names(&["Synth:Synth MIDI 1 24:0", "Synth:Synth MIDI 2 24:1"]);
        assert!(matches!(select_port(&ports, "Synth"), Err(PortMatchError::Ambiguous { .. })));
        assert_eq!(select_port(&ports, "Synth MIDI 2"), Ok(1));
        // An exact full name always wins over looser rules.
        let ports = names(&["Pad", "Pad Pro"]);
        assert_eq!(select_port(&ports, "Pad"), Ok(0));
    }
}

# Changelog

## 0.5.0 (unreleased)

A rewrite of the runtime. **The config format changed**; see the
[migration guide](docs/migration-0.5.md).

### Breaking

- Config files need `"config_version": 2`. The top-level `config` array is now
  `devices`, each device's `device` field is now `name`, `log_level` values are
  lowercase, and every control needs a `type` (`note` or `cc`).
- The `thresholds` blocks are gone. Encoders use `rate_limit_ms` and
  `deadband`; switches and triggers use `debounce_ms`.
- Encoder mapping moved from the command to the encoder, and `replace` tokens
  became `{value}` and `{delta}` placeholders.
- The default config location is now `~/.config/midiboard/config.json`.
- The CLI uses subcommands: `devices`, `listen`, `config generate`,
  `config validate`, `run`, `replay`, `send`, `led`, `beatstep`.
- The systemd system unit was replaced by a user unit, and a launchd agent was
  added for macOS.

### Added

- Relative encoder modes, acceleration, wrap-around, deadband for noisy pots,
  and decimal precision.
- LED and MIDI feedback: `led` (device profile), `note`, `cc` and raw `sysex`
  targets, trigger flashes, "lit while running", and error colors.
- Arturia BeatStep profile for pad and button LEDs, with optional pad takeover
  and a note on/off mode (`led_mode`) for units where SysEx doesn't work.
- `beatstep setup` switches BeatStep encoders to relative (or absolute) mode,
  and sets knob acceleration and pad gate mode; `beatstep show` prints the
  current setup in config terms.
- External state sync: switches follow a state command, and encoders start from
  and follow the system value.
- `channel` matching, `shell` mode with safely quoted placeholders, and command
  timeouts.
- `listen --record` and `replay` to measure encoder behavior on real traces.
- `send` and `led` for testing feedback by hand.
- Reconnects automatically when a device is unplugged and plugged back in.
- macOS (Apple Silicon) support, CI on Linux and macOS, and release binaries.
- A JSON schema covering the whole format, checked against the examples in tests.

### Fixed

- Only the first configured device ran; now all devices run at once.
- Each device thread kept a CPU core at 100%.
- Triggers skipped the first press and then every other press.
- A switch's first press always ran the `off` command.
- Clock, active sensing and other short MIDI messages crashed the listener.
- Note, CC and aftertouch messages with the same number, on any channel,
  triggered the same control.
- Encoder mapping ignored `map_min`.
- Encoders lagged by up to the activation window, moved in coarse steps, and
  lost the final value when you stopped turning.
- A slow command blocked all MIDI input. Commands that start something in the
  background (`app &`, `xdg-open`) don't keep the control busy, and a timeout
  stops the command's whole process group.
- `devices --input` leaked memory on every message.
- Non-UTF-8 command output was dropped with an error.
- Invalid configs (missing mapping, duplicate keys) only failed at runtime;
  `config validate` now reports every problem up front.
- `run` with no MIDI backend or a crashed device exited with status 0.

## 0.4.0

Released from the 2023 state of `master`, without further changes.

- **Breaking:** `Encoder` commands use a single `execute` command with
  `replace`, `map_min` and `map_max` to insert the knob value, instead of
  separate `increase`/`decrease` commands.

## 0.3.3

The last release on crates.io before the move to `choccydev/midiboard`.

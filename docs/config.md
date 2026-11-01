# Config reference

The config file is JSON. By default it lives at
`$XDG_CONFIG_HOME/midiboard/config.json`, which is usually
`~/.config/midiboard/config.json`, on both Linux and macOS. Every command that
reads it takes `--path` to use another file.

- `midiboard config generate` writes a starter file.
- `midiboard config validate` checks the file and lists **every** problem with
  where it is, e.g. `devices[0].controls.mute.feedback: ...`. A file that
  validates won't fail later at runtime because of its contents.
- Add `"$schema"` (as in the examples) to get completion and inline checks in
  editors that support JSON Schema, such as VS Code. The schema is
  [`schema/midiboard.schema.json`](../schema/midiboard.schema.json).

Unknown fields are errors, so typos don't go unnoticed.

Complete, working examples: [`examples/beatstep-linux.json`](../examples/beatstep-linux.json)
and [`examples/beatstep-macos.json`](../examples/beatstep-macos.json).

## Top level

```json
{
    "$schema": "https://raw.githubusercontent.com/choccydev/midiboard/master/schema/midiboard.schema.json",
    "config_version": 2,
    "log_level": "warn",
    "devices": [ ... ]
}
```

| Field | Required | Default | Meaning |
|---|---|---|---|
| `config_version` | yes | | Must be `2`. Files without it are from midiboard 0.4 or older; see the [migration guide](migration-0.5.md). |
| `log_level` | no | `warn` | `error`, `warn`, `info`, `debug` or `trace`. Applies to `midiboard run`. The `RUST_LOG` environment variable overrides it. |
| `devices` | yes | | One entry per controller. All of them run at the same time. |

## Devices

```json
{
    "name": "Arturia BeatStep",
    "output": { "profile": "beatstep", "take_over_pads": true },
    "controls": { ... }
}
```

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | The input port to listen to. Run `midiboard devices` to see the names. |
| `output` | no | Where feedback goes. See below. |
| `controls` | yes | Controls by name. Names start with `a-z` and contain only `a-z`, `0-9` and `_`. |

`name` is matched against the port list with these rules, in order. The first
rule that matches anything wins, and if it matches more than one port that's an
error that lists them:

1. the exact full port name, e.g. `Arturia BeatStep:Arturia BeatStep MIDI 1 20:0` on Linux;
2. the client name, the part before `:`, ignoring case and spaces, e.g. `arturia beatstep`;
3. the full name ignoring case and spaces;
4. a unique part of the name, e.g. `beatstep`.

On macOS ports have plain names like `Arturia BeatStep`, so rule 1 or 2 matches.

If the device is unplugged, midiboard logs it and reconnects when it comes back,
then repaints all LEDs.

### `output`

| Field | Default | Meaning |
|---|---|---|
| `port` | the device `name` | Output port for feedback. Same matching rules as `name`. |
| `profile` | none | Device profile, needed for `led` feedback. Currently `beatstep`. |
| `led_mode` | `sysex` | How `led` feedback is sent: `sysex` (profile messages, with colors) or `note` (note on/off to the control's own note, no colors). See [feedback](feedback.md#arturia-beatstep). |
| `take_over_pads` | `false` | BeatStep only. On connect, puts every pad that has `led` feedback in gate mode, so the BeatStep stops lighting those pads by itself. This changes the device's current settings until you power-cycle it or recall a preset. |

You can leave `output` out: if any control has feedback, the output port with
the same name as the input is opened automatically.

## Controls

```json
"mute": {
    "key": 36,
    "type": "note",
    "channel": 1,
    "command": { "kind": "Switch", ... },
    "feedback": { ... },
    "state": { ... }
}
```

| Field | Required | Meaning |
|---|---|---|
| `key` | yes | Note or CC number, 0-127. |
| `type` | yes | `note` or `cc`. Encoders must be `cc`. |
| `channel` | no | MIDI channel 1-16. Omit it to accept any channel. |
| `command` | yes | What to run: an `Encoder`, `Switch` or `Trigger`. See below. |
| `feedback` | no | What to send back to the device. See [feedback](feedback.md). |
| `state` | no | A command that reports the real state. See [feedback](feedback.md#external-state-sync). |

`midiboard listen <device>` prints exactly these values for every message:

```text
type: note  channel:  1  key:  36  value: 100  (note on)
type: cc    channel:  1  key:   7  value:  65
```

Two controls on the same device can't bind the same input. A control without a
`channel` overlaps every channel.

## Commands

All three kinds run commands described by the same object:

```json
{ "cmd": "pactl", "args": ["set-sink-volume", "@DEFAULT_SINK@", "{value}%"] }
```

| Field | Default | Meaning |
|---|---|---|
| `cmd` | | The program to run. With `shell: true`, the whole command line. |
| `args` | `[]` | Arguments, passed as-is (no shell, no quoting issues). |
| `shell` | `false` | Run `cmd` with `sh -c`, for pipes, `&&`, `$(...)`, `~` and variables. `args` must then be empty. |
| `timeout_ms` | none | Kill the command if it takes longer than this. |

Placeholders in `cmd` and `args` are replaced before running. With
`shell: true` the replacement is single-quoted, so a value can never inject
shell syntax. Don't put placeholders inside quotes yourself in shell mode:
`notify-send "Volume {value}"` would print the quotes. Write
`notify-send Volume\ {value}` or `notify-send "Volume "{value}` instead.

Commands have no timeout unless you set `timeout_ms`. A command that hangs
(for example `osascript` waiting on a permission prompt) keeps its control busy
until it exits, so set a timeout on anything that might hang. Commands that
start something in the background (`app &`, `xdg-open`) are fine: midiboard
stops waiting shortly after the command itself exits.

| Placeholder | Available in | Value |
|---|---|---|
| `{value}` | Encoder | The mapped value, e.g. `42`. |
| `{delta}` | Encoder | The signed change since the last run, e.g. `+3` or `-1`. |
| `{velocity}` | Trigger | The press velocity, 1-127. |

Commands run in the background, so a slow command never delays MIDI input. A
control has at most one command running at a time. What happens to presses and
turns while it runs is described per kind in [controls](controls.md). Command
output is logged at `info` level, and stderr at `warn`.

### `Encoder`

For knobs and faders. Details and tuning advice are in [encoders](encoders.md).

```json
{
    "kind": "Encoder",
    "mode": "relative_offset64",
    "map_min": 0,
    "map_max": 100,
    "step": 1,
    "acceleration": { "curve": 1.5, "max_gain": 4, "slow_ms": 80 },
    "wrap": false,
    "rate_limit_ms": 30,
    "deadband": 0,
    "initial": 50,
    "precision": 0,
    "execute": { "cmd": "pactl", "args": ["set-sink-volume", "@DEFAULT_SINK@", "{value}%"] }
}
```

| Field | Default | Meaning |
|---|---|---|
| `mode` | `absolute` | `absolute`, `relative_offset64`, `relative_twos`, `relative_signbit` or `relative_offset16`. |
| `map_min`, `map_max` | `0`, `127` | The range of `{value}`. |
| `step` | `1` | Relative modes: how far one encoder step moves the value. |
| `acceleration` | none | Relative modes: move further when turning fast. |
| `wrap` | `false` | Relative modes: wrap around at the ends instead of stopping. |
| `rate_limit_ms` | `30` | Minimum time between two runs. |
| `deadband` | `0` | Absolute mode: ignore reversals of up to this many raw steps (noisy pots). |
| `initial` | `map_min` | Starting value. A `state` source overrides it. |
| `precision` | `0` | Decimals in `{value}` and `{delta}`. |
| `execute` | | The command. |

### `Switch`

For pads and buttons that toggle something on and off.

```json
{
    "kind": "Switch",
    "on": { "cmd": "pactl", "args": ["set-sink-mute", "@DEFAULT_SINK@", "1"] },
    "off": { "cmd": "pactl", "args": ["set-sink-mute", "@DEFAULT_SINK@", "0"] },
    "initial_state": "off",
    "debounce_ms": 50
}
```

| Field | Default | Meaning |
|---|---|---|
| `on`, `off` | | Commands for switching on and off. |
| `initial_state` | | `on` or `off`. Required unless the control has a `state` source, which then decides. |
| `debounce_ms` | `50` | Presses closer together than this are ignored. |

### `Trigger`

For pads and buttons that do the same thing on every press.

```json
{
    "kind": "Trigger",
    "execute": { "cmd": "playerctl", "args": ["play-pause"] },
    "debounce_ms": 50
}
```

| Field | Default | Meaning |
|---|---|---|
| `execute` | | The command. |
| `debounce_ms` | `50` | Presses closer together than this are ignored. |

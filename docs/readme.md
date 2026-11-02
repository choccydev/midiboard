# Midiboard docs

midiboard turns a MIDI controller into a control surface for your computer: it
listens to the controller, runs commands when you press pads or turn knobs, and
can light the controller's LEDs to show what's going on.

- [Getting started](#getting-started), below: from zero to a working config.
- [Config reference](config.md): every field.
- [How controls behave](controls.md): triggers, switches and encoders.
- [Encoders](encoders.md): modes, tuning, and measuring with record/replay.
- [Feedback and state](feedback.md): LEDs, the Arturia BeatStep profile, and
  following changes made outside midiboard.
- [CLI reference](cli.md): every command and option.
- [Migrating to 0.5](migration-0.5.md): updating a config from 0.3 or 0.4.

## Getting started

### 1. Install

See the [README](../README.md#install). midiboard runs on Linux (ALSA) and
macOS (CoreMIDI).

### 2. Find your controller

Plug it in and list the MIDI ports:

```console
$ midiboard devices
Inputs (use as a device `name`):
  Midi Through:Midi Through Port-0 14:0
  Arturia BeatStep:Arturia BeatStep MIDI 1 20:0

Outputs (for feedback):
  Midi Through:Midi Through Port-0 14:0
  Arturia BeatStep:Arturia BeatStep MIDI 1 20:0
```

On macOS the names are plain, like `Arturia BeatStep`. In the config you can use
the full name, the part before `:` (`Arturia BeatStep`), or any unique part of
it (`beatstep`).

### 3. Find the controls

Watch what each pad and knob sends:

```console
$ midiboard listen beatstep
Listening to "beatstep". Press Enter or Ctrl-C to stop.
type: note  channel:  1  key:  36  value: 100  (note on)
type: note  channel:  1  key:  36  value:   0  (note off)
type: cc    channel:  1  key:   7  value:  65
type: cc    channel:  1  key:   7  value:  65
type: cc    channel:  1  key:   7  value:  63
```

Here the first pad is `type: note`, `key: 36`, and the big knob is `type: cc`,
`key: 7`. That knob sends 65 when turned right and 63 when turned left, so it's
an endless encoder in `relative_offset64` mode (see [encoders](encoders.md#relative-modes)).

### 4. Write the config

Create a starter file and open it:

```bash
midiboard config generate     # writes ~/.config/midiboard/config.json
```

```json
{
    "$schema": "https://raw.githubusercontent.com/choccydev/midiboard/master/schema/midiboard.schema.json",
    "config_version": 2,
    "log_level": "info",
    "devices": [
        {
            "name": "Arturia BeatStep",
            "controls": {
                "volume": {
                    "key": 7,
                    "type": "cc",
                    "command": {
                        "kind": "Encoder",
                        "mode": "relative_offset64",
                        "map_min": 0,
                        "map_max": 100,
                        "execute": { "cmd": "pactl", "args": ["set-sink-volume", "@DEFAULT_SINK@", "{value}%"] }
                    }
                },
                "mute": {
                    "key": 36,
                    "type": "note",
                    "command": {
                        "kind": "Switch",
                        "initial_state": "off",
                        "on": { "cmd": "pactl", "args": ["set-sink-mute", "@DEFAULT_SINK@", "1"] },
                        "off": { "cmd": "pactl", "args": ["set-sink-mute", "@DEFAULT_SINK@", "0"] }
                    }
                }
            }
        }
    ]
}
```

There are three kinds of control:

- **Encoder** for knobs and faders: runs a command with the knob's value.
- **Switch** for pads that toggle something on and off.
- **Trigger** for pads that do the same thing on every press.

Then check it:

```console
$ midiboard config validate
/home/you/.config/midiboard/config.json is valid: 1 device(s), 2 control(s), log level `info`.
```

If something's wrong, `validate` lists every problem and where it is.

### 5. Run it

```bash
midiboard run
```

Turn the knob and press the pad. With `log_level` set to `info`, you'll see
each command and its output. Stop with Ctrl-C. To start midiboard
automatically, [run it as a service](../README.md#run-as-a-service).

### 6. Light it up (optional)

To make the mute pad show whether you're muted, add `feedback`, and a `state`
source so it also follows mute changes made elsewhere:

```json
"mute": {
    "key": 36,
    "type": "note",
    "command": { "kind": "Switch", "on": {...}, "off": {...} },
    "state": { "cmd": "pactl", "args": ["get-sink-mute", "@DEFAULT_SINK@"], "match": "yes", "poll_ms": 2000 },
    "feedback": { "led": { "pad": 1 }, "on": "red", "off": "off" }
}
```

`led` needs a device profile; for the BeatStep add
`"output": { "profile": "beatstep" }` to the device. Other controllers usually
work with `note` feedback. All of this is in [feedback](feedback.md).

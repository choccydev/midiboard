# Feedback and state

midiboard can send MIDI back to your controller, to light pads, set LED rings
or move motorized faders, and it can follow changes made outside midiboard, so
the lights always show the real state.

## Feedback

Add a `feedback` object to a control. It needs exactly one **target**: `led`,
`note`, `cc` or `sysex`.

```json
"mute": {
    "key": 36,
    "type": "note",
    "command": { "kind": "Switch", "initial_state": "off", "on": {...}, "off": {...} },
    "feedback": { "led": { "pad": 1 }, "on": "red", "off": "off", "error": "magenta" }
}
```

| Field | Meaning |
|---|---|
| `led` | An LED addressed through the device profile (`output.profile`), e.g. `{ "pad": 1 }`. `on`/`off`/`error` are color names. |
| `note` | `{ "number": 36, "channel": 1 }`. Sends note on with velocity `on` (default 127) for lit, and note off for unlit (or note on with velocity `off`, if it isn't 0). Most controllers light pads this way. |
| `cc` | `{ "number": 20, "channel": 1 }`. For switches and triggers, sends value `on`/`off` (defaults 127/0). For encoders, echoes the current value scaled to 0-127; `on`/`off` don't apply. |
| `sysex` | `{ "on": "F0 ... F7", "off": "F0 ... F7", "error": "F0 ... F7" }`: raw messages in hex, for anything else. |
| `on`, `off`, `error` | What to send for the lit, unlit and failed states. `error` is optional. |
| `flash_ms` | Triggers: light up for this long on each press. |
| `while_running` | Triggers: stay lit while the command runs. |

The output port is the device `name` unless `output.port` says otherwise, and it
is opened automatically when any control has feedback.

### What is shown when

| Control | Startup / reconnect | During use | Command failed |
|---|---|---|---|
| Switch | its state (initial or read from `state`) | its state, after each command finishes | `error` for 0.5 s if set, then the real (unchanged) state |
| Trigger | off | on for `flash_ms`, or while running | `error` if set, for 0.5 s (or `flash_ms` if longer), then off |
| Encoder (`cc`) | current value | value after each run and state read | |

A switch LED changes only once the command has succeeded, so it never shows a
state the system isn't in. On Ctrl-C or SIGTERM, every LED is turned off before
midiboard exits.

## Arturia BeatStep

Set `"profile": "beatstep"` on the device's `output` to use `led` targets:

```json
{
    "name": "Arturia BeatStep",
    "output": { "profile": "beatstep", "take_over_pads": true },
    "controls": { ... }
}
```

- **Pads**: `{ "pad": 1 }` to `{ "pad": 16 }`, in the BeatStep's own numbering
  (pad 1 is control `0x70`). Check which physical pad is which with
  `midiboard led` (below).
- **Buttons**: `{ "button": "play" }`, and also `seq`, `sync`, `recall`, `store`,
  `shift` and `chan`. Stop has no LED. Some buttons only show some colors.
- **Colors**: `off`, `red`, `blue`, and `magenta` (alias `pink`). The original
  BeatStep has red and blue LEDs, and magenta is both lit.

`led` targets are addressed by pad number, independent of the note each pad
sends, so changing a pad's note in MIDI Control Center doesn't break feedback.
The `key` of the control still has to match what the pad sends; check it with
`midiboard listen`.

### Setting up the BeatStep

The BeatStep needs to be in **Control** mode (not Sequencer) for its pads and
knobs to send the messages you bind.

**Pad behavior.** In MIDI Control Center each pad is either *Toggle* or *Gate*.
In Toggle mode the BeatStep lights pads by itself and fights midiboard's LEDs.
Either set the pads you use for feedback to Gate in MIDI Control Center, or set
`"take_over_pads": true` and midiboard does it on every connect, for pads that
have `led` feedback. That change lasts until the device is power-cycled or a
preset is recalled.

**Encoders.** The 16 knobs and the big volume knob are endless. In MIDI Control
Center, set the ones you use with midiboard to **Relative #1** and use
`"mode": "relative_offset64"`. Absolute mode works too, but the knobs then stop
at 0 and 127.

**Check the LEDs by hand** before writing the config:

```bash
midiboard led "Arturia BeatStep" --pad 1 --color red
midiboard led "Arturia BeatStep" --button play --color blue
midiboard led "Arturia BeatStep" --pad 1 --color off
```

### About the protocol

Arturia doesn't publish the BeatStep SysEx protocol. midiboard uses what the
community has reverse-engineered:

```text
set parameter: F0 00 20 6B 7F 42 02 00 <param> <control> <value> F7
LED color:     param 0x10, pads 1-16 = controls 0x70-0x7F, buttons 0x58-0x5F
pad behavior:  param 0x06, value 0x00 toggle, 0x01 gate
```

Sources: [konsumer/beatstep](https://github.com/konsumer/beatstep) and the
[discussion in its issue #4](https://github.com/konsumer/beatstep/issues/4).
If something doesn't light up, `midiboard send` lets you try raw messages:

```bash
midiboard send "Arturia BeatStep" "F0 00 20 6B 7F 42 02 00 10 70 01 F7"
```

If SysEx doesn't work on your unit, use the `note` target instead: in Control
mode, the BeatStep lights a pad when it receives a note on for that pad's note,
and turns it off on note off.

## Other controllers

Most controllers light pads when they receive the note they send. Try it with
`midiboard send`:

```bash
midiboard send "My Controller" "90 24 7F"   # note on, note 36, velocity 127
midiboard send "My Controller" "80 24 00"   # note off
```

On many controllers (Akai, Novation and others) the velocity selects the color.
Use `note` feedback with the velocities you found:

```json
"feedback": { "note": { "number": 36, "channel": 1 }, "on": 5, "off": 0 }
```

For controllers with LED rings or motorized faders, `cc` feedback on an encoder
keeps the hardware in sync with the value, including changes picked up by
`state`.

## External state sync

A `state` source is a command that reports the real state. It keeps switches
and encoders, and their LEDs, in line with changes made outside midiboard: you
mute from the desktop, and the mute pad lights up.

```json
"mute": {
    "key": 36,
    "type": "note",
    "command": { "kind": "Switch", "on": {...}, "off": {...} },
    "state": {
        "cmd": "pactl",
        "args": ["get-sink-mute", "@DEFAULT_SINK@"],
        "match": "yes",
        "poll_ms": 2000
    },
    "feedback": { "led": { "pad": 1 }, "on": "red", "off": "off" }
}
```

| Field | Default | Meaning |
|---|---|---|
| `cmd`, `args`, `shell` | | The command, as for any other command. |
| `timeout_ms` | `2000` | Kill it if it takes longer. |
| `match` | | Switch: a regex; on when it matches the output. Encoder: a regex whose first group is the number. |
| `poll_ms` | none | Read again every this many ms (at least 100). |

When the state is read:

- at startup and after every reconnect;
- every `poll_ms`, if set;
- for switches, right after each `on`/`off` command, to confirm it took effect.

How the output is interpreted:

- **Switch with `match`**: on if the regex matches stdout. If the command fails,
  the last known state is kept.
- **Switch without `match`**: on if the command exits successfully, off
  otherwise.
- **Encoder**: the first group of `match`, or the first number in stdout, is the
  new value, clamped to the range. It doesn't run the command, and it's ignored
  while you're turning the knob (and briefly after) so it doesn't fight you.

A switch with a `state` source doesn't need `initial_state`. Failures are logged
at most once a minute.

Examples:

| What | Linux | macOS |
|---|---|---|
| Output muted | `pactl get-sink-mute @DEFAULT_SINK@`, match `yes` | `osascript -e "output muted of (get volume settings)"`, match `true` |
| Mic muted | `pactl get-source-mute @DEFAULT_SOURCE@`, match `yes` | |
| Volume | `pactl get-sink-volume @DEFAULT_SINK@`, match `(\d+)%` | `osascript -e "output volume of (get volume settings)"` |
| Service running | `systemctl --user is-active foo` (exit status) | `pgrep -x Foo` (exit status) |

# midiboard

Run commands from any MIDI controller: turn a knob to change the volume, press a
pad to toggle mute, launch scripts from buttons, and light up the controller's
LEDs to show what's going on.

- **Encoders that feel right**: every movement counts, commands run at a
  bounded rate, and the final value always lands. Absolute and relative
  (endless) encoders, acceleration, and jitter filtering for noisy pots.
- **Switches and triggers** for pads and buttons, with debouncing.
- **LED feedback**, including the Arturia BeatStep's pad and button LEDs, and
  generic note, CC and SysEx output for other controllers.
- **State sync**: mute from the desktop and the mute pad lights up; relative
  knobs continue from the current volume.
- **Robust**: several devices at once, reconnects after unplugging, commands
  never block MIDI input, and every config error is reported up front.
- Linux (ALSA) and macOS (CoreMIDI).

```json
"mute": {
    "key": 36,
    "type": "note",
    "command": {
        "kind": "Switch",
        "on": { "cmd": "pactl", "args": ["set-sink-mute", "@DEFAULT_SINK@", "1"] },
        "off": { "cmd": "pactl", "args": ["set-sink-mute", "@DEFAULT_SINK@", "0"] }
    },
    "state": { "cmd": "pactl", "args": ["get-sink-mute", "@DEFAULT_SINK@"], "match": "yes", "poll_ms": 2000 },
    "feedback": { "led": { "pad": 1 }, "on": "red", "off": "off" }
}
```

## Install

### Cargo

```bash
cargo install midiboard
```

On Linux this needs the ALSA headers: `libasound2-dev` (Debian/Ubuntu),
`alsa-lib-devel` (Fedora) or `alsa-lib` (Arch). macOS needs nothing extra.

### Prebuilt binaries

Download the archive for your platform (Linux x86_64 or macOS Apple Silicon)
from the [releases page](https://github.com/choccydev/midiboard/releases) and
put `midiboard` somewhere on your `PATH`.

### From source

```bash
git clone https://github.com/choccydev/midiboard.git
cd midiboard
cargo install --path .
```

## Quick start

```bash
midiboard devices              # find your controller's name
midiboard listen beatstep      # see what each pad and knob sends
midiboard config generate      # write ~/.config/midiboard/config.json
$EDITOR ~/.config/midiboard/config.json
midiboard config validate      # check it
midiboard run                  # go
```

The [getting started guide](docs/readme.md#getting-started) walks through it,
and [`examples/`](examples) has complete configs for an Arturia BeatStep on
Linux and macOS.

## Run as a service

### Linux (systemd user unit)

midiboard runs as a **user** service, so commands like `pactl`, `wpctl` and
`playerctl` can reach your audio server and session bus.

```bash
mkdir -p ~/.config/systemd/user
curl -o ~/.config/systemd/user/midiboard.service \
  https://raw.githubusercontent.com/choccydev/midiboard/master/dist/midiboard.service
# Edit ExecStart if midiboard isn't in ~/.cargo/bin.
systemctl --user daemon-reload
systemctl --user enable --now midiboard
journalctl --user -u midiboard -f    # logs
```

### macOS (launchd agent)

```bash
curl -o /tmp/midiboard.plist \
  https://raw.githubusercontent.com/choccydev/midiboard/master/dist/dev.choccymilk.midiboard.plist
# Edit the program path inside if midiboard isn't in ~/.cargo/bin.
sed "s|@HOME@|$HOME|g" /tmp/midiboard.plist > ~/Library/LaunchAgents/dev.choccymilk.midiboard.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.choccymilk.midiboard.plist
tail -f ~/Library/Logs/midiboard.log   # logs
```

Both restart midiboard if it exits with an error.

## Documentation

- [Getting started](docs/readme.md)
- [Config reference](docs/config.md)
- [How controls behave](docs/controls.md)
- [Encoders](docs/encoders.md)
- [Feedback and state](docs/feedback.md)
- [CLI reference](docs/cli.md)
- [Migrating from 0.3/0.4](docs/migration-0.5.md)
- [Changelog](CHANGELOG.md)

## License

MIT

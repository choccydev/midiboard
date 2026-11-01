# CLI reference

<!-- Generated from the clap definitions by `UPDATE_DOCS=1 cargo test cli_docs`. Don't edit by hand. -->

## midiboard

```text
Run commands from any MIDI 1.0 controller: change the volume with a knob, toggle mute with a pad,
launch scripts with a button, and light up the controller's LEDs to show what's going on.

The config file lives at $XDG_CONFIG_HOME/midiboard/config.json (~/.config/midiboard/config.json)
unless --path is given.

Logging defaults to the config's `log_level` for `run` and to `info` otherwise; set RUST_LOG (e.g.
RUST_LOG=debug) to override.

Usage: midiboard <COMMAND>

Commands:
  run      Run the service: listen to the configured devices and run commands
  devices  List MIDI input and output ports [alias: list]
  listen   Print every message a device sends, decoded as `type`, `channel` and `key`
  config   Create or check the config file
  replay   Run a recorded trace through the engine and print what would execute
  send     Send raw MIDI bytes to an output port, e.g. "90 24 7F" or a SysEx
  led      Set one LED on an Arturia BeatStep
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## midiboard run

```text
Run the service: listen to the configured devices and run commands

Usage: midiboard run [OPTIONS]

Options:
  -p, --path <FILE>
          Config file to use instead of the default location

  -h, --help
          Print help
```

## midiboard devices

```text
List MIDI input and output ports

Usage: midiboard devices

Options:
  -h, --help
          Print help
```

## midiboard listen

```text
Print every message a device sends, decoded as `type`, `channel` and `key`.

Use it to find the values to put in the config. With --record, also saves a trace that `midiboard
replay` can run through the engine.

Usage: midiboard listen [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>
          Input port (full name, client name, or a unique part of it)

Options:
  -r, --record <FILE>
          Save a trace (`t_us,bytes` per line) to FILE

  -h, --help
          Print help (see a summary with '-h')
```

## midiboard config

```text
Create or check the config file

Usage: midiboard config <COMMAND>

Commands:
  generate  Write a starter config file [alias: new]
  validate  Check a config file and report every problem found [alias: check]
  path      Print the default config location
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

## midiboard config generate

```text
Write a starter config file

Usage: midiboard config generate [OPTIONS]

Options:
  -p, --path <FILE>
          Config file to use instead of the default location

      --force
          Overwrite an existing file

  -h, --help
          Print help
```

## midiboard config validate

```text
Check a config file and report every problem found

Usage: midiboard config validate [OPTIONS]

Options:
  -p, --path <FILE>
          Config file to use instead of the default location

  -h, --help
          Print help
```

## midiboard config path

```text
Print the default config location

Usage: midiboard config path

Options:
  -h, --help
          Print help
```

## midiboard replay

```text
Run a recorded trace through the engine and print what would execute.

Output is CSV: `t_us,event,raw,position,value`, where `event` is `input` (a message for the control)
or `exec` (a command run). Plot position and value against time to see latency and smoothing.

Usage: midiboard replay [OPTIONS] --control <CONTROL> <TRACE>

Arguments:
  <TRACE>
          Trace recorded with `midiboard listen --record`

Options:
  -c, --control <CONTROL>
          Control to report on

      --exec-ms <EXEC_MS>
          Simulated time each command takes to finish, in milliseconds
          
          [default: 5]

  -o, --out <FILE>
          Write the CSV to FILE instead of stdout

  -p, --path <FILE>
          Config file to use instead of the default location

  -h, --help
          Print help (see a summary with '-h')
```

## midiboard send

```text
Send raw MIDI bytes to an output port, e.g. "90 24 7F" or a SysEx

Usage: midiboard send <DEVICE> <BYTES>

Arguments:
  <DEVICE>
          Output port (full name, client name, or a unique part of it)

  <BYTES>
          Hex bytes, separated by spaces

Options:
  -h, --help
          Print help
```

## midiboard led

```text
Set one LED on an Arturia BeatStep

Usage: midiboard led [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>
          Output port of the BeatStep

Options:
      --pad <PAD>
          Pad number, 1-16

      --button <BUTTON>
          Function button: play, seq, sync, recall, store, shift or chan

      --color <COLOR>
          off, red, blue or magenta
          
          [default: red]

  -h, --help
          Print help
```

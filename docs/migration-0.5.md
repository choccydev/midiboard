# Migrating to 0.5

midiboard 0.5 changes the config format, and older configs are rejected with a
pointer to this page. There's no automatic converter, but the changes are
mechanical. This page shows each old construct next to its replacement.

To stay on the old format for now, install an older version, e.g.
`cargo install midiboard --version 0.3.3`, or build the `v0.4.0` tag.

## File location

The default location moved from `~/midiboard.json` to
`~/.config/midiboard/config.json` (or `$XDG_CONFIG_HOME/midiboard/config.json`),
on Linux and macOS alike.

```bash
mkdir -p ~/.config/midiboard
mv ~/midiboard.json ~/.config/midiboard/config.json   # then edit it as below
```

`--path` works as before.

## Top level

| 0.3 / 0.4 | 0.5 |
|---|---|
| (none) | `"config_version": 2`, **required** |
| `"log_level": "Warn"` | `"log_level": "warn"` (lowercase) |
| `"config": [ ... ]` | `"devices": [ ... ]` |
| `"$schema": ".../aordano/control-board/..."` | `"$schema": "https://raw.githubusercontent.com/choccydev/midiboard/master/schema/midiboard.schema.json"` |

## Devices

| 0.3 / 0.4 | 0.5 |
|---|---|
| `"device": "Arturia BeatStep"` | `"name": "Arturia BeatStep"` |
| `"thresholds": { ... }` | removed; see below |

The thresholds are gone because the event handling no longer works with time
windows:

| Old threshold | Replacement |
|---|---|
| `thresholds.encoder.activation` | per-control `rate_limit_ms` on the Encoder (default 30). Start with about a tenth of your old value. |
| `thresholds.encoder.detection` | nothing; every message is used. For noisy pots use `deadband`. |
| `thresholds.switch.activation`, `thresholds.trigger.activation` | per-control `debounce_ms` (default 50). |
| per-control `"threshold": { ... }` overrides | per-control `rate_limit_ms` / `debounce_ms`. |

## Controls

Every control needs a `type`. Run `midiboard listen <device>` and copy the
`type` (and optionally `channel`) shown for it.

```diff
 "pause": {
     "key": 45,
+    "type": "note",
     "command": { ... }
 }
```

Old configs matched on the number alone, across note and CC messages and all
channels. Now `type` must match, and `channel` (optional) narrows it further.

### Encoder

0.4 used `replace` with a token, plus `map_min`/`map_max`:

```json
"command": {
    "kind": "Encoder",
    "execute": {
        "cmd": "pactl",
        "args": ["set-sink-volume", "@DEFAULT_SINK@", "@VOLUME_PERCENT@%"],
        "replace": "@VOLUME_PERCENT@",
        "map_max": 101,
        "map_min": 0
    }
}
```

0.5 moves the mapping up to the encoder and uses `{value}`:

```json
"command": {
    "kind": "Encoder",
    "map_min": 0,
    "map_max": 100,
    "execute": {
        "cmd": "pactl",
        "args": ["set-sink-volume", "@DEFAULT_SINK@", "{value}%"]
    }
}
```

Note: 0.4 ignored `map_min` (it computed `(max - min) * raw / 127`), which is
why `map_max: 101` was sometimes needed. 0.5 maps `raw = 127` exactly to
`map_max`.

0.3 had separate `increase`/`decrease` commands. Use `{delta}` with a tool that
accepts relative values:

```json
"execute": { "cmd": "pactl", "args": ["set-sink-volume", "@DEFAULT_SINK@", "{delta}%"] }
```

### Switch

```diff
 "command": {
     "kind": "Switch",
     "on": { ... },
     "off": { ... },
-    "initial_state": "OFF"
+    "initial_state": "off"
 }
```

Uppercase `"ON"`/`"OFF"` are still accepted. Behavior changes:

- The first press now goes to the opposite of `initial_state`. 0.3/0.4 always ran
  `off` first.
- If the command fails, the state no longer flips.

### Trigger

No config change besides `type`. Triggers now fire on every press; 0.3/0.4
skipped the first press and then every other one.

### Commands

`cmd` and `args` are the same. `args` is now optional, and commands gained
`shell` and `timeout_ms`. `replace`, `map_min` and `map_max` are no longer
allowed inside a command.

## Service

The systemd **system** unit was replaced by a **user** unit, so commands like
`pactl` and `playerctl` can reach your session. Remove the old one:

```bash
sudo systemctl disable --now midiboard
sudo rm /etc/systemd/system/midiboard.service
sudo systemctl daemon-reload
```

and install the new one as described in the [README](../README.md#run-as-a-service).

## CLI

| 0.3 / 0.4 | 0.5 |
|---|---|
| `midiboard devices --list` | `midiboard devices` |
| `midiboard devices --input <DEVICE>` | `midiboard listen <DEVICE>` |
| `midiboard config --generate` | `midiboard config generate` |
| `midiboard config --validate` | `midiboard config validate` |
| `midiboard run` | `midiboard run` |

## A full example

Before (0.4):

```json
{
    "log_level": "Warn",
    "config": [
        {
            "device": "Arturia BeatStep",
            "thresholds": {
                "encoder": { "activation": 300, "detection": 40 },
                "switch": { "activation": 400 },
                "trigger": { "activation": 1000 }
            },
            "controls": {
                "volume": {
                    "key": 7,
                    "command": {
                        "kind": "Encoder",
                        "execute": {
                            "cmd": "pactl",
                            "args": ["set-sink-volume", "@DEFAULT_SINK@", "@V@%"],
                            "replace": "@V@", "map_min": 0, "map_max": 100
                        }
                    }
                },
                "pause": {
                    "key": 45,
                    "command": { "kind": "Trigger", "execute": { "cmd": "playerctl", "args": ["play-pause"] } }
                }
            }
        }
    ]
}
```

After (0.5):

```json
{
    "$schema": "https://raw.githubusercontent.com/choccydev/midiboard/master/schema/midiboard.schema.json",
    "config_version": 2,
    "log_level": "warn",
    "devices": [
        {
            "name": "Arturia BeatStep",
            "controls": {
                "volume": {
                    "key": 7,
                    "type": "cc",
                    "command": {
                        "kind": "Encoder",
                        "map_min": 0,
                        "map_max": 100,
                        "execute": { "cmd": "pactl", "args": ["set-sink-volume", "@DEFAULT_SINK@", "{value}%"] }
                    }
                },
                "pause": {
                    "key": 45,
                    "type": "note",
                    "command": { "kind": "Trigger", "execute": { "cmd": "playerctl", "args": ["play-pause"] } }
                }
            }
        }
    ]
}
```

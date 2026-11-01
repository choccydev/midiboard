# Midiboard modernization plan

Status: **decisions resolved (§9), ready to implement**.
Target release: `0.5.0`. This is a breaking config change; old configs are dropped.

## Releases before the rewrite

- **`v0.3.3`** is tagged at `ac0d2da`, the commit crates.io 0.3.3 was published from.
- **`v0.4.0`** is tagged at the version-bump commit on top of `master`'s `739f1df`. It ships the unreleased Encoder change (`increase`/`decrease` → `execute` + `map_min`/`map_max`/`replace`) as-is.
- The tags are pushed. **You still need to:** create the GitHub Release pages from the tags (I don't have a release API here), and run `cargo publish` from `v0.4.0` (it needs your crates.io token).
- From 0.5.0 on, a release workflow (§2) builds binaries and drafts the GitHub Release when a tag is pushed.

This plan covers five goals:

1. Update the toolchain and dependencies.
2. Fix bugs and leaks.
3. Replace the encoder windowing logic.
4. Update the docs.
5. Send MIDI back to devices (feedback), with the Arturia BeatStep (non-Pro) pad LEDs as the target device.

---

## 0. What the audit found

I read every source file and built the project on rustc 1.97, after installing `libasound2-dev`. It builds with 4 warnings.
The list below is ordered by impact. Each item notes where the problem is.

### Critical (the app misbehaves today)

| # | Problem | Where |
|---|---------|-------|
| B1 | **Only the first device in `config[]` ever runs.** `handle.join()` sits inside the spawn loop, and the first thread never returns. | `run.rs:28-55` |
| B2 | **Each device thread spins a CPU core at 100%** with `loop {}`. | `run.rs:83` |
| B3 | **Triggers fire on every other press.** After a trigger fires, its state is reset to `None`. The next press then only re-initializes the state and does not fire. The first press after startup is swallowed the same way. On pads that send note-on and note-off, the press can be eaten by its own release. | `run.rs:151-165`, `on_key_event` |
| B4 | **A switch's first toggle always runs the `off` command**, whatever the `initial_state`. Both branches return `Activation::switch(true, false)`. | `run.rs:500-511` |
| B5 | **The callback panics on short MIDI messages.** It indexes `message[1]` and `message[2]` with no length check, and `Ignore::None` lets clock, active sensing and SysEx through. One 1-byte clock message panics the callback. | `run.rs:112-113`, `devices.rs:370` |
| B6 | **The status byte is ignored.** Note-on, note-off, CC and aftertouch with the same number count as the same control, on every channel. | `run.rs:112` |
| B7 | **`map_min` is ignored in value mapping.** `interpolate` returns `(max-min)*v/127` instead of `min + (max-min)*v/127`. | `util.rs:111-122` |
| B8 | **Commands run synchronously inside the MIDI callback** (`.output()`). A slow command stalls all input, and the timing math sees inflated gaps. | `run.rs:328` |
| B9 | **The systemd unit is a system service that runs `pactl` and `playerctl`.** Those tools need the user session (`XDG_RUNTIME_DIR` and the D-Bus session bus), so they fail under a system service. `ExecStart=/bin/midiboard` also disagrees with the install docs (`/usr/bin`). | `schema/midiboard.service` |

### Leaks and resource issues

The `FIXME` blames midir's `alsa_seq()`. Most of the leaking actually comes from our own code:

| # | Problem | Where |
|---|---------|-------|
| L1 | `string_to_sstr` (`Box::leak`) runs **once per received MIDI message** in `devices --input`. That is an unbounded leak. It is also called on error paths and in clap help strings. | `devices.rs:368,379`, `config.rs:445,451`, `main.rs` |
| L2 | `get_controls_by_key()` clones the whole controls `HashMap` **on every MIDI message**, through `get_threshold`. This is allocation churn rather than a leak, but it shows up in heap profiles. | `types.rs:247-273` |
| L3 | `get_input_port` opens a separate ALSA sequencer client to look up a port. midir can leave clients like this registered if it isn't dropped cleanly, as on the panic path. | `util.rs:44` |
| L4 | What remains on midir/ALSA's side: `valgrind` reports alsa-lib's global config cache (`snd_config_update`) as "still reachable". That is a one-time cost, not growth. midir 0.8 → 0.11 also brings ALSA backend fixes. I'll measure before and after with `valgrind --leak-check=full` and `heaptrack`, using a long synthetic stream. If anything still grows after the fixes, it gets fixed upstream or worked around with a single, long-lived `MidiInput`/`MidiOutput` per device. |

### Correctness and robustness

- B10 `Logger::fatal` uses `assert!(false)`: it panics with a backtrace instead of exiting cleanly. `Logger::change_level` takes `self` by value, so it does nothing.
- B11 `devices` and `config` with no flag call `panic!()` instead of printing usage.
- B12 Duplicate `key`s across controls silently overwrite each other (`get_controls_by_key`).
- B13 Port matching compares only the ALSA client name before `:`, and the last match wins. Two ports on one device (the BeatStep exposes one, but many devices expose several) can't be told apart.
- B14 Nothing handles a disconnect or reconnect. Unplugging the device leaves a dead process that systemd thinks is healthy.
- B15 `from_utf8(stdout)?` drops the log line, and turns the result into an error, when a command prints non-UTF-8 output.
- B16 Semantic config errors (`replace` without `map_min`/`map_max`, `min > max`) only appear at runtime, on the first knob turn, not in `config --validate`.
- B17 Schema: `"exclusiveMaximum": true` isn't valid in draft 2020-12. `$id` and `$schema` point at the old `aordano/control-board` repo. `args` is required in Rust but optional in the schema.
- B18 The version is hard-coded in both `main.rs` and `Cargo.toml`.
- B19 The docs describe an `Encoder` with `increase`/`decrease` fields that no longer exist. The README admits the docs are outdated.

---

## 1. Phase 0: safety net (do first)

1. **CI**: a GitHub Actions workflow on Ubuntu with `libasound2-dev` that runs `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test`.
2. **Characterization tests** for B3, B4 and B7, so each fix lands with a test that failed before it.
3. Make the event engine **free of ALSA and the system clock** (see §3), so it can be tested deterministically with synthetic, timestamped MIDI streams. The container and CI have no `/dev/snd`, so hardware paths are tested manually (§6).

## 2. Phase 1: modernize (goal 1)

| Item | From | To | Notes |
|---|---|---|---|
| Edition | 2021 | **2024**, with `rust-version` set | |
| `midir` | 0.8 | **0.11** | Brings `MidiOutput`, which feedback needs, plus ALSA fixes. |
| `clap` | 4.0 builder API | **4.6, derive API** | Removes the `string_to_sstr` leaks in help text. Version comes from `CARGO_PKG_VERSION`. |
| `config` | 0.13 | **removed → `serde_json`** | We only read one JSON file, so the layered-config crate is unnecessary weight. Errors now carry line and column numbers. |
| `colored` | 2 | **3** | |
| `rust-embed` | 6 | **8** | Or plain `include_str!`, since only the skeleton is embedded. |
| `chrono` | 0.4.22 | **removed** | `tracing-subscriber` handles timestamps. |
| custom `Logger` | — | **`tracing` + `tracing-subscriber`** | Config `log_level` sets the default, `RUST_LOG` overrides it, colored output is kept, and child-process output is logged as structured fields. |
| new | — | `regex` | For `state` sync matching (§5). |
| `rand` (dev) | 0.8 | removed | Tests become deterministic. |
| new | — | `signal-hook` or `ctrlc` | Clean shutdown, so LEDs can be cleared on exit. |
| new | — | `crossbeam-channel` | Moves events from the callback to the engine and executor. |

Also in this phase:

- **Config format (clean break)**: the file gets a required top-level `"config_version": 2`. Any file without it (every 0.3/0.4 config) is rejected with an error that links the migration guide. There are no deprecated aliases or compatibility code paths.
- **Config location**: `$XDG_CONFIG_HOME/midiboard/config.json` (`~/.config/midiboard/config.json`) on both Linux and macOS. If a legacy `~/midiboard.json` exists, the error points to the migration guide. `--path` keeps working.
- **Platforms**: Linux (ALSA) and **macOS on aarch64 (CoreMIDI)**, both through midir.
  - CI matrix: `ubuntu-latest` (x86_64) and `macos-14` (Apple Silicon), each running fmt, clippy and test.
  - Platform-specific code stays minimal. ALSA virtual ports are used in tests where available, and CoreMIDI supports virtual ports too.
  - Port names differ between ALSA (`client:port N:M`) and CoreMIDI (plain names), so the matching in §3 is tested against both formats.
- **Service files**:
  - Linux: a systemd **user unit** (`~/.config/systemd/user/midiboard.service`, `WantedBy=default.target`), which fixes B9. The system-unit template is removed.
  - macOS: a **launchd agent** (`~/Library/LaunchAgents/dev.choccymilk.midiboard.plist`, `KeepAlive`, logs to `~/Library/Logs/midiboard.log`).
- **Release workflow**: pushing a `v*` tag builds `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` binaries and drafts a GitHub Release with them attached.
- **Identity**: the repo URLs, the schema `$id` and the docs move to `choccydev/midiboard`. The author is `Agata Ordano <gh@choccymilk.dev>`.

## 3. Phase 2: architecture and bug fixes (goal 2)

Restructure the runtime into a pipeline:

```
 ALSA callback ──► parse ──► [channel] ──► Engine (per device thread) ──► [per-control mailbox] ──► Executor
   (no logic,      (typed      bounded       pure state machine,          latest-value-wins          (spawns cmd,
    no alloc)      MidiMsg)                  injected clock)              for encoders,              reports exit
                                                                          FIFO for switch/trigger)   status)
                                                    │
                                                    └────► Feedback ──► MidiOutput (LEDs etc.)
```

- **`midi.rs`**: a small, hand-written parser that turns raw bytes into `MidiMsg { NoteOn, NoteOff, CC, PolyAT, ChanAT, PitchBend, ProgramChange, SysEx }`. It treats note-on with velocity 0 as note-off and safely ignores realtime and unknown messages. This fixes B5 and B6.
- **Control matching** becomes `(message type, channel, number)`. `"type": "note" | "cc"` is **required**. `"channel": 1-16` is optional and defaults to any channel. `devices --input` prints the decoded type, channel and number, so they can be copied straight into the config. Duplicate bindings are a validation error (B12).
- **Engine**: one thread per device, so all configured devices run (B1). It blocks on the channel, with no spin (B2). A precomputed `HashMap<Binding, ControlId>` is built once, which removes the per-message clones (L2).
- **Executor**: commands run off the MIDI path (B8) and capture output with lossy UTF-8 (B15). Each control has at most one command in flight.
  - By default commands are exec'd directly. Setting `"shell": true` runs them through `sh -c` instead (the user's `$SHELL` is not used, for predictability), which allows pipes, `&&` and environment expansion.
  - Placeholders (`{value}`, `{delta}`) are substituted before the command is exec'd. In shell mode they are shell-quoted to prevent injection.
- **Trigger** fires on the *press edge* (note-on velocity > 0, or CC crossing above 0). Its only debounce is a minimum interval since the last fire, so no press is swallowed (B3).
- **Switch** toggles on the press edge, and the first press goes to the opposite of `initial_state` (B4). If the command fails, the switch state stays unchanged (decision D4), so LED feedback never lies.
- **Mapping**: `min + (max-min) * v/127`, with rounding and clamping (B7).
- **Ports**: a single long-lived `MidiInput`/`MidiOutput` per device (L3). Matching is exact port name first, then exact client name, then a case-insensitive substring, and multiple matches produce an error that lists them all (B13).
- **Hotplug**: if the port disappears, the engine logs it, retries the connection every N seconds and repaints feedback when the device comes back (B14).
- **Errors**: `fatal` exits with code 1 and no panic. CLI usage errors come from clap (B10, B11). `config --validate` performs every semantic check (B16).
- **Leak verification**: run `valgrind --leak-check=full` and `heaptrack` on a 30-minute synthetic stream fed through an ALSA virtual port (`midir::os::unix::VirtualOutput`) on a machine with `snd-seq`. Results go in the PR (L4).

## 4. Phase 3: encoder control (goal 3)

### Why the current approach falls apart

The current code opens a window on the first event (`activation` = 300 ms), drops events that come too close together (`detection` = 40 ms), fires once when an event arrives after the window closes, and then **resets the state to `None`**. The consequences:

- **Latency**: up to `activation` ms before anything happens. Then the *next* event only re-arms the window, so the effective update rate is about one per 300+ ms.
- **Lost final value (no trailing edge)**: if you stop turning inside a window, the last position never gets applied. Volume ends wherever the last window closed, not where the knob is.
- **Staircase output**: a smooth turn becomes 2–3 coarse jumps per second.
- **Dropped samples**: events closer than `detection` are popped, which throws away real movement on fast or high-resolution encoders.
- **Direction is computed and then discarded** (`Activation::encoder` ignores `increase`).
- **Relative encoders aren't supported at all.** The BeatStep's 16 encoders are *endless*. In absolute mode they hit 0/127 and stop (the old `TODO:Major`). In relative mode (`64±n`) the "value" is a delta, which the current code treats as an absolute position.

### New design: process every input, rate-limit only output

The engine decodes every event into a position change and never drops input:

```jsonc
"volume": {
  "key": 7, "type": "cc",
  "command": {
    "kind": "Encoder",
    "mode": "absolute",          // absolute | relative_offset64 | relative_twos | relative_signbit | relative_offset16
    "step": 1.0,                 // relative modes: units per tick
    "acceleration": { "curve": 1.6, "max_gain": 8 },   // optional, relative modes
    "wrap": false,               // clamp (default) or wrap around at the range ends
    "rate_limit_ms": 30,         // min interval between command executions
    "deadband": 1,               // absolute mode: ignore ±1 jitter reversals from noisy pots
    "execute": {
      "cmd": "pactl",
      "args": ["set-sink-volume", "@DEFAULT_SINK@", "{value}%"],
      "map_min": 0, "map_max": 100
    }
  }
}
```

1. **Decode**:
   - In absolute mode, the position is `v`.
   - In relative modes, `delta = decode(v)`, `pos += delta * step * gain(dt)`.
   - Positions are kept as `f64` in the `[map_min, map_max]` domain, clamped or wrapped.
   - The decode tables follow the common hardware conventions: offset-64 (the BeatStep's "Relative #1"), two's complement ("Relative #2"), sign-bit, and offset-16 ("Relative #3").
2. **Acceleration**: `gain(dt)` is a smooth function of the time between ticks (a power curve, capped at `max_gain`), so slow turns give fine control and fast turns cover the range. This replaces the "easing" TODO and the unused `ease_input`.
3. **Jitter handling for pots and faders**: a small hysteresis *deadband* replaces time windows. A reversal smaller than `deadband` is ignored; a continued movement is always applied.
4. **Output scheduling (latest value wins)**:
   - **Leading edge**: the first movement after idle runs the command immediately.
   - **Throttle**: after that, at most one execution per `rate_limit_ms`. Each run uses the *newest* value, and intermediate values are coalesced away.
   - **Trailing edge**: when movement stops, the final value is always executed. This guarantees that the end state matches the knob.
   - **Dedupe**: a run is skipped if the rendered integer value equals the last value executed.
   - **Back-pressure**: if the previous command is still running, the newest value waits in a one-slot mailbox. Commands never pile up.
5. **Placeholders**: `{value}` is the mapped absolute value, and `{delta}` is the signed change since the last execution, for tools that take relative steps (`pactl ... +{delta}%`). The old `replace` + `@TOKEN@` mechanism is removed.
6. **Thresholds**: the per-device `thresholds` block and its `detection`/`activation` fields are removed. Encoders use `rate_limit_ms` and `deadband`. Triggers and switches get an optional per-control `debounce_ms`, defaulting to 50.
7. **Starting point**: if a control has a `state` source (§5), the encoder's internal position starts from the real system value (for example, the current volume) instead of 0. Otherwise it starts at `initial` (default `map_min`).

### Measuring it ("plot it against sensitive stuff")

- `midiboard devices --input X --record trace.csv` logs `t_us,status,data1,data2` from real hardware.
- `midiboard replay trace.csv --control volume --out out.csv` runs a trace through the engine with a virtual clock and writes `t_us,input_pos,output_value,executed`. Plotting both columns makes latency, staircasing and the final error visible.
- Regression tests run synthetic traces (slow sweep, fast spin, jitter, direction flip, 1 kHz burst) and assert that:
  - the final executed value equals the final position;
  - latency from input to the first output is at most `rate_limit_ms` (0 on the leading edge);
  - the output never reverses direction during a monotonic sweep;
  - the execution count is at most `duration / rate_limit_ms + 2`.

## 5. Phase 4: device feedback (goal 5)

### Generic output layer

Each device can open a `MidiOutput`. By default the output port uses the same name as the input, which matches how the BeatStep exposes its ports. A control can declare `feedback`:

```jsonc
"device": "Arturia BeatStep",
"output": { "device": "Arturia BeatStep", "profile": "beatstep" },  // output optional; profile optional
"controls": {
  "mute": {
    "key": 36, "type": "note",
    "command": { "kind": "Switch", "initial_state": "OFF", "on": {...}, "off": {...} },
    "feedback": { "led": { "pad": 1, "on": "red", "off": "off" } }   // profile-aware
  },
  "next": {
    "key": 37, "type": "note",
    "command": { "kind": "Trigger", "execute": {...} },
    "feedback": { "led": { "pad": 2, "on": "blue" }, "flash_ms": 150, "error": "magenta" }
  },
  "volume": {
    "key": 7, "type": "cc",
    "command": { "kind": "Encoder", ... },
    "feedback": { "cc": { "number": 7, "channel": 1 } }   // echoes the mapped value back (LED rings/motor faders)
  }
}
```

Feedback actions:

- **`note`**: sends note-on with a velocity, and note-off. This is the generic way most controllers light pads.
- **`cc`**: sends a value. Used for LED rings, motorized faders and echoing absolute encoders.
- **`sysex`**: a raw template with placeholders, as an escape hatch for any device.
- **`led`**: resolved through a **device profile** into whichever of the above the device needs.

When feedback is sent:

- **Switch**: shows the confirmed state. It is painted at startup and repainted after every toggle, after the command result is known.
- **Trigger**: optional flash, or "lit while running". An optional error color is shown when the command exits non-zero.
- **Encoder**: optional CC echo of the mapped value.
- **Startup and reconnect**: every LED is painted to its initial state. **On shutdown** (SIGINT/SIGTERM): every LED is cleared.

### BeatStep profile (`profile: "beatstep"`)

From community reverse-engineering. Arturia does not publish this protocol.

- Set parameter: `F0 00 20 6B 7F 42 02 00 <param> <ctrl> <value> F7`. Get parameter: `... 42 01 00 <param> <ctrl> F7`, and the device replies with `02` and the value.
- LED color is `param 0x10`. Pads 1–16 are controls `0x70–0x7F`. Transport and function buttons are `0x58–0x5F`, where Stop has no LED.
- The original BeatStep's LEDs are red and blue. The colors are `off 0x00`, `red 0x01`, `blue 0x10` and `magenta 0x11`. Some buttons only support a subset of colors, so the profile validates color per control and warns on unsupported combinations.
- **Note fallback**: in Control Mode, a note-on to the pad's note lights it and a note-off clears it. This is how the profile behaves if SysEx is refused.
- **Pad behavior conflict**: in *Toggle* mode the BeatStep drives its own LEDs and fights us. The profile can optionally set each fed-back pad to *Gate* on startup (`param 0x06`, value `0x01`). This is opt-in (`"take_over_pads": true`) because it changes the device's stored settings until power cycle or preset recall.
- Optional helper: `midiboard beatstep setup` sets the chosen encoders to Relative #1 (`param 0x01`/`0x06`) so they become endless with the new encoder engine. Hardware needs to confirm which parameter IDs are correct here, so this is a stretch item.

### External state sync (`state`)

This keeps midiboard's internal state, and its LEDs, in line with changes made outside it, for example muting from the desktop.

```jsonc
"mute": {
  "key": 36, "type": "note",
  "command": { "kind": "Switch", "on": {...}, "off": {...} },
  "state": {
    "cmd": "pactl", "args": ["get-sink-mute", "@DEFAULT_SINK@"],
    "on_when": { "stdout_matches": "yes" },    // or "exit_success"
    "poll_ms": 2000                             // optional; omit to read only at startup and reconnect
  },
  "feedback": { "led": { "pad": 1, "on": "red", "off": "off" } }
},
"volume": {
  "key": 1, "type": "cc",
  "command": { "kind": "Encoder", "mode": "relative_offset64", ... },
  "state": {
    "cmd": "sh", "args": ["-c", "pactl get-sink-volume @DEFAULT_SINK@ | grep -oP '\\d+(?=%)' | head -1"],
    "value_from": { "stdout_number": true },    // parsed into the map_min..map_max domain
    "poll_ms": 5000
  }
}
```

- **Switch**: the state read at startup **replaces `initial_state`**, which becomes optional (it is required only when there is no `state` source). Each poll updates the state and repaints the LED if it changed.
- **Encoder**: each read moves the internal position to the system value, so relative encoders continue from the real volume. While the knob is moving, and for `rate_limit_ms * 2` after it stops, polls are ignored so they don't fight the user.
- **Execution**: polls run on the executor with a timeout, and never in parallel with a command for the same control. Before polling resumes after a command, the post-command state is read once, which confirms whether the command actually took effect.
- **Errors**: a failing state command logs a warning at most once per minute and keeps the last known state.

### Diagnostics

- `midiboard send --device X --hex "F0 00 20 6B 7F 42 02 00 10 70 01 F7"` sends raw bytes.
- `midiboard led --device X --profile beatstep --pad 1 --color red` sets one LED, to check the mapping quickly.

## 6. Hardware test checklist

You run this on your BeatStep, on both Linux and macOS (aarch64). I can't, because there is no MIDI hardware or ALSA sequencer here.

1. `devices --list` shows both input and output ports.
2. `led` lights each pad in every color; Stop ignores it.
3. A switch pad's LED tracks its state across 50 toggles. The LED is correct after unplugging and replugging, and is cleared on Ctrl-C.
4. Encoders in Relative #1 sweep 0→100 volume without stepping and land exactly where you stop. Capture a `--record` trace for the regression suite.
5. A 1-hour soak with `heaptrack` (Linux) or Instruments/`leaks` (macOS) shows flat memory.
6. Muting or unmuting from the desktop updates the mute pad LED within `poll_ms`. The volume encoder continues from the current system volume after the volume is changed externally.

## 7. Phase 5: docs (goal 4)

- Rewrite `docs/config.md` and `docs/events.md` for the new schema. They will cover encoder modes with diagrams of leading, throttle and trailing behavior, switches, triggers, feedback and profiles.
- New `docs/feedback.md` (generic plus BeatStep) and `docs/encoders.md` (how to pick a mode, tuning, and the record/replay workflow).
- `docs/cli.md` is generated from clap (`clap_mangen` or a `--help` dump), so it can't drift.
- The README has the new install steps for Linux and macOS, the systemd user unit and the launchd agent, and the "docs are outdated" banner is removed.
- Example configs for both platforms: `examples/beatstep-linux.json` (`pactl`, `playerctl`) and `examples/beatstep-macos.json` (`osascript` volume and mute, media keys).
- `docs/migration-0.5.md` shows how to rewrite each 0.3/0.4 config construct in the new format, side by side.
- `schema/midiboard.schema.json` is updated: valid 2020-12 keywords, the new fields, a required `config_version`, and the `$id` moved to `choccydev/midiboard`. The skeleton config is rewritten for the new format.
- A test checks that the shipped skeleton and examples validate against both the Rust types and the JSON schema (`jsonschema` dev-dependency).
- `CHANGELOG.md` covers 0.3.3, 0.4.0 and 0.5.0, and the 0.5.0 entry links the migration guide.

## 8. Order of work and size

| Step | Content | PR-sized? |
|---|---|---|
| 1 | Phase 0 (CI, tests) + Phase 1 (deps, clap derive, serde_json, XDG, user unit) | yes |
| 2 | Phase 2 (pipeline, parser, bug fixes B1–B19, leak check) | yes, the largest |
| 3 | Phase 3 (encoder engine, record/replay, trace tests) | yes |
| 4 | Phase 4 (output layer, BeatStep profile, diagnostics) | yes |
| 5 | Phase 5 (docs, schema, changelog, 0.5.0) | yes |

These can be one PR with a commit per step, or five stacked PRs. I'd do one branch with clean commits per step.

## 9. Decisions (resolved)

- **D1 Logging**: replace the custom `Logger` with `tracing`. ✅
- **D2 Config compatibility**: clean break in 0.5.0 with `config_version: 2`. Existing behavior is released first as `v0.3.3` and `v0.4.0`. ✅
- **D3 Identity**: move to `choccydev/midiboard`. The author email is `gh@choccymilk.dev`. ✅
- **D4 Switch failure**: if the command fails, the switch keeps its old state. ✅
- **D5 Platforms**: Linux plus macOS on aarch64, both covered in CI. Windows isn't a target, but midir code isn't deliberately made Linux-only. ✅
- **D6 Shell**: an optional `"shell": true`, with placeholders shell-quoted. ✅
- **D7 External state sync**: included in Phase 4, for both switches and encoders. ✅

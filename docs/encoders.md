# Encoders

This page covers how `Encoder` controls work, how to pick a mode, how to tune
them, and how to measure the result against your own hardware.

## How the value is computed

### Absolute mode (`"mode": "absolute"`)

The controller sends its position, 0-127, as most pots and faders do. midiboard
maps it linearly onto the range:

```text
value = map_min + (map_max - map_min) * raw / 127
```

Endless encoders in absolute mode stop at 0 and 127. If your encoders are
endless (no end stops), switch them to a relative mode on the controller
instead; it's much nicer to use.

**Noisy pots.** Cheap potentiometers often flicker by one step at rest
(`64, 65, 64, 65, ...`), which would make the command run over and over. Set
`deadband` to the size of the flicker, usually `1`: a step backwards that small
is ignored, while movement that continues in the same direction is always
applied. A real change of direction larger than the deadband goes through
immediately.

### Relative modes

Endless encoders in relative mode send *steps* rather than positions.
midiboard keeps the position itself:

```text
value += steps * step * acceleration
```

clamped to `map_min..map_max`, or wrapped around with `"wrap": true`.

There are several common ways to encode steps. Check your controller's manual or
editor, or turn the knob with `midiboard listen` and see what it sends:

| `mode` | Turning right sends | Turning left sends | Also known as |
|---|---|---|---|
| `relative_offset64` | 65, 66, ... | 63, 62, ... | "binary offset", BeatStep **Relative #1** |
| `relative_twos` | 1, 2, ... | 127, 126, ... | "two's complement", BeatStep **Relative #2** |
| `relative_signbit` | 1, 2, ... | 65, 66, ... | "signed bit" |
| `relative_offset16` | 17, 18, ... | 15, 14, ... | BeatStep **Relative #3** |

If turning right makes the value go down, or one tick jumps by ~60, the mode is
wrong. On a BeatStep, `midiboard beatstep show` prints each knob's mode and
`midiboard beatstep setup` changes it (see [feedback](feedback.md#setting-up-the-beatstep)).

**Acceleration** (relative modes only) makes fast turns cover more ground while
slow turns stay precise:

```json
"acceleration": { "curve": 1.5, "max_gain": 4, "slow_ms": 80 }
```

Steps that arrive further apart than `slow_ms` move by `step`. Faster steps are
multiplied by `(slow_ms / time between steps) ^ curve`, capped at `max_gain`.
Raise `curve` for a sharper response, and raise `max_gain` for a bigger top
speed. Many controllers can also accelerate on their own (the BeatStep has a
global "knob acceleration" setting); if you use both, keep midiboard's gentle.

## When the command runs

Every message updates the position, but commands are limited to one per
`rate_limit_ms`:

```text
knob    ▁▂▃▄▅▆▇█████          (messages every few ms, then stop)
command ↑  ↑  ↑  ↑   ↑        leading edge, throttled runs, trailing edge
        └ immediate          └ final value, always
```

1. **Leading edge.** The first movement after a pause runs immediately: no
   added latency.
2. **Throttle.** While you keep turning, the command runs at most once every
   `rate_limit_ms`, always with the newest value. Intermediate values are
   skipped, not queued.
3. **Trailing edge.** When you stop, the last value always runs, at most
   `rate_limit_ms` later. The result always matches the knob.
4. **No pile-up.** If the command is still running, the newest value waits for
   it to finish. A slow command lowers the update rate instead of building a
   backlog.
5. **Dedupe.** A run that would produce the same `{value}` as the last one is
   skipped.

`rate_limit_ms` (default 30) is the main tuning knob. Lower it for smoother
updates when the command is cheap; raise it if the command is expensive or the
target can't keep up. Values between 20 and 60 work well for audio volume.

## Placeholders

- `{value}`: the current value, rounded to `precision` decimals.
- `{delta}`: the change since the previous run, always signed (`+3`, `-1`).
  Useful for tools that take relative steps:

  ```json
  "execute": { "cmd": "pactl", "args": ["set-sink-volume", "@DEFAULT_SINK@", "{delta}%"] }
  ```

  `pactl` reads `+3%`/`-1%` as relative changes.

## Starting value and external changes

The value starts at `initial` (default `map_min`). With relative encoders it's
better to start from the real system value, so the first turn continues from
the current volume instead of jumping. Add a `state` source:

```json
"state": {
    "cmd": "pactl",
    "args": ["get-sink-volume", "@DEFAULT_SINK@"],
    "match": "(\\d+)%",
    "poll_ms": 5000
}
```

The value is read at startup and on every reconnect, and with `poll_ms` also
periodically, so changes from other apps are picked up. Reads are ignored while
you're turning the knob and shortly afterwards, so they never fight your hand.
`match` is a regex whose first group is the number; without it, the first
number in the output is used. Read values count as already applied, so they
never run the command. See [feedback](feedback.md#external-state-sync).

## Measuring: record and replay

To see exactly what midiboard does with your hardware, record a trace and replay
it offline through the same engine.

1. Record while turning the knob the way you normally would:

   ```bash
   midiboard listen "Arturia BeatStep" --record knob.csv
   ```

   The trace has one `t_us,bytes` line per message (microseconds since start,
   then raw hex bytes).

2. Replay it against a control from your config:

   ```bash
   midiboard replay knob.csv --control volume --out out.csv
   ```

   `--exec-ms` sets how long each simulated command takes (default 5), so you
   can see what a slow command does. The output is CSV:

   ```text
   t_us,event,raw,position,value
   1000,input,1,0.7874,
   1000,exec,,0.7874,1
   2000,input,2,1.5748,
   ...
   151000,exec,,100.0000,100
   ```

   `input` rows are messages for the control and the position right after them.
   `exec` rows are command runs with the `{value}` used.

3. Plot `position` and `value` over time to see latency, smoothness and whether
   the final value lands. For example with Python:

   ```python
   import csv, matplotlib.pyplot as plt
   rows = list(csv.DictReader(open("out.csv")))
   inp = [(int(r["t_us"]) / 1000, float(r["position"])) for r in rows if r["event"] == "input"]
   exe = [(int(r["t_us"]) / 1000, float(r["value"])) for r in rows if r["event"] == "exec"]
   plt.plot(*zip(*inp), ".", label="knob position")
   plt.step(*zip(*exe), where="post", label="executed value")
   plt.xlabel("ms"); plt.legend(); plt.show()
   ```

Edit `rate_limit_ms`, `deadband` or `acceleration` and replay again; no
hardware needed. The test suite (`src/replay.rs`) runs the same engine on
synthetic traces (slow and fast sweeps, a jittery pot, direction changes, a
1 kHz burst, slow commands) and checks that the first run is immediate, the
final value always lands, sweeps never step backwards and the number of runs
stays within the rate limit.

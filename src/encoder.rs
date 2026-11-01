//! Encoder state: decoding, acceleration, jitter rejection and output
//! scheduling.
//!
//! The rule is: every input event updates the position, and only command
//! execution is rate limited. A move after idle runs right away (leading
//! edge), further moves run at most once per `rate_limit` with the newest
//! value (throttle), and the last value is always run once movement stops
//! (trailing edge). The engine owns the clock and the in-flight flag; this
//! module only does the math.

use crate::config::{EncoderMode, EncoderSpec};
use std::time::Duration;

pub type Time = Duration;

/// Decodes a relative encoder value into a signed tick count.
pub fn decode_relative(mode: EncoderMode, v: u8) -> i16 {
    let v = i16::from(v & 0x7F);
    match mode {
        EncoderMode::Absolute => 0,
        EncoderMode::RelativeOffset64 => v - 64,
        EncoderMode::RelativeTwos => {
            if v < 64 {
                v
            } else {
                v - 128
            }
        }
        EncoderMode::RelativeSignbit => {
            if v & 0x40 != 0 {
                -(v & 0x3F)
            } else {
                v & 0x3F
            }
        }
        EncoderMode::RelativeOffset16 => v - 16,
    }
}

/// Renders a value with `precision` decimals, never producing "-0".
pub fn render(value: f64, precision: u8) -> String {
    let p = usize::from(precision);
    let s = format!("{value:.p$}");
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') { s[1..].to_string() } else { s }
}

fn round_to(value: f64, precision: u8) -> f64 {
    let f = 10f64.powi(i32::from(precision));
    (value * f).round() / f
}

/// What to put into the command when it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Emission {
    pub value: String,
    /// Signed change since the previous execution, always with a sign.
    pub delta: String,
}

#[derive(Debug, Clone)]
pub struct EncoderState {
    pub pos: f64,
    last_raw: Option<u8>,
    last_dir: i8,
    last_tick: Option<Time>,
    last_input: Option<Time>,
    last_exec_at: Option<Time>,
    last_exec_rendered: String,
    last_exec_value: f64,
    dirty: bool,
}

impl EncoderState {
    pub fn new(spec: &EncoderSpec) -> Self {
        let initial = round_to(spec.initial, spec.precision);
        EncoderState {
            pos: spec.initial,
            last_raw: None,
            last_dir: 0,
            last_tick: None,
            last_input: None,
            last_exec_at: None,
            last_exec_rendered: render(initial, spec.precision),
            last_exec_value: initial,
            dirty: false,
        }
    }

    /// Normalized position for CC echo, 0..=127.
    pub fn cc_value(&self, spec: &EncoderSpec) -> u8 {
        (((self.pos - spec.min) / (spec.max - spec.min)) * 127.0).round().clamp(0.0, 127.0) as u8
    }

    fn gain(spec: &EncoderSpec, dt: Option<Time>) -> f64 {
        match (spec.accel, dt) {
            (Some(a), Some(dt)) => {
                let dt_ms = (dt.as_secs_f64() * 1000.0).max(1.0);
                let slow = a.slow_ms as f64;
                if dt_ms >= slow { 1.0 } else { (slow / dt_ms).powf(a.curve).clamp(1.0, a.max_gain) }
            }
            _ => 1.0,
        }
    }

    /// Applies one input value. Returns true if the position changed.
    pub fn input(&mut self, spec: &EncoderSpec, now: Time, raw: u8) -> bool {
        let before = self.pos;
        if spec.mode == EncoderMode::Absolute {
            if let Some(prev) = self.last_raw {
                let diff = i16::from(raw) - i16::from(prev);
                if diff == 0 {
                    return false;
                }
                let dir = diff.signum() as i8;
                let reversal = self.last_dir != 0 && dir != self.last_dir;
                if reversal && diff.unsigned_abs() <= u16::from(spec.deadband) {
                    // Jitter on a noisy pot: a small step backwards is ignored,
                    // continued movement in the same direction never is.
                    return false;
                }
                self.last_dir = dir;
            }
            self.last_raw = Some(raw);
            self.pos = spec.min + (spec.max - spec.min) * f64::from(raw) / 127.0;
        } else {
            let ticks = decode_relative(spec.mode, raw);
            if ticks == 0 {
                return false;
            }
            let dt = self.last_tick.map(|t| now.saturating_sub(t));
            self.last_tick = Some(now);
            let next = self.pos + f64::from(ticks) * spec.step * Self::gain(spec, dt);
            self.pos = if spec.wrap {
                spec.min + (next - spec.min).rem_euclid(spec.max - spec.min)
            } else {
                next.clamp(spec.min, spec.max)
            };
        }
        self.last_input = Some(now);
        let changed = self.pos != before;
        self.dirty |= changed;
        changed
    }

    /// When the pending value may run, if one is pending.
    pub fn deadline(&self, spec: &EncoderSpec) -> Option<Time> {
        if !self.dirty {
            return None;
        }
        Some(self.last_exec_at.map_or(Duration::ZERO, |t| t + spec.rate_limit))
    }

    pub fn due(&self, spec: &EncoderSpec, now: Time) -> bool {
        self.deadline(spec).is_some_and(|d| now >= d)
    }

    /// Takes the pending value for execution. Returns `None` (and clears the
    /// pending flag) when the rendered value equals the last one executed.
    pub fn take(&mut self, spec: &EncoderSpec, now: Time) -> Option<Emission> {
        self.dirty = false;
        let value = round_to(self.pos, spec.precision);
        let rendered = render(value, spec.precision);
        if rendered == self.last_exec_rendered {
            return None;
        }
        let delta = value - self.last_exec_value;
        let delta_s = render(delta, spec.precision);
        let delta_s = if delta_s.starts_with('-') { delta_s } else { format!("+{delta_s}") };
        self.last_exec_at = Some(now);
        self.last_exec_value = value;
        self.last_exec_rendered = rendered.clone();
        Some(Emission { value: rendered, delta: delta_s })
    }

    /// True while the user is turning the knob, or shortly after.
    pub fn recently_active(&self, spec: &EncoderSpec, now: Time) -> bool {
        let quiet = (spec.rate_limit * 2).max(Duration::from_millis(250));
        self.dirty || self.last_input.is_some_and(|t| now.saturating_sub(t) < quiet)
    }

    /// Moves to a value read from the system. The value is treated as
    /// already executed, so it doesn't trigger a command.
    pub fn sync(&mut self, spec: &EncoderSpec, value: f64) {
        self.pos = value.clamp(spec.min, spec.max);
        let v = round_to(self.pos, spec.precision);
        self.last_exec_value = v;
        self.last_exec_rendered = render(v, spec.precision);
        self.last_raw = None;
        self.last_dir = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccelConfig, Exec};

    pub fn spec(mode: EncoderMode) -> EncoderSpec {
        EncoderSpec {
            mode,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            accel: None,
            wrap: false,
            rate_limit: Duration::from_millis(30),
            deadband: 0,
            initial: 0.0,
            precision: 0,
            exec: Exec { cmd: "true".into(), args: vec![], shell: false, timeout: None },
        }
    }

    fn ms(n: u64) -> Time {
        Duration::from_millis(n)
    }

    #[test]
    fn relative_decoding() {
        use EncoderMode::*;
        assert_eq!(decode_relative(RelativeOffset64, 65), 1);
        assert_eq!(decode_relative(RelativeOffset64, 61), -3);
        assert_eq!(decode_relative(RelativeTwos, 1), 1);
        assert_eq!(decode_relative(RelativeTwos, 127), -1);
        assert_eq!(decode_relative(RelativeTwos, 65), -63);
        assert_eq!(decode_relative(RelativeSignbit, 3), 3);
        assert_eq!(decode_relative(RelativeSignbit, 0x43), -3);
        assert_eq!(decode_relative(RelativeOffset16, 18), 2);
        assert_eq!(decode_relative(RelativeOffset16, 14), -2);
    }

    #[test]
    fn render_never_shows_negative_zero() {
        assert_eq!(render(-0.4, 0), "0");
        assert_eq!(render(-0.001, 2), "0.00");
        assert_eq!(render(-1.0, 0), "-1");
        assert_eq!(render(12.345, 1), "12.3");
    }

    #[test]
    fn absolute_maps_min_to_max() {
        // The old interpolate() ignored map_min.
        let mut s = spec(EncoderMode::Absolute);
        s.min = 20.0;
        s.max = 80.0;
        let mut st = EncoderState::new(&s);
        st.input(&s, ms(0), 0);
        assert_eq!(st.pos, 20.0);
        st.input(&s, ms(1), 127);
        assert_eq!(st.pos, 80.0);
    }

    #[test]
    fn deadband_rejects_small_reversals_only() {
        let mut s = spec(EncoderMode::Absolute);
        s.deadband = 1;
        let mut st = EncoderState::new(&s);
        assert!(st.input(&s, ms(0), 60));
        assert!(st.input(&s, ms(1), 61)); // up
        assert!(!st.input(&s, ms(2), 60)); // jitter back: ignored
        assert!(st.input(&s, ms(3), 62)); // continues up
        assert!(st.input(&s, ms(4), 59)); // real reversal (3 steps)
        assert!(st.input(&s, ms(5), 58)); // continues down
    }

    #[test]
    fn relative_clamps_or_wraps() {
        let mut s = spec(EncoderMode::RelativeOffset64);
        let mut st = EncoderState::new(&s);
        st.input(&s, ms(0), 60); // -4 from 0
        assert_eq!(st.pos, 0.0);
        s.wrap = true;
        let mut st = EncoderState::new(&s);
        st.input(&s, ms(0), 62); // -2 from 0 wraps to 98
        assert_eq!(st.pos, 98.0);
    }

    #[test]
    fn acceleration_scales_fast_turns() {
        let mut s = spec(EncoderMode::RelativeOffset64);
        s.accel = Some(AccelConfig { curve: 1.0, max_gain: 4.0, slow_ms: 80 });
        s.initial = 50.0;
        let mut st = EncoderState::new(&s);
        st.input(&s, ms(0), 65); // first tick: gain 1
        assert_eq!(st.pos, 51.0);
        st.input(&s, ms(200), 65); // slow: gain 1
        assert_eq!(st.pos, 52.0);
        st.input(&s, ms(240), 65); // 40ms: gain 2
        assert_eq!(st.pos, 54.0);
        st.input(&s, ms(245), 65); // 5ms: capped at 4
        assert_eq!(st.pos, 58.0);
    }

    #[test]
    fn take_dedupes_and_reports_signed_delta() {
        let s = spec(EncoderMode::RelativeOffset64);
        let mut st = EncoderState::new(&s);
        st.input(&s, ms(0), 67);
        assert_eq!(st.take(&s, ms(0)), Some(Emission { value: "3".into(), delta: "+3".into() }));
        st.input(&s, ms(1), 62);
        assert_eq!(st.take(&s, ms(40)), Some(Emission { value: "1".into(), delta: "-2".into() }));
        // Moving away and back before execution runs nothing.
        st.input(&s, ms(50), 65);
        st.input(&s, ms(51), 63);
        assert_eq!(st.take(&s, ms(80)), None);
        assert_eq!(st.deadline(&s), None);
    }

    #[test]
    fn sync_moves_without_executing() {
        let s = spec(EncoderMode::RelativeOffset64);
        let mut st = EncoderState::new(&s);
        st.sync(&s, 42.0);
        assert_eq!(st.deadline(&s), None);
        st.input(&s, ms(0), 65);
        assert_eq!(st.take(&s, ms(0)).unwrap(), Emission { value: "43".into(), delta: "+1".into() });
    }
}

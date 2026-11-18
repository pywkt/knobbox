//! Quadrature decoder for rotary encoders.
//!
//! The two encoder contacts A and B form a 2-bit Gray code. Turning one way
//! steps through `00 → 10 → 11 → 01 → 00` (A leads B); the other way runs the
//! sequence backwards. Each valid change counts ±1 "transition". Contact
//! bounce on one edge flips a single bit back and forth, which counts +1 then
//! −1 and nets zero, so no separate debouncing is needed.
//!
//! Transitions are turned into steps at fixed "emit points" (see
//! [`Resolution`]), and the leftover count is cleared there. That keeps the
//! decoder in step with the encoder's clicks even if a transition is missed.

/// How many transitions make one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// One step per full Gray-code cycle (4 transitions), emitted when both
    /// contacts read high. Right for EC11-style encoders with as many clicks
    /// as pulses (e.g. 20 clicks / 20 pulses): one step per click.
    Full,
    /// One step per half cycle (2 transitions), emitted at `00` and `11`.
    /// Right for encoders with twice as many clicks as pulses (e.g. 30 / 15).
    Half,
    /// One step per transition: 4× the pulse count per turn. For smooth
    /// (no-detent) encoders, where finer resolution is welcome.
    Quarter,
}

impl Resolution {
    const fn transitions_per_step(self) -> i16 {
        match self {
            Resolution::Full => 4,
            Resolution::Half => 2,
            Resolution::Quarter => 1,
        }
    }

    const fn is_emit_point(self, state: u8) -> bool {
        match self {
            Resolution::Full => state == 0b11,
            Resolution::Half => state == 0b11 || state == 0b00,
            Resolution::Quarter => true,
        }
    }
}

/// Marks a change of both bits at once: a transition was missed.
const SKIP: i8 = 2;

/// Indexed by `(previous_state << 2) | new_state`, state = `(A << 1) | B`.
/// +1 = forward (A leads B), −1 = backward, 0 = no change, SKIP = both bits
/// changed.
#[rustfmt::skip]
const TABLE: [i8; 16] = [
    //  to 00  to 01  to 10  to 11
           0,    -1,     1,  SKIP, // from 00
           1,     0,  SKIP,    -1, // from 01
          -1,  SKIP,     0,     1, // from 10
        SKIP,     1,    -1,     0, // from 11
];

#[derive(Clone, Copy, Debug)]
pub struct Decoder {
    resolution: Resolution,
    state: u8,
    /// Transitions counted since the last emit point.
    pending: i16,
    /// Direction of the last valid transition, used to guess a missed one.
    last_dir: i8,
}

impl Decoder {
    /// `a` and `b` are the contacts' first readings (true = high).
    pub const fn new(resolution: Resolution, a: bool, b: bool) -> Self {
        Decoder {
            resolution,
            state: state(a, b),
            pending: 0,
            last_dir: 0,
        }
    }

    /// Feed one reading of both contacts. Returns the steps completed by it:
    /// positive = forward, negative = backward, usually 0 or ±1.
    pub fn update(&mut self, a: bool, b: bool) -> i8 {
        let new = state(a, b);
        let change = TABLE[((self.state << 2) | new) as usize];
        self.state = new;
        match change {
            0 => return 0,
            // Both bits changed: a transition was missed between two
            // readings. Assume the encoder kept going the same way.
            SKIP => self.pending = self.pending.saturating_add(2 * self.last_dir as i16),
            dir => {
                self.pending = self.pending.saturating_add(dir as i16);
                self.last_dir = dir;
            }
        }
        if !self.resolution.is_emit_point(new) {
            return 0;
        }
        let steps = round_div(
            self.pending.into(),
            self.resolution.transitions_per_step().into(),
        );
        self.pending = 0;
        steps.clamp(i8::MIN.into(), i8::MAX.into()) as i8
    }
}

const fn state(a: bool, b: bool) -> u8 {
    ((a as u8) << 1) | b as u8
}

/// Divide, rounding halves away from zero.
fn round_div(n: i32, d: i32) -> i32 {
    let half = d / 2;
    if n >= 0 {
        (n + half) / d
    } else {
        (n - half) / d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Forward sequence starting from the `11` rest position.
    const FWD: [(bool, bool); 4] = [(false, true), (false, false), (true, false), (true, true)];

    fn feed(d: &mut Decoder, seq: &[(bool, bool)]) -> i32 {
        seq.iter().map(|&(a, b)| d.update(a, b) as i32).sum()
    }

    fn reversed(seq: &[(bool, bool)]) -> Vec<(bool, bool)> {
        // Backward from 11: 10, 00, 01, 11.
        let mut v: Vec<_> = seq.iter().rev().skip(1).copied().collect();
        v.push((true, true));
        v
    }

    #[test]
    fn full_resolution_one_step_per_cycle() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        assert_eq!(feed(&mut d, &FWD), 1);
        assert_eq!(feed(&mut d, &FWD), 1);
        let back = reversed(&FWD);
        assert_eq!(feed(&mut d, &back), -1);
    }

    #[test]
    fn step_is_emitted_only_on_reaching_rest() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        assert_eq!(feed(&mut d, &FWD[..3]), 0);
        assert_eq!(d.update(true, true), 1);
    }

    #[test]
    fn half_resolution_steps_at_00_and_11() {
        let mut d = Decoder::new(Resolution::Half, true, true);
        let steps: Vec<i8> = FWD.iter().map(|&(a, b)| d.update(a, b)).collect();
        assert_eq!(steps, [0, 1, 0, 1]);
    }

    #[test]
    fn quarter_resolution_steps_every_transition() {
        let mut d = Decoder::new(Resolution::Quarter, true, true);
        assert_eq!(feed(&mut d, &FWD), 4);
        assert_eq!(feed(&mut d, &reversed(&FWD)), -4);
    }

    #[test]
    fn bounce_on_one_edge_nets_zero() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        // A chatters between 1 and 0 while B stays high, ending back at rest.
        let chatter = [(false, true), (true, true), (false, true), (true, true)];
        assert_eq!(feed(&mut d, &chatter), 0);
        // Chatter mid-cycle, then completing the cycle, is exactly one step.
        let mut seq = vec![(false, true), (false, false), (false, true), (false, false)];
        seq.extend([(true, false), (false, false), (true, false), (true, true)]);
        assert_eq!(feed(&mut d, &seq), 1);
    }

    #[test]
    fn direction_reversal_mid_cycle_nets_zero() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        let seq = [(false, true), (false, false), (false, true), (true, true)];
        assert_eq!(feed(&mut d, &seq), 0);
    }

    #[test]
    fn missed_transition_is_filled_in_from_direction() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        // 11 → 01 → (skips 00) 10 → 11: still one forward step.
        let seq = [(false, true), (true, false), (true, true)];
        assert_eq!(feed(&mut d, &seq), 1);
    }

    #[test]
    fn skip_with_no_known_direction_counts_nothing() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        assert_eq!(d.update(false, false), 0);
        assert_eq!(d.update(true, true), 0);
    }

    #[test]
    fn fast_spin_keeps_count() {
        let mut d = Decoder::new(Resolution::Full, true, true);
        let total: i32 = (0..100).map(|_| feed(&mut d, &FWD)).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn noise_cannot_overflow() {
        let mut d = Decoder::new(Resolution::Full, false, true);
        d.update(false, false); // sets a direction
        for _ in 0..100_000 {
            d.update(true, false); // 00 ↔ 10 never reaches 11...
            d.update(false, true); // ...and 10 ↔ 01 is a skip every time
        }
        d.update(true, true);
    }
}

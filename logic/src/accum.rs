//! Collects an encoder's steps between MIDI sends, so a fast turn goes out as
//! one message of +6 rather than six messages of +1.

/// Largest delta one relative-CC message can carry.
pub const MAX_DELTA: i8 = 63;

#[derive(Clone, Copy, Debug, Default)]
pub struct Accumulator {
    pending: i16,
}

impl Accumulator {
    pub const fn new() -> Self {
        Accumulator { pending: 0 }
    }

    pub fn add(&mut self, steps: i8) {
        self.pending = self.pending.saturating_add(steps as i16);
    }

    /// Takes up to ±[`MAX_DELTA`] steps. Anything beyond that stays for the
    /// next send. Returns `None` when nothing is pending.
    pub fn take(&mut self) -> Option<i8> {
        if self.pending == 0 {
            return None;
        }
        let max = MAX_DELTA as i16;
        let out = self.pending.clamp(-max, max);
        self.pending -= out;
        Some(out as i8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_steps_between_sends() {
        let mut a = Accumulator::new();
        assert_eq!(a.take(), None);
        a.add(1);
        a.add(1);
        a.add(-1);
        a.add(1);
        assert_eq!(a.take(), Some(2));
        assert_eq!(a.take(), None);
    }

    #[test]
    fn opposite_steps_cancel_to_nothing() {
        let mut a = Accumulator::new();
        a.add(1);
        a.add(-1);
        assert_eq!(a.take(), None);
    }

    #[test]
    fn large_deltas_are_split_not_lost() {
        let mut a = Accumulator::new();
        for _ in 0..100 {
            a.add(1);
        }
        assert_eq!(a.take(), Some(63));
        assert_eq!(a.take(), Some(37));
        assert_eq!(a.take(), None);
        for _ in 0..70 {
            a.add(-1);
        }
        assert_eq!(a.take(), Some(-63));
        assert_eq!(a.take(), Some(-7));
    }
}

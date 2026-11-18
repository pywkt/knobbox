//! Debouncer for push-switches and buttons.
//!
//! A new state is accepted once the raw reading has disagreed with the
//! current state for [`DEBOUNCE_TICKS`] readings in a row. With a 1 ms scan
//! that is 5 ms, longer than a typical tactile switch bounces and too short
//! to notice.

pub const DEBOUNCE_TICKS: u8 = 5;

#[derive(Clone, Copy, Debug, Default)]
pub struct Debouncer {
    pressed: bool,
    count: u8,
}

impl Debouncer {
    /// Starts released.
    pub const fn new() -> Self {
        Debouncer {
            pressed: false,
            count: 0,
        }
    }

    /// Feed one raw reading (true = closed). Returns the new state when the
    /// debounced state changes.
    pub fn update(&mut self, raw_pressed: bool) -> Option<bool> {
        if raw_pressed == self.pressed {
            self.count = 0;
            return None;
        }
        self.count += 1;
        if self.count < DEBOUNCE_TICKS {
            return None;
        }
        self.count = 0;
        self.pressed = raw_pressed;
        Some(raw_pressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut Debouncer, readings: &[bool]) -> Vec<(usize, bool)> {
        readings
            .iter()
            .enumerate()
            .filter_map(|(i, &r)| d.update(r).map(|s| (i, s)))
            .collect()
    }

    #[test]
    fn clean_press_and_release() {
        let mut d = Debouncer::new();
        let mut r = vec![true; 10];
        r.extend([false; 10]);
        assert_eq!(run(&mut d, &r), [(4, true), (14, false)]);
    }

    #[test]
    fn bouncing_press_gives_one_event() {
        let mut d = Debouncer::new();
        let mut r = vec![true, false, true, true, false, true, false];
        r.extend([true; 10]);
        let events = run(&mut d, &r);
        assert_eq!(events.len(), 1);
        assert!(events[0].1);
    }

    #[test]
    fn glitch_shorter_than_window_is_ignored() {
        let mut d = Debouncer::new();
        let r = [true, true, true, true, false, false, false];
        assert!(run(&mut d, &r).is_empty());
    }
}

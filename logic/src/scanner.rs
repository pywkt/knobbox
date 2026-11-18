//! Turns pin snapshots into MIDI events. The firmware calls
//! [`Scanner::tick`] once per millisecond with fresh readings.

use crate::accum::Accumulator;
use crate::debounce::Debouncer;
use crate::decoder::{Decoder, Resolution};
use crate::layout::{Layout, Snapshot};
use crate::midi::{Event, MAX_BUTTONS, MAX_ENCODERS, MAX_SWITCHES};

/// Encoder deltas are sent every this many ticks (5 ms), so a fast turn
/// becomes one larger message instead of many small ones. Switches and
/// buttons are sent as soon as they're debounced.
pub const SEND_EVERY_TICKS: u8 = 5;

pub struct Scanner {
    layout: &'static Layout,
    decoders: [Decoder; MAX_ENCODERS],
    accums: [Accumulator; MAX_ENCODERS],
    switches: [Debouncer; MAX_SWITCHES],
    buttons: [Debouncer; MAX_BUTTONS],
    tick: u8,
    reverse_knobs: bool,
}

impl Scanner {
    /// `first` is a reading taken at startup, so the decoders begin in step
    /// with the encoders' actual positions.
    pub fn new(layout: &'static Layout, first: &Snapshot) -> Self {
        assert!(layout.encoders.len() <= MAX_ENCODERS);
        assert!(layout.switches.len() <= MAX_SWITCHES);
        assert!(layout.buttons.len() <= MAX_BUTTONS);
        let mut decoders = [Decoder::new(Resolution::Full, true, true); MAX_ENCODERS];
        for (d, e) in decoders.iter_mut().zip(layout.encoders) {
            *d = Decoder::new(e.resolution, first.is_high(e.a), first.is_high(e.b));
        }
        Scanner {
            layout,
            decoders,
            accums: [Accumulator::new(); MAX_ENCODERS],
            switches: [Debouncer::new(); MAX_SWITCHES],
            buttons: [Debouncer::new(); MAX_BUTTONS],
            tick: 0,
            reverse_knobs: false,
        }
    }

    /// Turn every knob's direction round, for a box whose encoders are all
    /// wired with A and B the other way (the firmware's `reverse-knobs`
    /// feature). One knob alone is fixed in the pin table instead.
    pub fn reverse_knobs(mut self, on: bool) -> Self {
        self.reverse_knobs = on;
        self
    }

    pub fn tick(&mut self, snap: &Snapshot, mut emit: impl FnMut(Event)) {
        let layout = self.layout;

        for (i, e) in layout.encoders.iter().enumerate() {
            let steps = self.decoders[i].update(snap.is_high(e.a), snap.is_high(e.b));
            self.accums[i].add(if self.reverse_knobs {
                steps.saturating_neg()
            } else {
                steps
            });
        }

        // Active-low: a closed switch pulls the pin low.
        for (i, &pin) in layout.switches.iter().enumerate() {
            if let Some(pressed) = self.switches[i].update(!snap.is_high(pin)) {
                emit(Event::Switch {
                    index: i as u8,
                    pressed,
                });
            }
        }
        for (i, &pin) in layout.buttons.iter().enumerate() {
            if let Some(pressed) = self.buttons[i].update(!snap.is_high(pin)) {
                emit(Event::Button {
                    index: i as u8,
                    pressed,
                });
            }
        }

        self.tick += 1;
        if self.tick >= SEND_EVERY_TICKS {
            self.tick = 0;
            for (i, acc) in self
                .accums
                .iter_mut()
                .take(layout.encoders.len())
                .enumerate()
            {
                if let Some(delta) = acc.take() {
                    emit(Event::Encoder {
                        index: i as u8,
                        delta,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{PICO_ONLY, Pin};

    struct Rig {
        scanner: Scanner,
        snap: Snapshot,
        events: Vec<Event>,
    }

    impl Rig {
        fn new() -> Self {
            let snap = Snapshot::IDLE;
            Rig {
                scanner: Scanner::new(&PICO_ONLY, &snap),
                snap,
                events: Vec::new(),
            }
        }

        fn tick(&mut self, n: usize) {
            for _ in 0..n {
                let events = &mut self.events;
                self.scanner.tick(&self.snap, |e| events.push(e));
            }
        }

        /// Turn encoder `i` one click forward, one tick per transition.
        fn click(&mut self, i: usize) {
            let e = PICO_ONLY.encoders[i];
            for (a, b) in [(false, true), (false, false), (true, false), (true, true)] {
                self.snap.set(e.a, a);
                self.snap.set(e.b, b);
                self.tick(1);
            }
        }
    }

    #[test]
    fn idle_inputs_send_nothing() {
        let mut r = Rig::new();
        r.tick(1000);
        assert!(r.events.is_empty());
    }

    #[test]
    fn clicks_are_batched_into_one_message() {
        let mut r = Rig::new();
        // Line up with the start of a send window.
        r.tick(SEND_EVERY_TICKS as usize);
        r.click(1); // 4 ticks
        r.tick(1); // window ends
        assert_eq!(r.events, [Event::Encoder { index: 1, delta: 1 }]);
        r.events.clear();
        for _ in 0..5 {
            r.click(2);
        }
        r.tick(SEND_EVERY_TICKS as usize);
        let total: i32 = r
            .events
            .iter()
            .map(|e| match e {
                Event::Encoder { index: 2, delta } => *delta as i32,
                other => panic!("unexpected {other:?}"),
            })
            .sum();
        assert_eq!(total, 5);
        assert!(r.events.len() < 5, "should be batched: {:?}", r.events);
    }

    #[test]
    fn reversed_knobs_count_the_other_way() {
        let mut r = Rig::new();
        r.scanner = Scanner::new(&PICO_ONLY, &r.snap).reverse_knobs(true);
        r.tick(SEND_EVERY_TICKS as usize);
        r.click(1);
        r.tick(1);
        assert_eq!(
            r.events,
            [Event::Encoder {
                index: 1,
                delta: -1
            }]
        );
    }

    #[test]
    fn switch_press_and_release() {
        // PICO_ONLY has no switches (their pins drive the screen), so add some.
        static WITH_SWITCHES: Layout = Layout {
            switches: &[Pin::pico(26), Pin::pico(27), Pin::pico(28)],
            ..PICO_ONLY
        };
        let mut r = Rig::new();
        r.scanner = Scanner::new(&WITH_SWITCHES, &r.snap);
        r.snap.set(WITH_SWITCHES.switches[2], false);
        r.tick(20);
        r.snap.set(WITH_SWITCHES.switches[2], true);
        r.tick(20);
        assert_eq!(
            r.events,
            [
                Event::Switch {
                    index: 2,
                    pressed: true
                },
                Event::Switch {
                    index: 2,
                    pressed: false
                }
            ]
        );
    }

    #[test]
    fn button_press() {
        let mut r = Rig::new();
        r.snap.set(PICO_ONLY.buttons[6], false);
        r.tick(20);
        assert_eq!(
            r.events,
            [Event::Button {
                index: 6,
                pressed: true
            }]
        );
    }

    #[test]
    fn encoder_starting_off_rest_is_handled() {
        // Power-up with encoder 0 sitting between clicks at 00.
        let mut snap = Snapshot::IDLE;
        let e = PICO_ONLY.encoders[0];
        snap.set(e.a, false);
        snap.set(e.b, false);
        let mut scanner = Scanner::new(&PICO_ONLY, &snap);
        let mut events = Vec::new();
        // Finish the click forward: 00 → 10 → 11.
        for (a, b) in [(true, false), (true, true)] {
            snap.set(e.a, a);
            snap.set(e.b, b);
            scanner.tick(&snap, |ev| events.push(ev));
        }
        for _ in 0..SEND_EVERY_TICKS {
            scanner.tick(&snap, |ev| events.push(ev));
        }
        // Half a click from a fresh start rounds to one step, never more.
        assert!(events.len() <= 1, "{events:?}");
    }
}

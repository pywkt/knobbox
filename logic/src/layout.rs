//! The pin table: which physical pin each logical input is wired to.
//!
//! Every input is a [`Pin`]: a source (a Pico GPIO, or one of the MCP23017
//! expanders) plus a bit number. The rest of the code only sees logical
//! inputs, so moving a wire is a one-line change here.
//!
//! All inputs are active-low: the pin has a pull-up, and the encoder contact
//! or switch connects it to ground.

use crate::decoder::Resolution;

pub const MAX_EXPANDERS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A Pico GPIO; `bit` is the GPIO number (GP0 = 0).
    Pico,
    /// MCP23017 number `n` (0-based); `bit` 0–7 = GPA0–7, 8–15 = GPB0–7.
    Expander(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    pub source: Source,
    pub bit: u8,
}

impl Pin {
    pub const fn pico(gpio: u8) -> Self {
        Pin {
            source: Source::Pico,
            bit: gpio,
        }
    }

    pub const fn expander(n: u8, bit: u8) -> Self {
        Pin {
            source: Source::Expander(n),
            bit,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EncoderPins {
    pub a: Pin,
    pub b: Pin,
    pub resolution: Resolution,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// Encoder 1 first. Swap `a` and `b` if one turns the wrong way.
    pub encoders: &'static [EncoderPins],
    /// Push-switches, in the same order as `encoders`.
    pub switches: &'static [Pin],
    /// Panel buttons, in the order of [`Button`].
    pub buttons: &'static [Pin],
}

/// Panel buttons by index. The Bitwig script gives them their meaning; these
/// names only fix the order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Button {
    PagePrev = 0,
    PageNext = 1,
    DevicePrev = 2,
    DeviceNext = 3,
    TrackPrev = 4,
    TrackNext = 5,
    Fine = 6,
}

impl Layout {
    /// Every pin the layout uses.
    pub fn pins(&self) -> impl Iterator<Item = Pin> + '_ {
        self.encoders
            .iter()
            .flat_map(|e| [e.a, e.b])
            .chain(self.switches.iter().copied())
            .chain(self.buttons.iter().copied())
    }
}

/// An EC11-style encoder (one step per click) on two Pico GPIOs.
const fn pico_encoder(a: u8, b: u8) -> EncoderPins {
    EncoderPins {
        a: Pin::pico(a),
        b: Pin::pico(b),
        resolution: Resolution::Full,
    }
}

/// Pico GPIOs the colour screen uses: SPI1 clock, SPI1 data, and D/C. No
/// layout may name them.
pub const SCREEN_GPIOS: [u8; 3] = [26, 27, 28];

/// Until the expanders arrive: 8 encoders and the 7 buttons straight on the
/// Pico. The last 3 pins drive the screen (they were the push-switches of
/// knobs 1–3, which were never wired). Encoders run in pin order down the
/// Pico's left side. Unwired buttons read as "not pressed". See
/// `hardware/pico-only-wiring.md`.
pub const PICO_ONLY: Layout = Layout {
    encoders: &[
        pico_encoder(0, 1),
        pico_encoder(2, 3),
        pico_encoder(4, 5),
        pico_encoder(6, 7),
        pico_encoder(8, 9),
        pico_encoder(10, 11),
        pico_encoder(12, 13),
        pico_encoder(14, 15),
    ],
    switches: &[],
    buttons: &[
        Pin::pico(16), // page ←
        Pin::pico(17), // page →
        Pin::pico(18), // device ←
        Pin::pico(19), // device →
        Pin::pico(20), // track ←
        Pin::pico(21), // track →
        Pin::pico(22), // fine (hold)
    ],
};

/// The state of every input source at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Bit n = GPn is high.
    pub pico: u32,
    /// Bit n of expander e = that pin is high.
    pub expanders: [u16; MAX_EXPANDERS],
}

impl Snapshot {
    /// Everything high: nothing pressed, encoders at rest.
    pub const IDLE: Snapshot = Snapshot {
        pico: u32::MAX,
        expanders: [u16::MAX; MAX_EXPANDERS],
    };

    pub fn is_high(&self, pin: Pin) -> bool {
        match pin.source {
            Source::Pico => self.pico >> pin.bit & 1 == 1,
            Source::Expander(n) => self.expanders[n as usize] >> pin.bit & 1 == 1,
        }
    }

    pub fn set(&mut self, pin: Pin, high: bool) {
        match pin.source {
            Source::Pico => {
                self.pico = (self.pico & !(1 << pin.bit)) | ((high as u32) << pin.bit);
            }
            Source::Expander(n) => {
                let word = &mut self.expanders[n as usize];
                *word = (*word & !(1 << pin.bit)) | ((high as u16) << pin.bit);
            }
        }
    }
}

/// Pico GPIOs that are on the board's edge pins: GP0–22 and GP26–28. (GP23,
/// 24, 25 and 29 are used on the board itself; GP25 is the LED.)
///
/// Once the expanders are added, GP4/GP5 become their I2C bus and must leave
/// the pin table.
pub fn pico_pin_is_usable(gpio: u8) -> bool {
    matches!(gpio, 0..=22 | 26..=28)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::{MAX_BUTTONS, MAX_ENCODERS, MAX_SWITCHES};

    fn check(layout: &Layout) {
        assert!(layout.encoders.len() <= MAX_ENCODERS);
        assert!(layout.switches.len() <= MAX_SWITCHES);
        assert!(layout.buttons.len() <= MAX_BUTTONS);
        let pins: Vec<Pin> = layout.pins().collect();
        for (i, p) in pins.iter().enumerate() {
            assert!(!pins[..i].contains(p), "{p:?} used twice");
            match p.source {
                Source::Pico => {
                    assert!(pico_pin_is_usable(p.bit), "GP{} is not usable", p.bit);
                    assert!(
                        !SCREEN_GPIOS.contains(&p.bit),
                        "GP{} drives the screen",
                        p.bit
                    );
                }
                Source::Expander(n) => {
                    assert!((n as usize) < MAX_EXPANDERS);
                    assert!(p.bit < 16);
                    // MCP23017 datasheet rev. D: GPA7/GPB7 are output only.
                    assert!(
                        p.bit != 7 && p.bit != 15,
                        "GPA7/GPB7 can't be inputs: {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn pico_only_layout_is_valid() {
        check(&PICO_ONLY);
        assert_eq!(PICO_ONLY.encoders.len(), 8);
        assert_eq!(PICO_ONLY.buttons.len(), Button::Fine as usize + 1);
    }

    #[test]
    fn snapshot_set_and_read() {
        let mut s = Snapshot::IDLE;
        let p = Pin::pico(13);
        let x = Pin::expander(2, 9);
        assert!(s.is_high(p) && s.is_high(x));
        s.set(p, false);
        s.set(x, false);
        assert!(!s.is_high(p) && !s.is_high(x));
        assert!(s.is_high(Pin::pico(12)) && s.is_high(Pin::expander(2, 8)));
        s.set(p, true);
        assert!(s.is_high(p));
    }
}

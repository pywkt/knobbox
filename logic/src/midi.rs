//! Events and their MIDI encoding.
//!
//! Everything goes out on MIDI channel 1 as Control Change messages:
//!
//! | Control              | CC        | Value                               |
//! |----------------------|-----------|-------------------------------------|
//! | Encoder 1–16         | 102–117   | relative, two's complement (7-bit)  |
//! | Encoder switch 1–16  | 70–85     | 127 down, 0 up                      |
//! | Button 1–8           | 52–59     | 127 down, 0 up                      |
//!
//! These avoid every CC with a standard job that a synth might act on (bank
//! select, data entry, pedals, (N)RPN, channel mode). 102–117 are undefined
//! in the MIDI spec, 70–85 are sound/general-purpose controllers, 52–59 are
//! the LSBs of the undefined CCs 20–27.
//!
//! The Bitwig script (`bitwig/knobbox.control.js`) uses the same numbers.

/// MIDI channel 1.
pub const CHANNEL: u8 = 0;
pub const ENCODER_CC_BASE: u8 = 102;
pub const SWITCH_CC_BASE: u8 = 70;
pub const BUTTON_CC_BASE: u8 = 52;

pub const MAX_ENCODERS: usize = 16;
pub const MAX_SWITCHES: usize = 16;
pub const MAX_BUTTONS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Encoder `index` (0-based) turned by `delta` steps, ±63 at most.
    Encoder { index: u8, delta: i8 },
    /// Push-switch of encoder `index` (0-based).
    Switch { index: u8, pressed: bool },
    /// Panel button `index` (0-based).
    Button { index: u8, pressed: bool },
}

impl Event {
    /// The (controller number, value) pair this event is sent as.
    pub fn cc(self) -> (u8, u8) {
        match self {
            Event::Encoder { index, delta } => (ENCODER_CC_BASE + index, relative_value(delta)),
            Event::Switch { index, pressed } => (SWITCH_CC_BASE + index, on_off(pressed)),
            Event::Button { index, pressed } => (BUTTON_CC_BASE + index, on_off(pressed)),
        }
    }

    /// The 3 bytes of the MIDI message.
    pub fn midi_bytes(self) -> [u8; 3] {
        let (cc, value) = self.cc();
        [0xB0 | CHANNEL, cc, value]
    }

    /// The 4-byte USB-MIDI event packet: cable 0, code index 0xB (Control
    /// Change), then the MIDI message.
    pub fn usb_packet(self) -> [u8; 4] {
        let [status, cc, value] = self.midi_bytes();
        [0x0B, status, cc, value]
    }
}

/// Two's complement in 7 bits: +1..+63 → 1..63, −1..−63 → 127..65.
pub fn relative_value(delta: i8) -> u8 {
    (delta.clamp(-63, 63) as u8) & 0x7F
}

fn on_off(pressed: bool) -> u8 {
    if pressed { 127 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_encoding_matches_bitwig_2s_complement() {
        assert_eq!(relative_value(1), 1);
        assert_eq!(relative_value(63), 63);
        assert_eq!(relative_value(-1), 127);
        assert_eq!(relative_value(-63), 65);
        assert_eq!(relative_value(100), 63);
        assert_eq!(relative_value(-100), 65);
    }

    #[test]
    fn packets() {
        let e = Event::Encoder {
            index: 0,
            delta: -2,
        };
        assert_eq!(e.usb_packet(), [0x0B, 0xB0, 102, 126]);
        let s = Event::Switch {
            index: 15,
            pressed: true,
        };
        assert_eq!(s.usb_packet(), [0x0B, 0xB0, 85, 127]);
        let b = Event::Button {
            index: 6,
            pressed: false,
        };
        assert_eq!(b.usb_packet(), [0x0B, 0xB0, 58, 0]);
    }

    #[test]
    fn every_control_has_a_distinct_valid_cc() {
        let mut seen = [false; 128];
        let all = (0..MAX_ENCODERS as u8)
            .map(|index| Event::Encoder { index, delta: 1 })
            .chain((0..MAX_SWITCHES as u8).map(|index| Event::Switch {
                index,
                pressed: true,
            }))
            .chain((0..MAX_BUTTONS as u8).map(|index| Event::Button {
                index,
                pressed: true,
            }));
        for e in all {
            let (cc, value) = e.cc();
            let reserved = matches!(cc, 0 | 1 | 6 | 7 | 10 | 32 | 38 | 64..=69 | 96..=101 | 120..);
            assert!(!reserved, "{e:?} uses CC {cc}, which has a standard job");
            assert!(value < 128);
            assert!(!seen[cc as usize], "CC {cc} used twice");
            seen[cc as usize] = true;
        }
    }
}

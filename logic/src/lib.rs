//! All of knobbox's input logic, with no hardware types, so it can be unit
//! tested on the PC with `cargo test`. The firmware reads pins into a
//! [`layout::Snapshot`] every millisecond and hands it to a
//! [`scanner::Scanner`], which returns MIDI events.

#![cfg_attr(not(test), no_std)]

pub mod accum;
pub mod debounce;
pub mod decoder;
pub mod layout;
pub mod midi;
pub mod scanner;
pub mod screen;
pub mod sysex;

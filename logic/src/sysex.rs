//! knobbox's SysEx messages, both ways.
//!
//! Every message is `F0 7D 6B 62 <command> <data…> F7`: `7D` is MIDI's
//! manufacturer ID for non-commercial use, `6B 62` is "kb". All data bytes
//! are 7-bit, as SysEx requires.
//!
//! | Command | Direction | Data | Meaning |
//! |---|---|---|---|
//! | `00` | host → box | | reboot into BOOTSEL (`tools/bootsel.sh`) |
//! | `10` | box → host | | hello: the box has (re)connected, send the screen everything |
//! | `20`–`2A` | host → box | see [`crate::screen`] | what the screen shows |
//!
//! The Bitwig script (`bitwig/knobbox.control.js`) uses the same numbers.

pub const HEADER: [u8; 4] = [0xF0, 0x7D, 0x6B, 0x62];

pub const CMD_REBOOT: u8 = 0x00;
pub const CMD_HELLO: u8 = 0x10;

pub const REBOOT_TO_BOOTSEL: [u8; 6] = [0xF0, 0x7D, 0x6B, 0x62, CMD_REBOOT, 0xF7];
pub const HELLO: [u8; 6] = [0xF0, 0x7D, 0x6B, 0x62, CMD_HELLO, 0xF7];

/// The longest message kept. The script truncates its strings so that every
/// message fits; anything longer is dropped whole.
pub const MAX_MESSAGE: usize = 64;

/// Splits a complete SysEx message (`F0` … `F7`) into its command and data,
/// if it's one of ours.
pub fn ours(msg: &[u8]) -> Option<(u8, &[u8])> {
    let body = msg.strip_prefix(&HEADER)?.strip_suffix(&[0xF7])?;
    let (&cmd, data) = body.split_first()?;
    Some((cmd, data))
}

/// The 4-byte USB-MIDI event packets that carry a SysEx message: cable 0,
/// code index 4 (starts or continues) for each full 3 bytes, then 5/6/7 for
/// the last 1/2/3.
pub fn usb_packets(msg: &[u8], mut emit: impl FnMut([u8; 4])) {
    let mut chunks = msg.chunks(3).peekable();
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        let mut p = [0u8; 4];
        p[0] = if last { 0x04 + chunk.len() as u8 } else { 0x04 };
        p[1..1 + chunk.len()].copy_from_slice(chunk);
        emit(p);
    }
}

/// Collects SysEx bytes from incoming USB-MIDI packets.
pub struct SysexReader {
    buf: [u8; MAX_MESSAGE],
    len: usize,
    overflow: bool,
}

impl Default for SysexReader {
    fn default() -> Self {
        Self::new()
    }
}

impl SysexReader {
    pub const fn new() -> Self {
        SysexReader {
            buf: [0; MAX_MESSAGE],
            len: 0,
            overflow: false,
        }
    }

    /// Feed one 4-byte USB-MIDI event packet. Returns the whole message
    /// (`F0` … `F7`) when this packet completes one that fitted.
    pub fn feed(&mut self, packet: [u8; 4]) -> Option<&[u8]> {
        // Code index number: how many of the 3 MIDI bytes belong to SysEx.
        let (count, ends) = match packet[0] & 0x0F {
            0x4 => (3, false), // SysEx starts or continues
            0x5 => (1, true),  // ends with 1 byte
            0x6 => (2, true),  // ends with 2 bytes
            0x7 => (3, true),  // ends with 3 bytes
            _ => return None,
        };
        for &b in &packet[1..1 + count] {
            if b == 0xF0 {
                self.len = 0;
                self.overflow = false;
            }
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            } else {
                self.overflow = true;
            }
        }
        if !ends {
            return None;
        }
        let len = core::mem::take(&mut self.len);
        if self.overflow || len == 0 || self.buf[0] != 0xF0 {
            return None;
        }
        Some(&self.buf[..len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(r: &mut SysexReader, packets: &[[u8; 4]]) -> Vec<Vec<u8>> {
        packets
            .iter()
            .filter_map(|p| r.feed(*p).map(|m| m.to_vec()))
            .collect()
    }

    #[test]
    fn recognises_reboot_message() {
        let mut r = SysexReader::new();
        let got = read_all(
            &mut r,
            &[[0x04, 0xF0, 0x7D, 0x6B], [0x07, 0x62, 0x00, 0xF7]],
        );
        assert_eq!(got, [REBOOT_TO_BOOTSEL.to_vec()]);
        assert_eq!(ours(&got[0]), Some((CMD_REBOOT, &[][..])));
    }

    #[test]
    fn ignores_other_sysex_and_cc() {
        let mut r = SysexReader::new();
        assert!(r.feed([0x0B, 0xB0, 20, 1]).is_none());
        let other = r.feed([0x04, 0xF0, 0x7E, 0x7F]).map(|m| m.to_vec());
        assert!(other.is_none());
        let identity = r
            .feed([0x07, 0x06, 0x01, 0xF7])
            .map(|m| m.to_vec())
            .unwrap();
        assert_eq!(ours(&identity), None);
        // A message longer than the buffer is dropped whole.
        let mut long = vec![0xF0, 0x7D, 0x6B, 0x62, 0x20];
        long.extend(std::iter::repeat_n(b'x', MAX_MESSAGE));
        long.push(0xF7);
        let mut packets = Vec::new();
        usb_packets(&long, |p| packets.push(p));
        assert!(read_all(&mut r, &packets).is_empty());
        // And it still works afterwards.
        let got = read_all(
            &mut r,
            &[[0x04, 0xF0, 0x7D, 0x6B], [0x07, 0x62, 0x00, 0xF7]],
        );
        assert_eq!(got, [REBOOT_TO_BOOTSEL.to_vec()]);
    }

    #[test]
    fn packets_round_trip() {
        for len in 0..10 {
            let mut msg = HEADER.to_vec();
            msg.push(0x20);
            msg.extend((0..len).map(|i| b'a' + i));
            msg.push(0xF7);
            let mut packets = Vec::new();
            usb_packets(&msg, |p| packets.push(p));
            let mut r = SysexReader::new();
            assert_eq!(read_all(&mut r, &packets), [msg.clone()], "length {len}");
            let (cmd, data) = ours(&msg).unwrap();
            assert_eq!(cmd, 0x20);
            assert_eq!(data.len(), len as usize);
        }
    }

    #[test]
    fn hello_packets() {
        let mut packets = Vec::new();
        usb_packets(&HELLO, |p| packets.push(p));
        assert_eq!(
            packets,
            [[0x04, 0xF0, 0x7D, 0x6B], [0x07, 0x62, 0x10, 0xF7]]
        );
    }
}

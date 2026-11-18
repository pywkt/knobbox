//! Draws the screen on the PC, for a look without the hardware:
//! `cargo run --example preview -- out.ppm` writes four pictures with
//! made-up Bitwig values: `out-startup.ppm` (before Bitwig is heard from),
//! `out.ppm` (a device page), `out-mixer.ppm` and `out-zoom.ppm` (a knob
//! being turned).

use knobbox_logic::screen::{
    self, BUF_LEN, HEIGHT, NO_METER, RegionBuf, STATUS_LOCKED, STATUS_PLAYING, STATUS_RECORDING,
    STYLE_COLOR, STYLE_SELECTED, Screen, WIDTH, bands, regions,
};

struct Image(Vec<u8>);

impl Image {
    fn new() -> Self {
        Image(vec![0u8; (WIDTH * HEIGHT * 3) as usize])
    }

    /// Draw what changed, as the firmware does, band by band.
    fn draw(&mut self, s: &mut Screen) {
        let mut buf = vec![0u8; BUF_LEN];
        for r in regions(s.take_dirty()) {
            for band in bands(r.rect()) {
                let mut rb = RegionBuf::new(band, &mut buf);
                s.render(r, &mut rb);
                let a = rb.area();
                for (i, px) in rb.bytes().as_chunks::<2>().0.iter().enumerate() {
                    let v = u16::from_be_bytes([px[0], px[1]]);
                    let x = a.top_left.x as u32 + i as u32 % a.size.width;
                    let y = a.top_left.y as u32 + i as u32 / a.size.width;
                    let o = ((y * WIDTH + x) * 3) as usize;
                    self.0[o] = ((v >> 11) as u8) << 3;
                    self.0[o + 1] = ((v >> 5 & 0x3F) as u8) << 2;
                    self.0[o + 2] = ((v & 0x1F) as u8) << 3;
                }
            }
        }
    }

    fn save(&self, path: &str) {
        let mut out = format!("P6 {WIDTH} {HEIGHT} 255\n").into_bytes();
        out.extend(&self.0);
        std::fs::write(path, out).unwrap();
    }
}

fn knob(s: &mut Screen, k: u8, name: &str, v: f32, text: &str) {
    let mut d = vec![k];
    d.extend_from_slice(name.as_bytes());
    s.apply(screen::CMD_KNOB_NAME, &d);
    let v = (v * screen::VALUE_MAX as f32) as u16;
    let mut d = vec![k, (v >> 7) as u8, (v & 0x7F) as u8];
    d.extend_from_slice(text.as_bytes());
    s.apply(screen::CMD_KNOB_VALUE, &d);
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "screen.ppm".into());
    let named = |suffix: &str| path.replace(".ppm", &format!("-{suffix}.ppm"));
    let mut img = Image::new();
    let mut s = Screen::new();
    img.draw(&mut s);
    img.save(&named("startup"));

    // A device page: Fine held, the device locked, playing at 120 BPM, and an
    // LFO moving the cutoff.
    s.apply(screen::CMD_TRACK, b"Bass Synth");
    s.apply(screen::CMD_COLOR, &[30, 90, 127]);
    s.apply(screen::CMD_DEVICE, b"Polysynth");
    s.apply(screen::CMD_PAGE, b"\x01\x05Filter");
    s.apply(screen::CMD_STATUS, &[STATUS_PLAYING | STATUS_LOCKED, 9, 48]);
    let knobs: [(&str, f32, &str); 8] = [
        ("Cutoff", 0.62, "1.84 kHz"),
        ("Resonance", 0.25, "25.0 %"),
        ("Env Amount", 0.5, "+0.00 st"),
        ("Key Track", 1.0, "100 %"),
        ("Drive", 0.0, "0.00 dB"),
        ("Filter Type", 0.33, "LP 4"),
        ("Mix", 0.8, "80.0 %"),
        ("", 0.0, ""),
    ];
    for (k, (name, v, text)) in knobs.iter().enumerate() {
        knob(&mut s, k as u8, name, *v, text);
    }
    s.apply(screen::CMD_KNOB_MOD, &[0, 100]);
    s.apply(screen::CMD_METERS, &[88]);
    s.set_fine(true);
    img.draw(&mut s);
    img.save(&path);

    // The mixer: track names, colours and levels; "Bass Synth" is selected.
    s.set_fine(false);
    s.apply(screen::CMD_DEVICE, b"Mixer: tracks 1-8");
    s.apply(screen::CMD_PAGE, b"\x00\x04Volume");
    s.apply(
        screen::CMD_STATUS,
        &[STATUS_PLAYING | STATUS_RECORDING, 9, 48],
    );
    let tracks: [(&str, f32, &str, [u8; 3], u8); 8] = [
        ("Drums", 0.79, "+0.0 dB", [127, 60, 20], 110),
        ("Bass Synth", 0.72, "-2.4 dB", [30, 90, 127], 88),
        ("Pad", 0.55, "-9.1 dB", [90, 40, 127], 40),
        ("Lead", 0.66, "-4.8 dB", [40, 120, 50], 70),
        ("Vocals", 0.79, "+0.0 dB", [127, 110, 20], 120),
        ("FX Riser", 0.3, "-24 dB", [100, 100, 100], 0),
        ("Keys", 0.6, "-7.0 dB", [20, 110, 110], 55),
        ("", 0.0, "", [0, 0, 0], NO_METER),
    ];
    let mut levels = vec![screen::METER_MAX - 38];
    for (k, (name, v, text, [r, g, b], level)) in tracks.iter().enumerate() {
        knob(&mut s, k as u8, name, *v, text);
        let selected = if k == 1 { STYLE_SELECTED } else { 0 };
        s.apply(
            screen::CMD_KNOB_STYLE,
            &[k as u8, STYLE_COLOR | selected, *r, *g, *b],
        );
        levels.push(*level);
    }
    s.apply(screen::CMD_KNOB_MOD, &[0]);
    s.apply(screen::CMD_METERS, &levels);
    img.draw(&mut s);
    img.save(&named("mixer"));

    // Turning the selected track's volume.
    s.touch(1, 0);
    img.draw(&mut s);
    img.save(&named("zoom"));
    println!(
        "wrote {}, {path}, {} and {}",
        named("startup"),
        named("mixer"),
        named("zoom")
    );
}

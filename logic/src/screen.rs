//! What the 240 × 240 colour screen shows, and how it's drawn.
//!
//! The Bitwig script sends names, values and levels as SysEx (see
//! [`crate::sysex`]); [`Screen::apply`] stores them and marks the regions they
//! change. The firmware then draws each dirty [`Region`] into a small buffer
//! with [`Screen::render`] (in [`bands`] when it's bigger than the buffer) and
//! sends that rectangle to the display, so a knob turn redraws one knob's box,
//! not the whole screen.
//!
//! The screen doesn't know about Bitwig's modes: in the mixer the script sends
//! track names as the knob names, the tracks' colours as knob styles, and
//! their levels as knob meters.
//!
//! Layout, matching the panel (knobs 1–4 are the back row):
//!
//! ```text
//! +----------------------------------------+
//! | Track name                             |  header, 34 px
//! | Device                     Page  2/5   |
//! | > 120.0 BPM                 LOCK FINE  |  status, 15 px
//! |========== selected track's level      |  meter, 3 px
//! +---------+---------+---------+----------+
//! |  knob 1 |  knob 2 |  knob 3 |  knob 4  |  94 px: name, arc, value,
//! +---------+---------+---------+----------+  level (mixer)
//! |  knob 5 |  knob 6 |  knob 7 |  knob 8  |
//! +---------+---------+---------+----------+
//! ```
//!
//! While a knob turns, its name, arc and value fill the space below the meter
//! ("zoom", [`Screen::touch`]) until it has been still for a moment.
//!
//! Screen messages (data after the command byte; strings are printable
//! ASCII, anything else shows as `?`):
//!
//! | Command | Data |
//! |---|---|
//! | `20` | track name |
//! | `21` | track colour: red, green, blue, 0–127 each |
//! | `22` | device name (empty: no device) |
//! | `23` | page index (0-based, `7F` = none), page count, page name |
//! | `24` | knob (0-based), parameter name (empty: no parameter) |
//! | `25` | knob, value high 7 bits, value low 7 bits (0–16383), value as text |
//! | `26` | status flags (1 playing, 2 recording, 4 locked), tempo × 10 high 7 bits, low 7 bits (tempo optional) |
//! | `27` | knob, style flags (1 has a colour, 2 selected), red, green, blue (colour optional) |
//! | `28` | knob, modulated value 0–127 (absent: not modulated) |
//! | `29` | selected track's level, then a level per knob from knob 1 (0–126, `7F` = no meter) |
//! | `2A` | zoom time in tenths of a second (0: no zoom) |

use core::convert::Infallible;
use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{
    FONT_6X10, FONT_6X13, FONT_7X13_BOLD, FONT_9X15_BOLD, FONT_10X20,
};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Arc, Circle, PrimitiveStyle, Rectangle, Triangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyle, TextStyleBuilder};

pub const WIDTH: u32 = 240;
pub const HEIGHT: u32 = 240;

/// Knobs the protocol carries (the v1 box has 16); the grid shows the first
/// [`SHOWN_KNOBS`], the zoom any of them.
pub const MAX_KNOBS: usize = 16;
pub const SHOWN_KNOBS: usize = 8;
const COLS: u32 = 4;

const HEADER_H: u32 = 34;
const STATUS_H: u32 = 15;
const METER_H: u32 = 3;
const TOP_H: u32 = HEADER_H + STATUS_H + METER_H;
const CELL_W: u32 = WIDTH / COLS;
const CELL_H: u32 = (HEIGHT - TOP_H) / 2;

/// The drawing buffer: 40 full rows, RGB565 big-endian. Regions bigger than
/// this are drawn in [`bands`].
pub const BUF_LEN: usize = (WIDTH * 40 * 2) as usize;

pub const CMD_TRACK: u8 = 0x20;
pub const CMD_COLOR: u8 = 0x21;
pub const CMD_DEVICE: u8 = 0x22;
pub const CMD_PAGE: u8 = 0x23;
pub const CMD_KNOB_NAME: u8 = 0x24;
pub const CMD_KNOB_VALUE: u8 = 0x25;
pub const CMD_STATUS: u8 = 0x26;
pub const CMD_KNOB_STYLE: u8 = 0x27;
pub const CMD_KNOB_MOD: u8 = 0x28;
pub const CMD_METERS: u8 = 0x29;
pub const CMD_CONFIG: u8 = 0x2A;

pub const STATUS_PLAYING: u8 = 1;
pub const STATUS_RECORDING: u8 = 2;
pub const STATUS_LOCKED: u8 = 4;
pub const STYLE_COLOR: u8 = 1;
pub const STYLE_SELECTED: u8 = 2;
pub const METER_MAX: u8 = 126;
pub const NO_METER: u8 = 0x7F;
pub const MOD_MAX: u8 = 127;

pub const NAME_LEN: usize = 16;
pub const VALUE_LEN: usize = 12;
pub const TITLE_LEN: usize = 24;
/// Value messages carry 14 bits.
pub const VALUE_MAX: u16 = 0x3FFF;
/// Until the script says otherwise: zoom for a second.
const DEFAULT_ZOOM_TENTHS: u8 = 10;

const BG: Rgb565 = Rgb565::BLACK;
const HEADER_BG: Rgb565 = Rgb565::new(3, 6, 3);
const TEXT: Rgb565 = Rgb565::WHITE;
const DIM_TEXT: Rgb565 = Rgb565::new(22, 44, 22);
const TRACK_RING: Rgb565 = Rgb565::new(6, 12, 6);
const MOD_RING: Rgb565 = Rgb565::new(28, 56, 28);
/// Used until Bitwig sends a track colour, or when it's too dark to see.
const DEFAULT_ACCENT: Rgb565 = Rgb565::new(31, 38, 0);
const FINE_BADGE: Rgb565 = Rgb565::new(31, 56, 0);
const LOCK_BADGE: Rgb565 = Rgb565::new(8, 50, 31);
const PLAY: Rgb565 = Rgb565::new(4, 52, 8);
const RECORD: Rgb565 = Rgb565::new(31, 8, 4);
const METER_LOW: Rgb565 = Rgb565::new(4, 48, 8);
const METER_MID: Rgb565 = Rgb565::new(30, 52, 0);
const METER_HIGH: Rgb565 = Rgb565::new(31, 10, 2);

/// A knob's arc: diameter, stroke, and where it sits in its cell.
const ARC_D: u32 = 44;
const ARC_W: u32 = 5;
const ARC_TOP: i32 = 19;
const ARC_START: f32 = 135.0;
const ARC_SWEEP: f32 = 270.0;
/// A knob's level meter, at the right of its cell beside the arc.
const CELL_METER_X: i32 = 56;
const CELL_METER_W: u32 = 3;

/// The zoom: one knob's arc, large, centred below the name.
const ZOOM_ARC_D: u32 = 100;
const ZOOM_ARC_W: u32 = 10;
const ZOOM_ARC_TL: Point = Point::new(70, 89);
const ZOOM_METER: Rectangle = Rectangle::new(Point::new(214, 89), Size::new(8, 100));

/// A short ASCII string with a fixed capacity.
#[derive(Clone, Copy, Debug)]
pub struct Str<const N: usize> {
    buf: [u8; N],
    len: u8,
}

impl<const N: usize> Default for Str<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Str<N> {
    pub const fn new() -> Self {
        Str {
            buf: [0; N],
            len: 0,
        }
    }

    /// Store `bytes`, cut to N, with anything but printable ASCII as `?`.
    /// Returns whether the string changed.
    pub fn set(&mut self, bytes: &[u8]) -> bool {
        let mut new = Self::new();
        for (d, &b) in new.buf.iter_mut().zip(bytes) {
            *d = if (0x20..=0x7E).contains(&b) { b } else { b'?' };
            new.len += 1;
        }
        let changed = new.as_str() != self.as_str();
        *self = new;
        changed
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<const N: usize> Write for Str<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if self.len as usize == N {
                return Err(core::fmt::Error);
            }
            self.buf[self.len as usize] = b;
            self.len += 1;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Knob {
    pub name: Str<NAME_LEN>,
    pub text: Str<VALUE_LEN>,
    pub value: u16,
    /// Its own colour (a track's, in the mixer); otherwise the track colour.
    pub color: Option<Rgb565>,
    /// Marked as the selected one (the selected track, in the mixer).
    pub selected: bool,
    /// Where modulation has moved it to, 0–127, when it's moved.
    pub modulated: Option<u8>,
    /// Its level, 0–126, when it has a meter.
    pub meter: Option<u8>,
}

impl Knob {
    const fn new() -> Self {
        Knob {
            name: Str::new(),
            text: Str::new(),
            value: 0,
            color: None,
            selected: false,
            modulated: None,
            meter: None,
        }
    }
}

/// A part of the screen that's redrawn on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Track, device and page.
    Header,
    /// Transport, tempo, and the LOCK and FINE badges.
    Status,
    /// The selected track's level, a strip across the screen.
    HeaderMeter,
    /// One knob's box of the grid…
    Knob(u8),
    /// …and its level meter alone, which changes much more often.
    KnobMeter(u8),
    /// The zoom: name across the top…
    ZoomTop,
    /// …the arc in the middle, with blank space left of it…
    ZoomLeft,
    ZoomArc,
    /// …and the meter right of it…
    ZoomRight,
    /// …and the value along the bottom.
    ZoomBottom,
}

const CELLS: u32 = (1 << SHOWN_KNOBS) - 1;
const CELL_METERS: u32 = CELLS << 8;
const HEADER_BIT: u32 = 1 << 16;
const STATUS_BIT: u32 = 1 << 17;
const HEADER_METER_BIT: u32 = 1 << 18;
const ZOOM_TOP: u32 = 1 << 19;
const ZOOM_LEFT: u32 = 1 << 20;
const ZOOM_ARC: u32 = 1 << 21;
const ZOOM_RIGHT: u32 = 1 << 22;
const ZOOM_BOTTOM: u32 = 1 << 23;
const ZOOM_ALL: u32 = ZOOM_TOP | ZOOM_LEFT | ZOOM_ARC | ZOOM_RIGHT | ZOOM_BOTTOM;
const TOP: u32 = HEADER_BIT | STATUS_BIT | HEADER_METER_BIT;
/// Everything, as the grid shows it.
const ALL: u32 = TOP | CELLS | CELL_METERS;

impl Region {
    pub fn rect(self) -> Rectangle {
        let r = |x: u32, y: u32, w: u32, h: u32| {
            Rectangle::new(Point::new(x as i32, y as i32), Size::new(w, h))
        };
        let body = TOP_H;
        let mid_h = 114;
        let zoom_mid = body + 30;
        match self {
            Region::Header => r(0, 0, WIDTH, HEADER_H),
            Region::Status => r(0, HEADER_H, WIDTH, STATUS_H),
            Region::HeaderMeter => r(0, HEADER_H + STATUS_H, WIDTH, METER_H),
            Region::Knob(i) => {
                let tl = cell_origin(i);
                Rectangle::new(tl, Size::new(CELL_W, CELL_H))
            }
            Region::KnobMeter(i) => {
                let tl = cell_origin(i);
                Rectangle::new(
                    tl + Point::new(CELL_METER_X, ARC_TOP),
                    Size::new(CELL_METER_W, ARC_D),
                )
            }
            Region::ZoomTop => r(0, body, WIDTH, 30),
            Region::ZoomLeft => r(0, zoom_mid, 60, mid_h),
            Region::ZoomArc => r(60, zoom_mid, 120, mid_h),
            Region::ZoomRight => r(180, zoom_mid, 60, mid_h),
            Region::ZoomBottom => r(0, zoom_mid + mid_h, WIDTH, HEIGHT - zoom_mid - mid_h),
        }
    }

    fn bit(self) -> u32 {
        match self {
            Region::Header => HEADER_BIT,
            Region::Status => STATUS_BIT,
            Region::HeaderMeter => HEADER_METER_BIT,
            Region::Knob(i) => 1 << i,
            Region::KnobMeter(i) => 1 << (8 + i),
            Region::ZoomTop => ZOOM_TOP,
            Region::ZoomLeft => ZOOM_LEFT,
            Region::ZoomArc => ZOOM_ARC,
            Region::ZoomRight => ZOOM_RIGHT,
            Region::ZoomBottom => ZOOM_BOTTOM,
        }
    }
}

fn cell_origin(i: u8) -> Point {
    let (col, row) = (i as u32 % COLS, i as u32 / COLS);
    Point::new((col * CELL_W) as i32, (TOP_H + row * CELL_H) as i32)
}

/// The regions named by a dirty mask from [`Screen::take_dirty`], top first.
/// A knob's meter comes after its box, so a box drawn whole is never left
/// with a stale meter.
pub fn regions(dirty: u32) -> impl Iterator<Item = Region> {
    [Region::Header, Region::Status, Region::HeaderMeter]
        .into_iter()
        .chain((0..SHOWN_KNOBS as u8).map(Region::Knob))
        .chain((0..SHOWN_KNOBS as u8).map(Region::KnobMeter))
        .chain([
            Region::ZoomTop,
            Region::ZoomLeft,
            Region::ZoomArc,
            Region::ZoomRight,
            Region::ZoomBottom,
        ])
        .filter(move |r| dirty & r.bit() != 0)
}

/// `area` cut into horizontal bands that each fit in [`BUF_LEN`].
pub fn bands(area: Rectangle) -> impl Iterator<Item = Rectangle> {
    let rows = (BUF_LEN as u32 / (area.size.width.max(1) * 2)).max(1);
    let h = area.size.height;
    (0..h.div_ceil(rows)).map(move |i| {
        let y = i * rows;
        Rectangle::new(
            area.top_left + Point::new(0, y as i32),
            Size::new(area.size.width, rows.min(h - y)),
        )
    })
}

/// What a knob message changed, for [`Screen::knob_dirty`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Name,
    Value,
    Style,
    Modulation,
    Meter,
}

#[derive(Clone)]
pub struct Screen {
    /// Bitwig has sent something; until then the screen shows a start-up page.
    live: bool,
    track: Str<TITLE_LEN>,
    color: Option<Rgb565>,
    device: Str<TITLE_LEN>,
    page: Str<NAME_LEN>,
    page_index: Option<u8>,
    page_count: u8,
    knobs: [Knob; MAX_KNOBS],
    fine: bool,
    status: u8,
    /// Tempo in tenths of a BPM.
    tempo: Option<u16>,
    track_meter: Option<u8>,
    /// The knob filling the screen, and until when (milliseconds, the
    /// caller's clock).
    zoom: Option<u8>,
    zoom_until: u64,
    zoom_tenths: u8,
    dirty: u32,
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen {
    pub const fn new() -> Self {
        Screen {
            live: false,
            track: Str::new(),
            color: None,
            device: Str::new(),
            page: Str::new(),
            page_index: None,
            page_count: 0,
            knobs: [Knob::new(); MAX_KNOBS],
            fine: false,
            status: 0,
            tempo: None,
            track_meter: None,
            zoom: None,
            zoom_until: 0,
            zoom_tenths: DEFAULT_ZOOM_TENTHS,
            dirty: ALL,
        }
    }

    /// Apply one screen message. Returns false for a command or data it
    /// doesn't recognise (and changes nothing).
    pub fn apply(&mut self, cmd: u8, data: &[u8]) -> bool {
        let mut dirty = 0;
        match (cmd, data) {
            (CMD_TRACK, name) => {
                if self.track.set(name) {
                    dirty |= HEADER_BIT;
                }
            }
            (CMD_COLOR, &[r, g, b]) => {
                let c = Some(color(r, g, b));
                if c != self.color {
                    self.color = c;
                    dirty |= ALL | ZOOM_ALL; // the header bar and every arc
                }
            }
            (CMD_DEVICE, name) => {
                if self.device.set(name) {
                    dirty |= HEADER_BIT;
                }
            }
            (CMD_PAGE, &[index, count, ref name @ ..]) => {
                let index = (index != 0x7F).then_some(index);
                if self.page.set(name) | (index != self.page_index) | (count != self.page_count) {
                    self.page_index = index;
                    self.page_count = count;
                    dirty |= HEADER_BIT;
                }
            }
            (CMD_KNOB_NAME, &[k, ref name @ ..]) if (k as usize) < MAX_KNOBS => {
                if self.knobs[k as usize].name.set(name) {
                    dirty |= self.knob_dirty(k, Part::Name);
                }
            }
            (CMD_KNOB_VALUE, &[k, hi, lo, ref text @ ..]) if (k as usize) < MAX_KNOBS => {
                let knob = &mut self.knobs[k as usize];
                let value = seven_bit_pair(hi, lo);
                if knob.text.set(text) | (value != knob.value) {
                    knob.value = value;
                    dirty |= self.knob_dirty(k, Part::Value);
                }
            }
            (CMD_STATUS, &[flags, ref tempo @ ..]) if matches!(tempo.len(), 0 | 2) => {
                let tempo = match *tempo {
                    [hi, lo] => Some(seven_bit_pair(hi, lo)),
                    _ => None,
                };
                if flags != self.status || tempo != self.tempo {
                    self.status = flags;
                    self.tempo = tempo;
                    dirty |= STATUS_BIT;
                }
            }
            (CMD_KNOB_STYLE, &[k, flags, ref rgb @ ..])
                if (k as usize) < MAX_KNOBS && matches!(rgb.len(), 0 | 3) =>
            {
                let c = match *rgb {
                    [r, g, b] if flags & STYLE_COLOR != 0 => Some(color(r, g, b)),
                    _ => None,
                };
                let selected = flags & STYLE_SELECTED != 0;
                let knob = &mut self.knobs[k as usize];
                if c != knob.color || selected != knob.selected {
                    knob.color = c;
                    knob.selected = selected;
                    dirty |= self.knob_dirty(k, Part::Style);
                }
            }
            (CMD_KNOB_MOD, &[k, ref m @ ..]) if (k as usize) < MAX_KNOBS && m.len() <= 1 => {
                let m = m.first().map(|&m| m.min(MOD_MAX));
                let knob = &mut self.knobs[k as usize];
                if m != knob.modulated {
                    knob.modulated = m;
                    dirty |= self.knob_dirty(k, Part::Modulation);
                }
            }
            (CMD_METERS, &[track, ref levels @ ..]) if levels.len() <= MAX_KNOBS => {
                let track = level(track);
                if track != self.track_meter {
                    self.track_meter = track;
                    dirty |= HEADER_METER_BIT;
                }
                for (k, &l) in levels.iter().enumerate() {
                    let l = level(l);
                    if l != self.knobs[k].meter {
                        self.knobs[k].meter = l;
                        dirty |= self.knob_dirty(k as u8, Part::Meter);
                    }
                }
            }
            (CMD_CONFIG, &[zoom_tenths]) => {
                self.zoom_tenths = zoom_tenths;
                if zoom_tenths == 0 {
                    self.end_zoom();
                }
            }
            _ => return false,
        }
        if !self.live {
            self.live = true;
            dirty = ALL;
        }
        self.dirty |= dirty;
        true
    }

    /// The regions a change to knob `k` redraws, in the grid and the zoom.
    fn knob_dirty(&self, k: u8, part: Part) -> u32 {
        let mut d = 0;
        if (k as usize) < SHOWN_KNOBS {
            d |= if part == Part::Meter {
                1 << (8 + k)
            } else {
                1 << k
            };
        }
        if self.zoom == Some(k) {
            d |= match part {
                Part::Name => ZOOM_TOP,
                Part::Value => ZOOM_ARC | ZOOM_BOTTOM,
                Part::Modulation => ZOOM_ARC,
                Part::Meter => ZOOM_RIGHT,
                Part::Style => ZOOM_ALL,
            };
        }
        d
    }

    /// The Fine button, which the box knows without Bitwig.
    pub fn set_fine(&mut self, on: bool) {
        if on != self.fine {
            self.fine = on;
            self.dirty |= STATUS_BIT;
        }
    }

    /// Knob `k` turned at `now_ms`: show it large until it has been still for
    /// the zoom time. Returns false if it isn't shown (no zoom configured, no
    /// parameter, or Bitwig hasn't sent anything yet).
    pub fn touch(&mut self, k: u8, now_ms: u64) -> bool {
        if !self.live
            || self.zoom_tenths == 0
            || k as usize >= MAX_KNOBS
            || self.knobs[k as usize].name.is_empty()
        {
            return false;
        }
        self.zoom_until = now_ms + self.zoom_tenths as u64 * 100;
        if self.zoom != Some(k) {
            self.zoom = Some(k);
            self.dirty |= ZOOM_ALL;
        }
        true
    }

    /// End the zoom if its time is up.
    pub fn expire(&mut self, now_ms: u64) {
        if self.zoom.is_some() && now_ms >= self.zoom_until {
            self.end_zoom();
        }
    }

    /// When the zoom ends, if one is showing.
    pub fn zoom_deadline(&self) -> Option<u64> {
        self.zoom.map(|_| self.zoom_until)
    }

    pub fn zoomed(&self) -> Option<u8> {
        self.zoom
    }

    fn end_zoom(&mut self) {
        if self.zoom.take().is_some() {
            self.dirty |= CELLS | CELL_METERS;
        }
    }

    /// Everything needs drawing again (after the display is reset).
    pub fn invalidate(&mut self) {
        self.dirty = ALL | ZOOM_ALL;
    }

    /// The regions changed since the last call, as a mask for [`regions`]:
    /// the grid's or the zoom's, whichever is showing. (Ending the zoom
    /// redraws the whole grid, so grid changes during it aren't kept.)
    pub fn take_dirty(&mut self) -> u32 {
        let showing = if self.zoom.is_some() {
            TOP | ZOOM_ALL
        } else {
            ALL
        };
        core::mem::take(&mut self.dirty) & showing
    }

    pub fn knob(&self, k: usize) -> &Knob {
        &self.knobs[k]
    }

    /// Draw one region. `target` only needs to cover that region (or a band
    /// of it).
    pub fn render<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
        &self,
        region: Region,
        target: &mut D,
    ) {
        match region {
            Region::Header => self.render_header(target),
            Region::Status => self.render_status(target),
            Region::HeaderMeter => self.render_header_meter(target),
            Region::Knob(k) => self.render_knob(k, target),
            Region::KnobMeter(k) => self.render_knob_meter(k, target),
            Region::ZoomTop
            | Region::ZoomLeft
            | Region::ZoomArc
            | Region::ZoomRight
            | Region::ZoomBottom => self.render_zoom(region, target),
        }
    }

    fn accent(&self) -> Rgb565 {
        self.color.unwrap_or(DEFAULT_ACCENT)
    }

    fn knob_color(&self, k: usize) -> Rgb565 {
        self.knobs[k].color.unwrap_or(self.accent())
    }

    fn render_header<D: DrawTarget<Color = Rgb565, Error = Infallible>>(&self, d: &mut D) {
        let area = Region::Header.rect();
        let _ = d.fill_solid(&area, HEADER_BG);
        let _ = d.fill_solid(
            &Rectangle::new(area.top_left, Size::new(6, HEADER_H)),
            self.accent(),
        );
        let left = 12;
        let right = WIDTH as i32 - 4;
        if !self.live {
            text(
                d,
                "knobbox",
                Point::new(left, 2),
                &FONT_10X20,
                TEXT,
                Alignment::Left,
            );
            text(
                d,
                "waiting for Bitwig",
                Point::new(left, 21),
                &FONT_6X13,
                DIM_TEXT,
                Alignment::Left,
            );
            return;
        }
        text(
            d,
            fit(self.track.as_str(), right - left, 9),
            Point::new(left, 2),
            &FONT_9X15_BOLD,
            TEXT,
            Alignment::Left,
        );

        // Line 2: device on the left, page and "2/5" on the right.
        let mut page: Str<32> = Str::new();
        if let Some(i) = self.page_index {
            let _ = write!(
                page,
                "{} {}/{}",
                fit(self.page.as_str(), 16 * 6, 6),
                i + 1,
                self.page_count
            );
        }
        text(
            d,
            page.as_str(),
            Point::new(right, 19),
            &FONT_6X13,
            DIM_TEXT,
            Alignment::Right,
        );
        let device = if self.device.is_empty() {
            "no device"
        } else {
            self.device.as_str()
        };
        let device_px = right - left - (page.as_str().len() as i32 + 1) * 6;
        text(
            d,
            fit(device, device_px, 6),
            Point::new(left, 19),
            &FONT_6X13,
            TEXT,
            Alignment::Left,
        );
    }

    fn render_status<D: DrawTarget<Color = Rgb565, Error = Infallible>>(&self, d: &mut D) {
        let area = Region::Status.rect();
        let _ = d.fill_solid(&area, HEADER_BG);
        let _ = d.fill_solid(
            &Rectangle::new(area.top_left, Size::new(6, STATUS_H)),
            self.accent(),
        );
        if !self.live {
            return;
        }
        let y = area.top_left.y;
        // Play (a triangle) or stopped (a square), then a red dot when
        // recording, then the tempo.
        if self.status & STATUS_PLAYING != 0 {
            let _ = Triangle::new(
                Point::new(12, y + 3),
                Point::new(12, y + 11),
                Point::new(19, y + 7),
            )
            .into_styled(PrimitiveStyle::with_fill(PLAY))
            .draw(d);
        } else {
            let _ = d.fill_solid(
                &Rectangle::new(Point::new(12, y + 4), Size::new(7, 7)),
                DIM_TEXT,
            );
        }
        let mut x = 25;
        if self.status & STATUS_RECORDING != 0 {
            let _ = Circle::new(Point::new(x, y + 3), 9)
                .into_styled(PrimitiveStyle::with_fill(RECORD))
                .draw(d);
            x += 14;
        }
        if let Some(t) = self.tempo {
            let mut s: Str<16> = Str::new();
            let _ = write!(s, "{}.{} BPM", t / 10, t % 10);
            text(
                d,
                s.as_str(),
                Point::new(x, y + 1),
                &FONT_6X13,
                DIM_TEXT,
                Alignment::Left,
            );
        }
        // Badges from the right: FINE while it's held, LOCK while the device
        // is locked.
        let mut right = WIDTH as i32 - 4;
        for (on, label, colour) in [
            (self.fine, "FINE", FINE_BADGE),
            (self.status & STATUS_LOCKED != 0, "LOCK", LOCK_BADGE),
        ] {
            if on {
                let badge = Rectangle::new(Point::new(right - 30, y + 2), Size::new(30, 11));
                let _ = d.fill_solid(&badge, colour);
                text(
                    d,
                    label,
                    Point::new(right - 15, y + 3),
                    &FONT_6X10,
                    Rgb565::BLACK,
                    Alignment::Center,
                );
                right -= 34;
            }
        }
    }

    fn render_header_meter<D: DrawTarget<Color = Rgb565, Error = Infallible>>(&self, d: &mut D) {
        let area = Region::HeaderMeter.rect();
        let _ = d.fill_solid(&area, BG);
        if let (true, Some(l)) = (self.live, self.track_meter) {
            meter(d, area, l, false);
        }
    }

    fn render_knob<D: DrawTarget<Color = Rgb565, Error = Infallible>>(&self, k: u8, d: &mut D) {
        let area = Region::Knob(k).rect();
        let _ = d.fill_solid(&area, BG);
        let mid = area.top_left.x + CELL_W as i32 / 2;
        let top = area.top_left.y;
        let arc_tl = Point::new(mid - ARC_D as i32 / 2, top + ARC_TOP);
        ring(d, arc_tl, ARC_D, ARC_W, TRACK_RING, 1.0);

        if !self.live {
            // Start-up page: each box shows its knob's number, so the screen's
            // rotation can be checked against the panel.
            let mut n: Str<2> = Str::new();
            let _ = write!(n, "{}", k + 1);
            text(
                d,
                n.as_str(),
                Point::new(mid, arc_tl.y + ARC_D as i32 / 2 - 10),
                &FONT_10X20,
                TEXT,
                Alignment::Center,
            );
            return;
        }
        let knob = &self.knobs[k as usize];
        let colour = self.knob_color(k as usize);
        let name_at = Point::new(mid, top + 3);
        if knob.name.is_empty() {
            text(
                d,
                "-",
                name_at,
                &FONT_7X13_BOLD,
                DIM_TEXT,
                Alignment::Center,
            );
            self.draw_cell_meter(k, d);
            return;
        }
        // 13 px fonts: the panel is 27.7 mm across, so 13 px is about 1.5 mm tall.
        let name = fit(knob.name.as_str(), CELL_W as i32 - 4, 7);
        if knob.selected {
            let pill = Rectangle::new(
                Point::new(area.top_left.x + 2, top + 2),
                Size::new(CELL_W - 4, 15),
            );
            let _ = d.fill_solid(&pill, colour);
            text(
                d,
                name,
                name_at,
                &FONT_7X13_BOLD,
                Rgb565::BLACK,
                Alignment::Center,
            );
        } else {
            text(d, name, name_at, &FONT_7X13_BOLD, TEXT, Alignment::Center);
        }
        ring(d, arc_tl, ARC_D, ARC_W, colour, fraction(knob.value));
        if let Some(m) = knob.modulated {
            let inset = (ARC_W + 3) as i32;
            ring(
                d,
                arc_tl + Point::new(inset, inset),
                ARC_D - 2 * inset as u32,
                2,
                MOD_RING,
                m as f32 / MOD_MAX as f32,
            );
        }
        text(
            d,
            fit(knob.text.as_str(), CELL_W as i32, 6),
            Point::new(mid, arc_tl.y + ARC_D as i32 + 5),
            &FONT_6X13,
            DIM_TEXT,
            Alignment::Center,
        );
        self.draw_cell_meter(k, d);
    }

    fn render_knob_meter<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
        &self,
        k: u8,
        d: &mut D,
    ) {
        let _ = d.fill_solid(&Region::KnobMeter(k).rect(), BG);
        self.draw_cell_meter(k, d);
    }

    fn draw_cell_meter<D: DrawTarget<Color = Rgb565, Error = Infallible>>(&self, k: u8, d: &mut D) {
        if let (true, Some(l)) = (self.live, self.knobs[k as usize].meter) {
            meter(d, Region::KnobMeter(k).rect(), l, true);
        }
    }

    /// Every part of the zoom; `target` keeps only the region being drawn.
    fn render_zoom<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
        &self,
        region: Region,
        d: &mut D,
    ) {
        let _ = d.fill_solid(&region.rect(), BG);
        let Some(k) = self.zoom else {
            return;
        };
        let knob = &self.knobs[k as usize];
        let colour = self.knob_color(k as usize);
        let mid = WIDTH as i32 / 2;
        match region {
            Region::ZoomTop => {
                let at = Point::new(mid, Region::ZoomTop.rect().top_left.y + 5);
                let name = if knob.name.is_empty() {
                    "-"
                } else {
                    knob.name.as_str()
                };
                if knob.selected {
                    let w = (name.len() as u32 * 10 + 12).min(WIDTH);
                    let pill =
                        Rectangle::new(Point::new(mid - w as i32 / 2, at.y - 2), Size::new(w, 24));
                    let _ = d.fill_solid(&pill, colour);
                    text(d, name, at, &FONT_10X20, Rgb565::BLACK, Alignment::Center);
                } else {
                    text(d, name, at, &FONT_10X20, TEXT, Alignment::Center);
                }
            }
            Region::ZoomArc => {
                ring(d, ZOOM_ARC_TL, ZOOM_ARC_D, ZOOM_ARC_W, TRACK_RING, 1.0);
                if !knob.name.is_empty() {
                    ring(
                        d,
                        ZOOM_ARC_TL,
                        ZOOM_ARC_D,
                        ZOOM_ARC_W,
                        colour,
                        fraction(knob.value),
                    );
                }
                if let Some(m) = knob.modulated {
                    let inset = (ZOOM_ARC_W + 6) as i32;
                    ring(
                        d,
                        ZOOM_ARC_TL + Point::new(inset, inset),
                        ZOOM_ARC_D - 2 * inset as u32,
                        3,
                        MOD_RING,
                        m as f32 / MOD_MAX as f32,
                    );
                }
            }
            Region::ZoomRight => {
                if let Some(l) = knob.meter {
                    let _ = d.fill_solid(&ZOOM_METER, TRACK_RING);
                    meter(d, ZOOM_METER, l, true);
                }
            }
            Region::ZoomBottom => {
                // Twice the size of the 10 × 20 font: 12 characters fill the width.
                let top = Region::ZoomBottom.rect().top_left.y + 2;
                let s = fit(knob.text.as_str(), WIDTH as i32, 20);
                let w = s.len() as i32 * 20;
                let mut big = Scaled2 {
                    target: d,
                    origin: Point::new(mid - w / 2, top),
                };
                let origin = big.origin;
                text(&mut big, s, origin, &FONT_10X20, TEXT, Alignment::Left);
            }
            _ => {}
        }
    }
}

fn seven_bit_pair(hi: u8, lo: u8) -> u16 {
    ((hi as u16 & 0x7F) << 7) | (lo as u16 & 0x7F)
}

fn level(l: u8) -> Option<u8> {
    (l != NO_METER).then_some(l.min(METER_MAX))
}

fn fraction(value: u16) -> f32 {
    value.min(VALUE_MAX) as f32 / VALUE_MAX as f32
}

/// A knob's arc from its start, `fraction` of the way round.
fn ring<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
    d: &mut D,
    top_left: Point,
    diameter: u32,
    width: u32,
    colour: Rgb565,
    fraction: f32,
) {
    let sweep = ARC_SWEEP * fraction.clamp(0.0, 1.0);
    if sweep >= 1.0 {
        let _ = Arc::new(top_left, diameter, ARC_START.deg(), sweep.deg())
            .into_styled(PrimitiveStyle::with_stroke(colour, width))
            .draw(d);
    }
}

/// A level meter filling `area` from the bottom (`vertical`) or the left:
/// green, then yellow from 70 %, red from 90 %.
fn meter<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
    d: &mut D,
    area: Rectangle,
    level: u8,
    vertical: bool,
) {
    let len = if vertical {
        area.size.height
    } else {
        area.size.width
    };
    let filled = len * level.min(METER_MAX) as u32 / METER_MAX as u32;
    let bands = [
        (0, len * 7 / 10, METER_LOW),
        (len * 7 / 10, len * 9 / 10, METER_MID),
        (len * 9 / 10, len, METER_HIGH),
    ];
    for (from, to, colour) in bands {
        let to = to.min(filled);
        if to <= from {
            break;
        }
        let r = if vertical {
            let bottom = area.top_left.y + len as i32;
            Rectangle::new(
                Point::new(area.top_left.x, bottom - to as i32),
                Size::new(area.size.width, to - from),
            )
        } else {
            Rectangle::new(
                area.top_left + Point::new(from as i32, 0),
                Size::new(to - from, area.size.height),
            )
        };
        let _ = d.fill_solid(&r, colour);
    }
}

/// Bitwig's 7-bit colour channels to RGB565; too dark to see on black becomes
/// the default accent.
fn color(r: u8, g: u8, b: u8) -> Rgb565 {
    let (r, g, b) = (r & 0x7F, g & 0x7F, b & 0x7F);
    if r.max(g).max(b) < 24 {
        return DEFAULT_ACCENT;
    }
    Rgb565::new(r >> 2, g >> 1, b >> 2)
}

/// The start of an ASCII string that fits in `px` pixels of a `char_w` font.
fn fit(s: &str, px: i32, char_w: i32) -> &str {
    let n = (px / char_w).max(0) as usize;
    &s[..s.len().min(n)]
}

fn text<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
    d: &mut D,
    s: &str,
    at: Point,
    font: &'static embedded_graphics::mono_font::MonoFont<'static>,
    color: Rgb565,
    align: Alignment,
) {
    let style: TextStyle = TextStyleBuilder::new()
        .alignment(align)
        .baseline(Baseline::Top)
        .build();
    let _ = Text::with_text_style(s, at, MonoTextStyle::new(font, color), style).draw(d);
}

/// Draws everything twice the size, growing away from `origin`: the largest
/// built-in font is 10 × 20, and the zoom's value wants 20 × 40.
struct Scaled2<'a, D> {
    target: &'a mut D,
    origin: Point,
}

impl<D: DrawTarget<Color = Rgb565, Error = Infallible>> Dimensions for Scaled2<'_, D> {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(self.origin, Size::new(WIDTH, HEIGHT))
    }
}

impl<D: DrawTarget<Color = Rgb565, Error = Infallible>> DrawTarget for Scaled2<'_, D> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(p, c) in pixels {
            let q = self.origin + (p - self.origin) * 2;
            let _ = self
                .target
                .fill_solid(&Rectangle::new(q, Size::new(2, 2)), c);
        }
        Ok(())
    }
}

/// A rectangle of the screen in memory, as the display wants it: RGB565,
/// high byte first, rows top to bottom.
pub struct RegionBuf<'a> {
    area: Rectangle,
    buf: &'a mut [u8],
}

impl<'a> RegionBuf<'a> {
    pub fn new(area: Rectangle, buf: &'a mut [u8]) -> Self {
        let len = (area.size.width * area.size.height * 2) as usize;
        RegionBuf {
            area,
            buf: &mut buf[..len],
        }
    }

    pub fn area(&self) -> Rectangle {
        self.area
    }

    pub fn bytes(&self) -> &[u8] {
        self.buf
    }

    fn index(&self, p: Point) -> Option<usize> {
        let x = p.x - self.area.top_left.x;
        let y = p.y - self.area.top_left.y;
        let (w, h) = (self.area.size.width as i32, self.area.size.height as i32);
        ((0..w).contains(&x) && (0..h).contains(&y)).then(|| ((y * w + x) * 2) as usize)
    }
}

impl Dimensions for RegionBuf<'_> {
    fn bounding_box(&self) -> Rectangle {
        self.area
    }
}

impl DrawTarget for RegionBuf<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(p, c) in pixels {
            if let Some(i) = self.index(p) {
                let v = RawU16::from(c).into_inner();
                self.buf[i] = (v >> 8) as u8;
                self.buf[i + 1] = v as u8;
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        let a = area.intersection(&self.area);
        let v = RawU16::from(color).into_inner();
        let (hi, lo) = ((v >> 8) as u8, v as u8);
        for y in a.rows() {
            let Some(start) = self.index(Point::new(a.top_left.x, y)) else {
                continue;
            };
            for px in self.buf[start..start + a.size.width as usize * 2]
                .as_chunks_mut::<2>()
                .0
            {
                px[0] = hi;
                px[1] = lo;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knob_value(k: u8, v: u16, text: &str) -> Vec<u8> {
        let mut d = vec![k, (v >> 7) as u8, (v & 0x7F) as u8];
        d.extend_from_slice(text.as_bytes());
        d
    }

    fn live() -> Screen {
        let mut s = Screen::new();
        s.apply(CMD_KNOB_NAME, b"\x02Cutoff");
        s.take_dirty();
        s
    }

    #[test]
    fn first_message_redraws_everything() {
        let mut s = Screen::new();
        assert_eq!(s.take_dirty(), ALL); // start-up page
        assert_eq!(s.take_dirty(), 0);
        assert!(s.apply(CMD_KNOB_NAME, b"\x02Cutoff"));
        assert_eq!(s.take_dirty(), ALL);
        assert!(s.apply(CMD_KNOB_NAME, b"\x02Resonance"));
        assert_eq!(s.take_dirty(), 1 << 2);
    }

    #[test]
    fn only_changes_mark_regions() {
        let mut s = live();
        s.apply(CMD_TRACK, b"Bass");
        s.take_dirty();
        s.apply(CMD_TRACK, b"Bass");
        assert_eq!(s.take_dirty(), 0);
        s.apply(CMD_KNOB_VALUE, &knob_value(5, 8000, "-6.0 dB"));
        assert_eq!(s.take_dirty(), 1 << 5);
        s.apply(CMD_KNOB_VALUE, &knob_value(5, 8000, "-6.0 dB"));
        assert_eq!(s.take_dirty(), 0);
        assert_eq!(s.knob(5).value, 8000);
        assert_eq!(s.knob(5).text.as_str(), "-6.0 dB");
        // Knobs past the shown 8 are stored but draw nothing.
        s.apply(CMD_KNOB_NAME, b"\x0CDrive");
        assert_eq!(s.take_dirty(), 0);
        assert_eq!(s.knob(12).name.as_str(), "Drive");
        s.set_fine(true);
        assert_eq!(
            regions(s.take_dirty()).collect::<Vec<_>>(),
            [Region::Status]
        );
    }

    #[test]
    fn status_style_modulation_and_meters() {
        let mut s = live();
        assert!(s.apply(CMD_STATUS, &[STATUS_PLAYING | STATUS_LOCKED, 9, 48])); // 120.0
        assert_eq!(s.tempo, Some(1200));
        assert_eq!(s.take_dirty(), STATUS_BIT);
        assert!(s.apply(CMD_STATUS, &[0]));
        assert_eq!(s.tempo, None);
        s.take_dirty();

        assert!(s.apply(
            CMD_KNOB_STYLE,
            &[3, STYLE_COLOR | STYLE_SELECTED, 127, 0, 0]
        ));
        assert!(s.knob(3).selected && s.knob(3).color.is_some());
        assert_eq!(s.take_dirty(), 1 << 3);
        assert!(s.apply(CMD_KNOB_STYLE, &[3, 0]));
        assert!(!s.knob(3).selected && s.knob(3).color.is_none());
        s.take_dirty();

        assert!(s.apply(CMD_KNOB_MOD, &[1, 64]));
        assert_eq!(s.knob(1).modulated, Some(64));
        assert!(s.apply(CMD_KNOB_MOD, &[1]));
        assert_eq!(s.knob(1).modulated, None);
        assert_eq!(s.take_dirty(), 1 << 1);

        // Meters touch only the meter regions.
        assert!(s.apply(CMD_METERS, &[100, NO_METER, 20, NO_METER]));
        assert_eq!(s.track_meter, Some(100));
        assert_eq!(s.knob(1).meter, Some(20));
        assert_eq!(s.take_dirty(), HEADER_METER_BIT | 1 << 9);
        assert!(s.apply(CMD_METERS, &[100, NO_METER, 20, NO_METER]));
        assert_eq!(s.take_dirty(), 0);
    }

    #[test]
    fn zoom_follows_the_turned_knob_and_times_out() {
        let mut s = Screen::new();
        assert!(!s.touch(2, 0), "no zoom before Bitwig is heard from");
        s.apply(CMD_KNOB_NAME, b"\x02Cutoff");
        s.take_dirty();
        assert!(!s.touch(4, 0), "no zoom for a knob without a parameter");
        assert!(s.touch(2, 1000));
        assert_eq!(s.zoomed(), Some(2));
        assert_eq!(s.zoom_deadline(), Some(2000));
        assert_eq!(s.take_dirty(), ZOOM_ALL);
        // Turning again keeps it and pushes the deadline, without a redraw.
        assert!(s.touch(2, 1500));
        assert_eq!(s.zoom_deadline(), Some(2500));
        assert_eq!(s.take_dirty(), 0);
        // While zoomed: its value redraws the zoom's arc and value; other
        // knobs' changes wait for the grid.
        s.apply(CMD_KNOB_VALUE, &knob_value(2, 100, "1 kHz"));
        s.apply(CMD_KNOB_VALUE, &knob_value(6, 100, "x"));
        assert_eq!(s.take_dirty(), ZOOM_ARC | ZOOM_BOTTOM);
        s.apply(CMD_METERS, &[NO_METER, 0, 0, 50]);
        assert_eq!(s.take_dirty(), ZOOM_RIGHT);
        s.expire(2499);
        assert_eq!(s.zoomed(), Some(2));
        s.expire(2500);
        assert_eq!(s.zoomed(), None);
        assert_eq!(s.zoom_deadline(), None);
        assert_eq!(s.take_dirty(), CELLS | CELL_METERS);
        // Zoom time 0 turns it off.
        s.apply(CMD_CONFIG, &[0]);
        assert!(!s.touch(2, 3000));
        s.apply(CMD_CONFIG, &[5]);
        assert!(s.touch(2, 3000));
        assert_eq!(s.zoom_deadline(), Some(3500));
    }

    #[test]
    fn bad_messages_are_ignored() {
        let mut s = Screen::new();
        s.take_dirty();
        assert!(!s.apply(0x7E, b""));
        assert!(!s.apply(CMD_COLOR, &[1, 2]));
        assert!(!s.apply(CMD_KNOB_NAME, b"\x10x")); // knob 17
        assert!(!s.apply(CMD_KNOB_VALUE, &[0, 1]));
        assert!(!s.apply(CMD_STATUS, &[0, 1]));
        assert!(!s.apply(CMD_KNOB_STYLE, &[0, 1, 2]));
        assert!(!s.apply(CMD_KNOB_MOD, &[0, 1, 2]));
        assert!(!s.apply(CMD_METERS, &[0; 18]));
        assert!(!s.apply(CMD_CONFIG, &[]));
        assert_eq!(s.take_dirty(), 0);
    }

    #[test]
    fn strings_are_cut_and_cleaned() {
        let mut t: Str<4> = Str::new();
        assert!(t.set("Gain\u{b5}s".as_bytes()));
        assert_eq!(t.as_str(), "Gain");
        assert!(t.set(&[b'a', 0xC2, 0x0A]));
        assert_eq!(t.as_str(), "a??");
    }

    fn coverage(mask: u32) -> Vec<u8> {
        let mut covered = vec![0u8; (WIDTH * HEIGHT) as usize];
        for r in regions(mask) {
            for p in r.rect().points() {
                covered[(p.y as u32 * WIDTH + p.x as u32) as usize] += 1;
            }
        }
        covered
    }

    #[test]
    fn regions_tile_the_screen() {
        // The grid and the zoom each cover the screen exactly once…
        assert!(coverage(TOP | CELLS).iter().all(|&c| c == 1));
        assert!(coverage(TOP | ZOOM_ALL).iter().all(|&c| c == 1));
        // …and each knob's meter lies inside its box, clear of its arc.
        for k in 0..SHOWN_KNOBS as u8 {
            let cell = Region::Knob(k).rect();
            let m = Region::KnobMeter(k).rect();
            assert_eq!(cell.intersection(&m), m);
            let arc_right = cell.top_left.x + (CELL_W + ARC_D + ARC_W) as i32 / 2;
            assert!(m.top_left.x > arc_right);
        }
        assert_eq!(
            Region::ZoomRight.rect().intersection(&ZOOM_METER),
            ZOOM_METER
        );
        let arc = Rectangle::new(
            ZOOM_ARC_TL - Point::new(ZOOM_ARC_W as i32 / 2, ZOOM_ARC_W as i32 / 2),
            Size::new(ZOOM_ARC_D + ZOOM_ARC_W, ZOOM_ARC_D + ZOOM_ARC_W),
        );
        assert_eq!(Region::ZoomArc.rect().intersection(&arc), arc);
    }

    #[test]
    fn bands_cover_a_region_within_the_buffer() {
        for r in regions(ALL | ZOOM_ALL) {
            let area = r.rect();
            let mut rows = 0;
            for b in bands(area) {
                assert!((b.size.width * b.size.height * 2) as usize <= BUF_LEN);
                assert_eq!(b.top_left.y, area.top_left.y + rows as i32);
                rows += b.size.height;
            }
            assert_eq!(rows, area.size.height, "{r:?}");
        }
    }

    #[test]
    fn region_buffer_writes_big_endian_inside_only() {
        let mut buf = [0u8; 8];
        let area = Rectangle::new(Point::new(10, 20), Size::new(2, 2));
        let mut rb = RegionBuf::new(area, &mut buf);
        let c = Rgb565::new(31, 0, 0); // 0xF800
        rb.draw_iter([Pixel(Point::new(11, 21), c), Pixel(Point::new(0, 0), c)])
            .unwrap();
        assert_eq!(rb.bytes(), [0, 0, 0, 0, 0, 0, 0xF8, 0x00]);
        rb.fill_solid(&Rectangle::new(Point::new(0, 0), Size::new(11, 21)), c)
            .unwrap();
        assert_eq!(rb.bytes(), [0xF8, 0, 0, 0, 0, 0, 0xF8, 0x00]);
    }
}

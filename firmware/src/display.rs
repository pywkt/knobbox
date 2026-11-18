//! The 1.54" ST7789 colour screen (`hardware/components/st7789-154.md`), driven
//! from the Pico's second core so drawing never delays the knob scan.
//!
//! Core 0 updates [`SCREEN`] (from Bitwig's SysEx, the Fine button and knob
//! turns) and signals [`CHANGED`]; [`run`] then draws each changed region into
//! a buffer and sends it over SPI with DMA. The zoom (a turned knob shown
//! large) starts here, without waiting for Bitwig, and [`run`] ends it when
//! its time is up.
//!
//! The screen is optional. Nothing here waits on it (the SPI only transmits,
//! and the panel has no busy line), so without one the same firmware runs
//! the knobs and keys exactly as it does with one.

use core::cell::RefCell;

use embassy_futures::select::select;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};
use embedded_graphics::prelude::{Point, Size};
use embedded_graphics::primitives::Rectangle;
use knobbox_logic::screen::{BUF_LEN, HEIGHT, RegionBuf, Screen, WIDTH, bands, regions};

pub static SCREEN: Mutex<CriticalSectionRawMutex, RefCell<Screen>> =
    Mutex::new(RefCell::new(Screen::new()));
pub static CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Change the screen from core 0 and wake the drawing.
pub fn update(f: impl FnOnce(&mut Screen)) {
    SCREEN.lock(|s| f(&mut s.borrow_mut()));
    CHANGED.signal(());
}

/// Knob `k` (0-based) turned: show it large for a moment.
pub fn touch(k: u8) {
    update(|s| {
        s.touch(k, Instant::now().as_millis());
    });
}

/// Which way up the picture is, as the ST7789's MADCTL value plus the
/// column/row offset that rotation needs (the chip's memory is 240 × 320, the
/// panel 240 × 240).
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Rotation {
    /// Upright with the board's pin header at the top.
    R0,
    /// Turned 90° one way…
    R90,
    R180,
    /// …and 90° the other.
    R270,
}

impl Rotation {
    fn madctl(self) -> u8 {
        const MY: u8 = 0x80;
        const MX: u8 = 0x40;
        const MV: u8 = 0x20;
        match self {
            Rotation::R0 => 0,
            Rotation::R90 => MX | MV,
            Rotation::R180 => MX | MY,
            Rotation::R270 => MY | MV,
        }
    }

    fn offset(self) -> (u16, u16) {
        match self {
            Rotation::R180 => (0, 80),
            Rotation::R270 => (80, 0),
            _ => (0, 0),
        }
    }
}

/// The case holds the board on its side with the pin header toward the knobs
/// (+X), so the picture is turned to stand upright for someone sitting at
/// the front. If the start-up page (knob numbers 1–4 on top) is sideways or
/// upside down, try the other rotations here.
pub const ROTATION: Rotation = Rotation::R270;

/// IPS ST7789 panels show colours as negatives unless inversion is on.
const INVERT_COLOURS: bool = true;

/// Some panels have red and blue swapped; set this if Bitwig's blue tracks
/// come out orange.
const BGR: bool = false;

/// The ST7789 takes up to 62.5 MHz, but the screen hangs on 20 cm jumpers, so
/// stay well under it. A knob's box (11 KB) takes about 4 ms at this rate,
/// the zoom's arc and value together (49 KB) about 16 ms.
pub const SPI_HZ: u32 = 24_000_000;

mod cmd {
    pub const SWRESET: u8 = 0x01;
    pub const SLPOUT: u8 = 0x11;
    pub const NORON: u8 = 0x13;
    pub const INVON: u8 = 0x21;
    pub const DISPON: u8 = 0x29;
    pub const CASET: u8 = 0x2A;
    pub const RASET: u8 = 0x2B;
    pub const RAMWR: u8 = 0x2C;
    pub const MADCTL: u8 = 0x36;
    pub const COLMOD: u8 = 0x3A;
}

pub struct St7789 {
    spi: Spi<'static, SPI1, Async>,
    dc: Output<'static>,
}

impl St7789 {
    /// `spi` must be in mode 3 (clock idles high, data on the second edge):
    /// with CS tied low, that's the mode these panels follow.
    pub fn new(spi: Spi<'static, SPI1, Async>, dc: Output<'static>) -> Self {
        St7789 { spi, dc }
    }

    async fn command(&mut self, c: u8, data: &[u8]) {
        self.dc.set_low();
        let _ = self.spi.write(&[c]).await;
        if !data.is_empty() {
            self.dc.set_high();
            let _ = self.spi.write(data).await;
        }
    }

    pub async fn init(&mut self) {
        self.command(cmd::SWRESET, &[]).await;
        Timer::after_millis(150).await;
        self.command(cmd::SLPOUT, &[]).await;
        Timer::after_millis(120).await;
        self.command(cmd::COLMOD, &[0x55]).await; // 16 bits a pixel
        let bgr = if BGR { 0x08 } else { 0 };
        self.command(cmd::MADCTL, &[ROTATION.madctl() | bgr]).await;
        if INVERT_COLOURS {
            self.command(cmd::INVON, &[]).await;
        }
        self.command(cmd::NORON, &[]).await;
        Timer::after_millis(10).await;
        self.command(cmd::DISPON, &[]).await;
        Timer::after_millis(10).await;
    }

    /// Send a w × h rectangle of RGB565 pixels (high byte first) at (x, y).
    pub async fn blit(&mut self, x: u16, y: u16, w: u16, h: u16, pixels: &[u8]) {
        let (ox, oy) = ROTATION.offset();
        let (x0, y0) = (x + ox, y + oy);
        let (x1, y1) = (x0 + w - 1, y0 + h - 1);
        self.command(
            cmd::CASET,
            &[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8],
        )
        .await;
        self.command(
            cmd::RASET,
            &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8],
        )
        .await;
        self.command(cmd::RAMWR, pixels).await;
    }
}

/// The drawing loop, on core 1: clear the panel, then redraw whatever
/// changed, for ever.
#[embassy_executor::task]
pub async fn run(mut lcd: St7789) {
    let mut buf = [0u8; BUF_LEN];
    lcd.init().await;
    // Clear to black, a band at a time.
    let all = Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT));
    for band in bands(all) {
        let len = (band.size.width * band.size.height * 2) as usize;
        blit(&mut lcd, band, &buf[..len]).await;
    }
    SCREEN.lock(|s| s.borrow_mut().invalidate());
    loop {
        // Copy the state out: the lock is a critical section across both
        // cores, so it mustn't be held while drawing.
        let (dirty, screen, deadline) = SCREEN.lock(|s| {
            let mut s = s.borrow_mut();
            s.expire(Instant::now().as_millis());
            (s.take_dirty(), s.clone(), s.zoom_deadline())
        });
        if dirty == 0 {
            match deadline {
                Some(ms) => {
                    select(CHANGED.wait(), Timer::at(Instant::from_millis(ms))).await;
                }
                None => CHANGED.wait().await,
            }
            continue;
        }
        for region in regions(dirty) {
            for band in bands(region.rect()) {
                let len = {
                    let mut rb = RegionBuf::new(band, &mut buf);
                    screen.render(region, &mut rb);
                    rb.bytes().len()
                };
                blit(&mut lcd, band, &buf[..len]).await;
            }
        }
    }
}

async fn blit(lcd: &mut St7789, area: Rectangle, pixels: &[u8]) {
    let (x, y) = (area.top_left.x as u16, area.top_left.y as u16);
    let (w, h) = (area.size.width as u16, area.size.height as u16);
    lcd.blit(x, y, w, h, pixels).await;
}

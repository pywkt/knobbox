//! knobbox firmware for the Raspberry Pi Pico.
//!
//! Every millisecond: read the pins named in the layout, let
//! `knobbox_logic::Scanner` turn them into events, and queue those as USB-MIDI
//! packets. The box only reports what happened; the Bitwig script decides
//! what it does.
//!
//! The USB device has two parts:
//! - a MIDI port (`knobbox MIDI 1`), which sends the events and listens for
//!   SysEx: the "reboot into BOOTSEL" message (see `tools/bootsel.sh`) and
//!   what the screen shows (`knobbox_logic::sysex`);
//! - a serial port (`/dev/ttyACM*`) with a log line per event, for checking
//!   wiring without Bitwig: `cat /dev/ttyACM0`.
//!
//! The colour screen runs on the second core (`display.rs`).
//!
//! The onboard LED is on while the firmware runs and flickers on each event.

#![no_std]
#![no_main]

mod display;

use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

use embassy_executor::{Executor, Spawner};
use embassy_futures::join::join5;
use embassy_rp::gpio::{AnyPin, Input, Level, Output, Pull};
use embassy_rp::multicore::{Stack, spawn_core1};
use embassy_rp::peripherals::{DMA_CH0, USB};
use embassy_rp::spi::{self, Spi};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{bind_interrupts, dma, rom_data};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Ticker, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::class::midi::MidiClass;
use embassy_usb::{Builder, Config};
use knobbox_logic::layout::{Button, Layout, PICO_ONLY, SCREEN_GPIOS, Snapshot, Source};
use knobbox_logic::midi::Event;
use knobbox_logic::scanner::Scanner;
use knobbox_logic::sysex::{CMD_REBOOT, HELLO, SysexReader, ours, usb_packets};
use static_cell::StaticCell;

const LAYOUT: &Layout = &PICO_ONLY;

/// Every knob counts the other way. For a box wired with each encoder's A and
/// B swapped: `cargo run --release --features reverse-knobs`.
const REVERSE_KNOBS: bool = cfg!(feature = "reverse-knobs");

/// The Pico has 26 usable GPIOs, so a layout can't name more Pico pins.
const MAX_PICO_INPUTS: usize = 26;

/// The onboard LED of the (non-W) Pico.
const LED_GPIO: u8 = 25;

/// Most USB-MIDI packets (4 bytes each) sent in one USB transfer.
const PACKETS_PER_TRANSFER: usize = 12;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
});

static mut CORE1_STACK: Stack<8192> = Stack::new();
static CORE1_EXECUTOR: StaticCell<Executor> = StaticCell::new();

type PicoInputs = [Option<(u8, Input<'static>)>; MAX_PICO_INPUTS];

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::High);

    // Every Pico pin the layout names becomes an input with its pull-up on.
    let mut inputs: PicoInputs = [const { None }; MAX_PICO_INPUTS];
    for (slot, pin) in inputs
        .iter_mut()
        .zip(LAYOUT.pins().filter(|p| p.source == Source::Pico))
    {
        assert!(pin.bit != LED_GPIO && !SCREEN_GPIOS.contains(&pin.bit));
        // SAFETY: the layout's tests check each GPIO appears once, and the
        // only other pins this firmware touches are the LED and the screen's
        // (both asserted above) and USB (dedicated pins, not GPIOs).
        let gpio = unsafe { AnyPin::steal(pin.bit) };
        *slot = Some((pin.bit, Input::new(gpio, Pull::Up)));
    }

    // The screen: SPI1 on GP26 (clock) and GP27 (data), D/C on GP28, drawn
    // from core 1. CS is tied low on the board, which wants SPI mode 3.
    let mut spi_config = spi::Config::default();
    spi_config.frequency = display::SPI_HZ;
    spi_config.polarity = spi::Polarity::IdleHigh;
    spi_config.phase = spi::Phase::CaptureOnSecondTransition;
    let lcd_spi = Spi::new_txonly(p.SPI1, p.PIN_26, p.PIN_27, p.DMA_CH0, Irqs, spi_config);
    let lcd = display::St7789::new(lcd_spi, Output::new(p.PIN_28, Level::High));
    // SAFETY: CORE1_STACK is only ever borrowed here, once.
    spawn_core1(
        p.CORE1,
        unsafe { &mut *addr_of_mut!(CORE1_STACK) },
        move || {
            let executor = CORE1_EXECUTOR.init(Executor::new());
            executor.run(|spawner| spawner.spawn(display::run(lcd).unwrap()))
        },
    );

    // USB device.
    let driver = Driver::new(p.USB, Irqs);
    let mut config = Config::new(0x1209, 0x0001); // pid.codes test PID, private use
    config.manufacturer = Some("DIY");
    config.product = Some("knobbox");
    config.serial_number = Some("1");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    let mut logger_state = State::new();
    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [],
        &mut control_buf,
    );
    let midi = MidiClass::new(&mut builder, 1, 1, 64);
    let logger_class = CdcAcmClass::new(&mut builder, &mut logger_state, 64);
    let mut usb = builder.build();

    let log_fut = embassy_usb_logger::with_class!(1024, log::LevelFilter::Info, logger_class);
    let (mut midi_tx, mut midi_rx) = midi.split();

    // Packets waiting for USB. If nobody is reading (Bitwig closed), it fills
    // up and new events are dropped rather than stalling the scan.
    let queue: Channel<NoopRawMutex, [u8; 4], 32> = Channel::new();

    let scan = async {
        Timer::after_millis(2).await; // let the pull-ups settle
        let mut scanner = Scanner::new(LAYOUT, &read(&inputs)).reverse_knobs(REVERSE_KNOBS);
        let mut ticker = Ticker::every(Duration::from_millis(1));
        loop {
            ticker.next().await;
            scanner.tick(&read(&inputs), |event| {
                log_event(event);
                match event {
                    Event::Button { index, pressed } if index == Button::Fine as u8 => {
                        display::update(|s| s.set_fine(pressed));
                    }
                    Event::Encoder { index, .. } => display::touch(index),
                    _ => {}
                }
                let _ = queue.try_send(event.usb_packet());
                led.toggle();
            });
        }
    };

    let send = async {
        loop {
            midi_tx.wait_connection().await;
            queue.clear();
            log::info!("USB connected");
            // Ask the Bitwig script for everything the screen shows, in case
            // it was already running.
            usb_packets(&HELLO, |packet| {
                let _ = queue.try_send(packet);
            });
            if REVERSE_KNOBS {
                log::info!("knobs reversed (reverse-knobs feature)");
            }
            loop {
                let mut buf = [0u8; PACKETS_PER_TRANSFER * 4];
                buf[..4].copy_from_slice(&queue.receive().await);
                let mut len = 4;
                while len < buf.len() {
                    let Ok(packet) = queue.try_receive() else {
                        break;
                    };
                    buf[len..len + 4].copy_from_slice(&packet);
                    len += 4;
                }
                if midi_tx.write_packet(&buf[..len]).await.is_err() {
                    break;
                }
            }
            log::info!("USB disconnected");
        }
    };

    let receive = async {
        let mut reader = SysexReader::new();
        let mut buf = [0u8; 64];
        loop {
            midi_rx.wait_connection().await;
            while let Ok(n) = midi_rx.read_packet(&mut buf).await {
                for packet in buf[..n].as_chunks::<4>().0 {
                    let mut reboot = false;
                    if let Some((cmd, data)) = reader.feed(*packet).and_then(ours) {
                        if cmd == CMD_REBOOT {
                            reboot = true;
                        } else {
                            let mut known = true;
                            display::update(|s| known = s.apply(cmd, data));
                            if !known {
                                log::info!("unknown SysEx command {:02x}", cmd);
                            }
                        }
                    }
                    if reboot {
                        log::info!("rebooting into BOOTSEL");
                        Timer::after_millis(20).await; // let the log line out
                        // The LED blinks while the bootloader is active.
                        rom_data::reset_to_usb_boot(1 << LED_GPIO, 0);
                    }
                }
            }
        }
    };

    join5(usb.run(), log_fut, scan, send, receive).await;
}

fn read(inputs: &PicoInputs) -> Snapshot {
    let mut snap = Snapshot::IDLE;
    for (gpio, input) in inputs.iter().flatten() {
        if input.is_low() {
            snap.pico &= !(1 << gpio);
        }
    }
    snap
}

fn log_event(event: Event) {
    match event {
        Event::Encoder { index, delta } => log::info!("knob {} {:+}", index + 1, delta),
        Event::Switch { index, pressed } => {
            log::info!(
                "knob {} switch {}",
                index + 1,
                if pressed { "down" } else { "up" }
            )
        }
        Event::Button { index, pressed } => {
            log::info!(
                "button {} {}",
                index + 1,
                if pressed { "down" } else { "up" }
            )
        }
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    // No debug probe to report to. Unplug and replug to restart.
    loop {
        cortex_m::asm::nop();
    }
}

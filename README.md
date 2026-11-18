# knobbox

A DIY USB MIDI controller made only for [Bitwig Studio](https://www.bitwig.com/)
on Linux: endless knobs that follow whatever device you select, a few buttons to
move around, and a small colour screen that shows what each knob is doing.

A Raspberry Pi Pico runs Rust firmware that only reports what the knobs and
buttons do. A Bitwig controller script decides what they control, so changing
the behaviour means editing a text file, never reflashing the box.

## Status

A working prototype: **8 knobs, 7 buttons and a 1.54″ screen**, all
wired straight to the Pico, in a 3D-printed case. The full design adds a second
row of 8 knobs and push-to-reset on every knob through MCP23017 I/O expanders;
that is not built yet.

## What it does

**Devices** (the starting mode)

- Knobs 1–8 control the selected device's current Remote Controls page, and
  knobs 9–16 the page after it (once the second row exists).
- **Page ←/→**, **Device ←/→** and **Track ←/→** move through Remote Controls
  pages, devices and tracks. Everything remaps as you go, with no MIDI learn.

**Mixer** (Fine + Track ←/→ switches between the two)

- The knobs set the volume, pan or a send of 8 tracks. Page ←/→ picks which,
  and Device ←/→ moves to the 8 tracks before or after.

**Either mode**

- Hold **Fine** while turning for small steps.
- Double-tap **Fine** to reset the last knob you turned to its default. (Holding
  a knob down for half a second does the same once the knob switches are
  wired.)
- **Fine + Device ←/→** locks the knobs to this device, so clicking around
  Bitwig doesn't move them.
- Turning fast takes bigger steps (Bitwig's controller preferences:
  Acceleration).
- Bitwig pops up the track, device and page when they change. The box's screen
  shows them too, with each knob's parameter, value and modulation, the
  transport and tempo, and the track's level.

## What's in here

| Folder | What |
|---|---|
| `firmware/` | The Pico's firmware (Rust, [Embassy](https://embassy.dev/)): scans the pins every millisecond, sends MIDI over USB, draws the screen on the second core. |
| `logic/` | Everything the firmware decides that doesn't touch hardware (debouncing, encoder decoding, MIDI and SysEx encoding, the screen's drawing), so it can be tested on a PC. |
| `bitwig/` | The controller script, `knobbox.control.js`, and `test.js`, which checks it against a fake Bitwig. |
| `hardware/` | Wiring (`pico-only-wiring.md`), the printed case (`case/`, and `case-no-screen/` for a box without the screen), and the screen's notes. |
| `tools/` | `bootsel.sh`, which reboots a running box into flashing mode. |

## Building one

**Parts:** a Raspberry Pi Pico (RP2040) with headers, 8 EC11 rotary encoders,
7 MX-style key switches and keycaps, a 1.54″ 240×240 ST7789 SPI screen, jumper
wires, and 4 M3 screws. No resistors: the Pico's internal pull-ups are used.

1. **Print the case.** The STLs in `hardware/case/out/` are ready to print;
   [`hardware/case/README.md`](hardware/case/README.md) has the print settings,
   the full parts list and the assembly steps.
2. **Wire it** per [`hardware/pico-only-wiring.md`](hardware/pico-only-wiring.md).
3. **Flash the firmware.** You need Rust (stable, edition 2024), the Pico's
   target and the UF2 converter:

   ```sh
   rustup target add thumbv6m-none-eabi
   cargo install elf2uf2-rs
   ```

   Hold the Pico's BOOTSEL button while plugging it in, so it shows up as the
   `RPI-RP2` drive, then:

   ```sh
   cd firmware
   cargo run --release
   ```

   If every knob turns the wrong way, the encoders are wired with A and B
   swapped: flash with `cargo run --release --features reverse-knobs`
   instead. Once the firmware is running, `tools/bootsel.sh` (needs `amidi`,
   from alsa-utils) reboots the box into BOOTSEL for the next flash, without
   unplugging it.

4. **Install the script.** Put the `bitwig/` folder in Bitwig's controller
   scripts folder as `knobbox`, for example with a link:

   ```sh
   ln -s "$PWD/bitwig" ~/"Bitwig Studio/Controller Scripts/knobbox"
   ```

   Plug the box in. Bitwig finds it by its port name, `knobbox MIDI 1`; if
   not, add it under Settings → Controllers (vendor **DIY**, model
   **knobbox**). Bitwig reloads the script whenever the file is saved.

## Testing without the box

```sh
cd logic && cargo test                                  # the firmware's logic
node bitwig/test.js                                     # the Bitwig script
cd logic && cargo run --example preview -- screen.ppm   # draw the screen to images
```

With the box plugged in, `cat /dev/ttyACM0` prints a line per knob turn or
button press, which is handy for checking the wiring without Bitwig.

## More for Bitwig

Other Bitwig projects by the same author:

- [bitstep](https://github.com/pywkt/bitstep): a step sequencer that sends notes, with per-lane lengths and rates, 64 patterns and a song mode.
- [chopdrop](https://github.com/pywkt/chopdrop): a sample player laid out as lanes, with a chop editor, 16 outputs and time-stretching.
- [stuttter](https://github.com/pywkt/stuttter): a stutter, repeat and tape-stop effect built from step blocks.
- [Concord](https://github.com/pywkt/concord): play one key, hear a full chord, voiced and strummed.

## License

[ISC](LICENSE)

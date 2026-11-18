# Pico-only wiring: 8 knobs + 7 buttons + the screen

For use until the MCP23017 expanders arrive. Everything goes straight to the
Pico, and all 26 of its usable pins are taken: 8 encoders (16 pins), the 7
buttons, and the colour screen (3 pins). No knob has its push-switch wired,
so hold-to-reset waits for the expanders. (Until 2026-11-11 the screen's pins
were the switches of knobs 1–3, which were never wired.)

No resistors are needed. The Pico's internal pull-ups are used, and every
encoder contact, switch and button just connects its pin to **GND**. Anything
left unwired reads as "not pressed".

Pin numbers are the Pico's physical pins (pin 1 is top left with the USB
port at the top; the left side counts 1–20 down, the right side 21–40 up).
The pin table in the firmware is `PICO_ONLY` in `logic/src/layout.rs`.

## Encoders: down the left side

A bare EC11 has 3 legs on one side (**A, C, B**, with C the common in the
middle) and 2 on the other (the push-switch). Knob n drives Bitwig's
Remote Control n on the current page.

| | A | B | C (middle leg) |
|---|---|---|---|
| Knob 1 | GP0 (pin 1) | GP1 (pin 2) | GND (pin 3) |
| Knob 2 | GP2 (pin 4) | GP3 (pin 5) | GND |
| Knob 3 | GP4 (pin 6) | GP5 (pin 7) | GND (pin 8) |
| Knob 4 | GP6 (pin 9) | GP7 (pin 10) | GND |
| Knob 5 | GP8 (pin 11) | GP9 (pin 12) | GND (pin 13) |
| Knob 6 | GP10 (pin 14) | GP11 (pin 15) | GND |
| Knob 7 | GP12 (pin 16) | GP13 (pin 17) | GND (pin 18) |
| Knob 8 | GP14 (pin 19) | GP15 (pin 20) | GND |

All GND pins are connected inside the Pico. The easiest way is one GND rail
on the breadboard, fed from pin 3 or 8.

## Screen: 1.54" ST7789 (optional)

`hardware/components/st7789-154.md`. Female–female jumpers, about 20 cm, from
the screen's 8-pin header. Its `SCL`/`SDA` are SPI clock and data, not I2C.

The screen is optional, and there is only one firmware. The Pico only ever
sends to the screen and never waits for an answer, so with nothing on GP26–28
the pictures go nowhere and the knobs and keys work exactly as before. Plug
the screen in (with the USB cable out) and it shows up at the next power-up.

| Screen pin | Pico |
|---|---|
| GND | GND (pin 33) |
| VCC | 3V3 OUT (pin 36), **not** VBUS or VSYS |
| SCL | GP26 (pin 31) |
| SDA | GP27 (pin 32) |
| RST | 3V3 OUT (pin 36), with VCC |
| DC | GP28 (pin 34) |
| CS | GND |
| BL | 3V3 OUT (pin 36), with VCC |

Pin 36 feeds three wires (VCC, RST, BL): use a 1-to-3 splitter, or solder
them together. The same goes for GND.

At power-up the screen shows "knobbox / waiting for Bitwig" and the numbers
1–8 laid out like the knobs. Once the Bitwig script runs it shows the track,
device, page, and each knob's parameter, value and position.

## Buttons: right side (all optional)

One leg to the pin, the other to GND. Without a button, touching a jumper wire
from the pin to GND counts as a press.

| Button | Pin |
|---|---|
| Page ← | GP16 (pin 21) |
| Page → | GP17 (pin 22) |
| Device ← | GP18 (pin 24) |
| Device → | GP19 (pin 25) |
| Track ← | GP20 (pin 26) |
| Track → | GP21 (pin 27) |
| Fine (hold) | GP22 (pin 29) |

GND is next to them at pins 23 and 28.

## KY-040 modules

They label A as `CLK`, B as `DT`, and the switch as `SW`; wire `GND` to GND.
Their `+` pin feeds onboard pull-up resistors: connect it to **3V3(OUT),
pin 36**, or leave it unconnected. **Never connect it to VBUS or VSYS (5 V)**:
the Pico's pins only take 3.3 V.

## Checks

- **Every knob counts the wrong way:** A and B are swapped on all of them
  (the legs aren't marked, so this is easy to do). Leave the wires and flash
  with `cargo run --release --features reverse-knobs`. The serial log says
  "knobs reversed" when it's on.
- **One knob counts the wrong way:** swap its A and B wires, or swap the two
  numbers in its `pico_encoder(a, b)` line in the pin table.
- **One step per two clicks:** that encoder has twice as many clicks as
  pulses. Set its `resolution` to `Half` in the pin table.
- **Steps between clicks rather than on them:** harmless. The encoder rests
  with both contacts closed rather than open.
- **Screen stays black:** check VCC and BL are on 3V3 and GND is connected
  (the backlight alone should light it grey). Then check SCL/SDA aren't
  swapped.
- **Screen sideways or upside down:** the numbers 1–4 should be on top. Change
  `ROTATION` in `firmware/src/display.rs` and reflash.
- **Colours look like a photo negative:** set `INVERT_COLOURS` to `false` in
  the same file. **Blue tracks look orange:** set `BGR` to `true`.
- **Garbled or flickering picture:** lower `SPI_HZ` there (try 12 MHz). Long
  jumpers are the usual cause.
- **Screen stuck on "waiting for Bitwig":** the script needs its MIDI output.
  In Bitwig's Settings > Controllers, check the knobbox controller has
  `knobbox MIDI 1` as both input and output (re-add it if it only shows an
  input).

## When the expanders arrive

GP4/GP5 become the I2C bus, so knob 3 moves off them, and most encoders move
onto the expanders. That's a new pin table; nothing else
in the code changes.

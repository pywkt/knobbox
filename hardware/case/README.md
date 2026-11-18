# knobbox test case

A plain two-part printed box for the Pico-only prototype: 8 EC11 knobs, 7 MX keys, a 1.54″
colour screen and the Pico (`../pico-only-wiring.md`). It's built for testing, so it puts easy assembly and
rewiring ahead of looks. This file is how to build and print it.

**Size: 222.1 × 74.1 × 40 mm** (W × D × H). `model.py` works this out from `params.py`.

The STLs in `out/` are ready to print; run `model.py` only to change something. For a box
without the screen, `../case-no-screen/` keeps the round 2 box as it was (the firmware runs
with or without the screen).

## History

The case is modelled in code (build123d). Each round ran the checks, rendered pictures,
and was then printed and tried.

| Round | Date | What |
|---|---|---|
| 1 | 2026-11-10 | Layout as blocks (`out/round1_*.png`). Approved, with the knob pitch widened from 25 mm to 28. |
| 2 | 2026-11-10 | Printable parts: `base.stl`, `panel.stl`, `knob.stl` (`out/round2_*.png`). **No fit test**: the whole box was printed first, so the hole sizes below are chosen to forgive a first print. |
| 2, printed | 2026-11-11 | Printed and wired: everything fit and worked, no changes. (All the knobs counted backwards, a wiring swap fixed in firmware: `../pico-only-wiring.md` § Checks.) |
| 3 | 2026-11-11 | The 1.54″ ST7789 screen (`../components/st7789-154.md`) at the back left, in a window with a glass pocket and a locating frame; FINE moved in front of it; the box 17 mm wider (`out/round3_*.png`). Base and panel reprint; the knobs are unchanged. |
| 2, used | 2026-11-12 | Two notes from a day's use: the knobs sat close enough that turning one caught the next, and after a few presses a knob stopped clicking its encoder (it had crept down the shaft until it landed on the thread end first). |
| 3, knob | 2026-11-12 | New `knob.stl` only, for either box: Ø17 instead of Ø19 (11 mm between knobs), and it stops on the shaft's tip with 1.5 mm of air over the thread end and under the skirt, so a press always reaches the switch. The holes stay where they were (`KNOB_LAYOUT_D`): base and panel are byte for byte the round 3 ones. |
| 3, printed | 2026-11-12 | Printed, screen installed: everything fit and worked, no changes. The screen's glass and window guesses held. |
| 4 | | The new knobs' print report. |

- `params.py`: every dimension, named, with where it came from. Values marked **CHECK**
  are from memory, not calipers.
- `model.py`: builds the parts, runs `check()` (PASS/FAIL lines; any FAIL stops the
  renders), exports `out/`, then renders.
- `tools/render_layout.py`: the pictures (a painter's-algorithm renderer, adapted from an earlier case)
  and the dimensioned plan. `tools/render_stl.py`: a quick 3-view of any STL.

```sh
cd hardware/case
uv venv --python 3.12 .venv && uv pip install -p .venv/bin/python build123d numpy matplotlib scipy   # once
.venv/bin/python model.py      # about a minute
```

build123d needs Python ≤ 3.12. Ignore the Fontconfig warnings it prints: they come from
the system's font config and don't matter. The labels use
`/usr/share/fonts/noto/NotoSans-Bold.ttf` (`LABEL_FONT`).

## Printing

PLA, 0.2 mm layers, **no supports**, 3 perimeters, 15 % infill. The STLs are already in
print orientation; don't rotate them.

| File | Print | Orientation | Time* |
|---|---|---|---|
| `out/base.stl` | 1 | open side up, as exported | ~9 h 15 |
| `out/panel.stl` | 1 | **top face down** (ribs, frame and pockets facing up), as exported | ~3 h 15 |
| `out/knob.stl` | 8 | **top down**, as exported: no bridges, no supports | ~12 min each |

\*PrusaSlicer's defaults on the command line, not your printer profile. Expect your
profile to be quicker.

- The panel's labels are engraved 0.6 mm into its top face, which prints on the bed. The
  letters are gaps in the first three layers. PrusaSlicer warns "long bridging extrusions"
  because of them (round 3; round 2 also said "low bed adhesion"); the warning goes away
  with `LABELS = False`. The gaps are
  about 1 mm wide, so they bridge easily. If the letters still come out messy, set
  `LABELS = False` and reprint.
- The base's USB cutout has a 12 mm flat top, which the printer bridges.
- The screen window's edge is a 45° chamfer. It prints on the bed side, so it's a 45°
  overhang: no support.

## Hardware

- 4 × M3 screws, any length from 8 to 12 mm, pan or button head, driven straight into
  the plastic (no inserts). The pilot holes are 12 mm deep.
- 8 bare EC11 encoders with their nuts and washers; snap off the small anti-rotation tab
  beside the bushing with pliers.
- 7 MX-style switches and keycaps.
- The 1.54″ 240 × 240 ST7789 screen board (Hosyond, `../components/st7789-154.md`) and 8
  female-female jumper wires about 20 cm long (male pins at both ends: its header and the
  Pico's).
- Raspberry Pi Pico with headers soldered on: **black plastic strip on the plain side, pins
  pointing away from the chips**.
- Jumper wires cut in half, glue (hot-melt or superglue) for the Pico, hot glue for the
  screen.

## Assembly

1. **Pico.** Lay it chips-down on the four standoffs, USB end at the back wall, so the
   pins on the standoffs come through its mounting holes and the header pins point up. The USB
   socket lines up with the cutout in the back wall. Put a dab of glue on each pin. A
   paperclip through one of the two small holes in the floor presses BOOTSEL. Only one of
   them is under the button, because which side it lands on once flipped wasn't checked.
   Normally `tools/bootsel.sh` does this instead.
2. **Encoders.** Solder the half-jumpers to the legs first (A, C, B; the push-switch legs
   stay unwired: their Pico pins drive the screen now). Push each one up through its hole from underneath, then washer and nut on top.
   Point the 3-leg side toward the Pico and the wires stay short.
3. **Keys.** MX switches clip into the plate **from the top**: push them down through the
   square holes. Solder the wires afterwards from underneath (or before, threading the wires
   through first). Put the keycaps on.
4. **Screen.** With the panel upside down, drop the screen board into the frame at the back
   left, glass first, so the glass sits in its pocket and the picture shows through the
   window. **Header toward the knobs** (the firmware's picture rotation assumes it). It's
   held by nothing yet: put a blob of hot glue in each corner of the frame, over the board's
   edge, and let it set. Push the 8 female-female jumpers onto its header and wire it per
   `../pico-only-wiring.md` (screen section). To take it out again, peel the glue off.
   Or skip the glue: strips of polyimide (Kapton) tape across the frame hold it well enough
   and come off cleanly (what the prototype uses).
5. **Ground.** 15 legs go to GND (8 encoder middle legs, 7 keys), plus the screen's GND (and
   CS, which is tied low), more than the Pico's 8 GND pins (3, 8, 13, 18, 23, 28, 33, 38). Daisy-chain them under the
   panel with a bare or stripped wire, then run two or three jumpers from the chain to
   GND pins.
6. **Wire up** per `../pico-only-wiring.md`, female ends straight onto the header pins.
   Leave each wire about 15 cm long, so the panel can lie upside down beside the base while
   you wire it. It stays tethered by the wires whenever the lid is off.
7. **Knobs.** Line the D up with the shaft's flat and push them all the way on, until the
   shaft's tip stops them. About 1.5 mm of air stays under the skirt.
8. **Close.** The panel sits on top of the walls, and its ribs and the screen's frame drop
   just inside them, which lines it up. Then the four screws. Don't overtighten: they cut their own thread in PLA.

## If something doesn't fit

Change the parameter, run `model.py` again, and reprint only that part.

| Problem | Parameter (now) | Try |
|---|---|---|
| Switches won't go in / fall out | `MX_CUT` (14.1) | 14.2 / 14.0 |
| Encoder bushing won't pass | `ENC_HOLE_D` (8.0) | 8.3 |
| Screws too hard to drive / strip | `SCREW_PILOT_D` (2.6) | 2.8 / 2.4 |
| Knob too tight / loose on the shaft | `FIT_PRESS` (0.1) | 0.15 / 0.05 |
| Knob press doesn't click | the knob lands on the thread end or the panel first: `KNOB_NUT_GAP` / `KNOB_LIFT` (1.5 / 1.5) | 2.0 / 2.0 |
| Knob stops before it's down | the shaft's flat is shorter than `KNOB_FLAT_L` (6) | measure the flat; set `ENC_FLAT_L` and `KNOB_FLAT_L` 1 under it |
| Nut won't catch the thread | the bushing is shorter than `ENC_BUSHING_L` (5) | measure it: the panel needs to be thinner round the encoders, which `model.py` doesn't do yet |
| Pico won't sit on the standoffs at the USB end | `PICO_STANDOFF_D_USB` (3.6) | 3.2 |
| Letters print badly | `LABELS` (True) | False |
| Screen board won't go into its frame / rattles | `FIT_SLIDE` (0.3, also the ribs) or `SCREEN_PCB_L` / `SCREEN_PCB_W` (43.72 / 32.00) | measure the board; set those |
| Glass doesn't reach the window (board sits low, glass rides on the pocket edge) | the glass is bigger or sits elsewhere on the board than guessed: `SCREEN_GLASS_L` / `_W` (33.72 / 31.52), `SCREEN_GLASS_FROM_HEADER` (5.0) | measure them; or `SCREEN_POCKET_CLEAR` (0.5) → 1.0 |
| Board sits too high or too low (glass loose, or panel bulges) | `SCREEN_GLASS_T` (1.8) + `SCREEN_TAPE` (0.2), the glass and tape above the board | measure board front to glass top |
| Picture edge hidden / the window shows the glass's border | `SCREEN_ACTIVE_SHIFT` (1.1) or `SCREEN_WINDOW_MARGIN` (1.0) | look which side is cut off; shift, or margin 0.5 |

Other options: `SCREW_COUNTERSINK` for flat heads, `ENC_TAB_HOLE` to keep the encoders'
tabs (CHECK the slot position), `FEET_RECESS` for stick-on feet, `PICO_FIXING = "m2"` for
M2 self-tappers instead of glue pins, `RIBS`.

## CHECK values (from memory, not measured)

| What | Value | Matters for |
|---|---|---|
| Short-shaft EC11: bushing above the body | 5.0 | the nut (needs ≥ 4.5 with a 2 mm panel) |
| Body to shaft tip | 15.0 | where the knob stops: a shaft more than 0.5 shorter leaves too little air for a press to click (raise `KNOB_NUT_GAP` and `KNOB_LIFT` by the difference) |
| D flat length | 7.0 | the knob's D bore (6) must be shorter |
| Shaft Ø / across the flat | 6.0 / 4.5 | the knob's bore |
| Jumper housing length | 14 | bay height (35.5 of 38 used) |
| Header plastic on the Pico's plain side | yes | pins-up mount |
| BOOTSEL position | 12 from the USB end, 4 off centre | floor holes |
| Micro-USB socket width | 7.5 | the USB-end standoffs |
| Keycap width | 18 | keycap gaps, screw-head clearance |
| Screen glass outline (the usual 1.54″ panel) | 33.72 × 31.52 | the glass pocket, the window's bezel |
| Screen glass + backlight thickness; tape under it | 1.8; 0.2 | how high the board sits; the frame's glue lip |
| Glass from the board's header edge | 5.0 | where the window is |
| Active area from the glass centre, toward the header | 1.1 | where the window is |
| Screen header row from the board edge | 2.54 | the wire keep-out under it |
| Screen PCB thickness | 1.6 | the frame's depth |

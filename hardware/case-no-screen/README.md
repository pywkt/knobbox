# knobbox test case, without the screen

**A frozen copy.** This is the box as printed and used on 2026-11-11 (round 2), before the
screen. It's kept so that a box without the screen can still be printed, and it doesn't get
the later changes; the current case, with the screen, is in `../case/`. The firmware is the
same for both: it runs with or without the screen.

- **Ready to print:** `out/base.stl` and `out/panel.stl`, no Python needed.
- **Knobs:** print `../case/out/knob.stl`, not this copy's own. This copy's own knob has a fault that the newer one fixes: it creeps down the
  shaft until presses stop clicking. Push the new knobs all the way down, until the shaft's
  tip stops them.

A plain two-part printed box for the Pico-only prototype: 8 EC11 knobs, 7 MX keys and the
Pico (`../pico-only-wiring.md`). It's built for testing, so it puts easy assembly and
rewiring ahead of looks. This file is how to build and print it.

**Size: 205.1 × 74.1 × 40 mm** (W × D × H). `model.py` works this out from `params.py`.

## History

The case is modelled in code (build123d). Each round ran the checks, rendered pictures,
and was then printed and tried.

| Round | Date | What |
|---|---|---|
| 1 | 2026-11-10 | Layout as blocks (`out/round1_*.png`). Approved, with the knob pitch widened from 25 mm to 28. |
| 2 | 2026-11-10 | Printable parts: `base.stl`, `panel.stl`, `knob.stl` (`out/round2_*.png`). **No fit test**: the whole box was printed first, so the hole sizes below are chosen to forgive a first print. |
| 3 | 2026-11-11 | Printed and wired: everything fits and works, no changes. (All the knobs counted backwards, a wiring swap fixed in firmware: `../pico-only-wiring.md` § Checks.) |

- `params.py`: every dimension, named, with where it came from. Values marked **CHECK**
  are from memory, not calipers.
- `model.py`: builds the parts, runs `check()` (PASS/FAIL lines; any FAIL stops the
  renders), exports `out/`, then renders.
- `tools/render_layout.py`: the pictures (a painter's-algorithm renderer, adapted from an earlier case)
  and the dimensioned plan. `tools/render_stl.py`: a quick 3-view of any STL.

```sh
cd hardware/case-no-screen
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
| `out/base.stl` | 1 | open side up, as exported | ~9 h |
| `out/panel.stl` | 1 | **top face down** (ribs and pockets facing up), as exported | ~3 h 20 |
| `../case/out/knob.stl` | 8 | **top down**, as exported: no bridges, no supports | ~12 min each |

\*PrusaSlicer's defaults on the command line, not your printer profile. Expect your
profile to be quicker.

- The panel's labels are engraved 0.6 mm into its top face, which prints on the bed. The
  letters are gaps in the first three layers. PrusaSlicer warns "low bed adhesion, long
  bridging" because of them; that warning goes away with `LABELS = False`. The gaps are
  about 1 mm wide, so they bridge easily. If the letters still come out messy, set
  `LABELS = False` and reprint.
- The base's USB cutout has a 12 mm flat top, which the printer bridges.

## Hardware

- 4 × M3 screws, any length from 8 to 12 mm, pan or button head, driven straight into
  the plastic (no inserts). The pilot holes are 12 mm deep.
- 8 bare EC11 encoders with their nuts and washers; snap off the small anti-rotation tab
  beside the bushing with pliers.
- 7 MX-style switches and keycaps.
- Raspberry Pi Pico with headers soldered on: **black plastic strip on the plain side, pins
  pointing away from the chips**.
- Jumper wires cut in half, glue (hot-melt or superglue) for the Pico.

## Assembly

1. **Pico.** Lay it chips-down on the four standoffs, USB end at the back wall, so the
   pins on the standoffs come through its mounting holes and the header pins point up. The USB
   socket lines up with the cutout in the back wall. Put a dab of glue on each pin. A
   paperclip through one of the two small holes in the floor presses BOOTSEL. Only one of
   them is under the button, because which side it lands on once flipped wasn't checked.
   Normally `tools/bootsel.sh` does this instead.
2. **Encoders.** Solder the half-jumpers to the legs first (A, C, B; plus the switch legs on
   knobs 1–3). Push each one up through its hole from underneath, then washer and nut on top.
   Point the 3-leg side toward the Pico and the wires stay short.
3. **Keys.** MX switches clip into the plate **from the top**: push them down through the
   square holes. Solder the wires afterwards from underneath (or before, threading the wires
   through first). Put the keycaps on.
4. **Ground.** 18 legs go to GND (8 encoder middle legs, 3 knob switches, 7 keys), more
   than the Pico's 8 GND pins (3, 8, 13, 18, 23, 28, 33, 38). Daisy-chain them under the
   panel with a bare or stripped wire, then run two or three jumpers from the chain to
   GND pins.
5. **Wire up** per `../pico-only-wiring.md`, female ends straight onto the header pins.
   Leave each wire about 15 cm long, so the panel can lie upside down beside the base while
   you wire it. It stays tethered by the wires whenever the lid is off.
6. **Knobs** (from `../case/`). Line the D up with the shaft's flat and push them all the
   way on, until the shaft's tip stops them.
7. **Close.** The panel sits on top of the walls, and its ribs drop just inside them, which
   lines it up. Then the four screws. Don't overtighten: they cut their own thread in PLA.

## If something doesn't fit

Change the parameter, run `model.py` again, and reprint only that part.

| Problem | Parameter (now) | Try |
|---|---|---|
| Switches won't go in / fall out | `MX_CUT` (14.1) | 14.2 / 14.0 |
| Encoder bushing won't pass | `ENC_HOLE_D` (8.0) | 8.3 |
| Screws too hard to drive / strip | `SCREW_PILOT_D` (2.6) | 2.8 / 2.4 |
| Knob too tight / loose on the shaft | `FIT_PRESS` (0.1) | 0.15 / 0.05 |
| Knob stops before it's down | the shaft's flat is shorter than `KNOB_FLAT_L` (6) | measure the flat; set `ENC_FLAT_L` and `KNOB_FLAT_L` 1 under it |
| Nut won't catch the thread | the bushing is shorter than `ENC_BUSHING_L` (5) | measure it: the panel needs to be thinner round the encoders, which `model.py` doesn't do yet |
| Pico won't sit on the standoffs at the USB end | `PICO_STANDOFF_D_USB` (3.6) | 3.2 |
| Letters print badly | `LABELS` (True) | False |

Other options: `SCREW_COUNTERSINK` for flat heads, `ENC_TAB_HOLE` to keep the encoders'
tabs (CHECK the slot position), `FEET_RECESS` for stick-on feet, `PICO_FIXING = "m2"` for
M2 self-tappers instead of glue pins, `RIBS`.

## CHECK values (from memory, not measured)

| What | Value | Matters for |
|---|---|---|
| Short-shaft EC11: bushing above the body | 5.0 | the nut (needs ≥ 4.5 with a 2 mm panel) |
| Body to shaft tip | 15.0 | knob height only (the knob grips anywhere) |
| D flat length | 7.0 | the knob's D bore (6) must be shorter |
| Shaft Ø / across the flat | 6.0 / 4.5 | the knob's bore |
| Jumper housing length | 14 | bay height (35.5 of 38 used) |
| Header plastic on the Pico's plain side | yes | pins-up mount |
| BOOTSEL position | 12 from the USB end, 4 off centre | floor holes |
| Micro-USB socket width | 7.5 | the USB-end standoffs |
| Keycap width | 18 | keycap gaps, screw-head clearance |

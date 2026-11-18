"""knobbox test case model (build123d): 8 EC11 knobs, 7 MX keys and the Pico.

Round 1 (2026-11-10) placed every part as a block of its real outline; the layout was
approved with a 28 mm knob pitch. Round 2 makes the printable parts: the base (Pico
standoffs with glue pins, USB cutout, BOOTSEL holes, corner bosses with pilot holes), the
panel (encoder holes, MX cutouts in 1.5 mm pockets, screw holes, engraved labels, stiffening
ribs) and the knob (D bore, grip flutes). No fit test: the whole box was printed first
(2026-11-10), so the hole sizes are chosen to forgive a first print (README).

Run:

    .venv/bin/python model.py      # builds, checks, exports out/, renders out/round2_*.png

Frame: Z up, X across the front (left to right as you sit at it), Y front to back, origin
at the centre of the footprint on the table. All numbers come from
params.py; the box size comes out of layout().
"""
from __future__ import annotations

import math
import os
import sys
from dataclasses import dataclass

from build123d import (Align, Axis, Box, Compound, Cone, Cylinder, GeomType, Part, Pos, RegularPolygon, Rot,
                       Text, chamfer, export_step, export_stl, extrude)

import params as P

ROUND = "round2"

# ---- the stack in Z ----------------------------------------------------------------------
Z_FLOOR = P.FLOOR                       # floor top
Z_UNDER = P.FLOOR + P.INSIDE_H          # panel underside = top of the walls
Z_TOP = Z_UNDER + P.PANEL_T             # panel top face
CASE_H = Z_TOP
Z_PLATE_UNDER = Z_TOP - P.MX_PLATE_T    # underside of the key pockets


# ---- layout (spec "Panel layout"): the box size is computed here, never typed in ------------
@dataclass
class Layout:
    W: float
    D: float
    fine: tuple[float, float]
    knobs: dict[int, tuple[float, float]]           # knob number -> centre
    nav: dict[str, tuple[float, float]]             # "PAGE<" ... -> centre
    bay: tuple[float, float, float, float]          # x0, x1, y0, y1
    pico: tuple[float, float]                       # board centre
    notes: list[str]


def layout() -> Layout:
    notes = []
    # X, measured from the left outer edge first
    fine_x = P.EDGE_MARGIN
    k1 = fine_x + P.KEYCAP_W / 2 + P.GROUP_GAP + P.KNOB_D / 2
    kx = [k1 + i * P.KNOB_PITCH for i in range(P.KNOB_COLS)]
    bay_x0 = kx[-1] + P.ENC_WIRE_ZONE / 2 + P.BAY_CLEAR
    bay_x1 = bay_x0 + P.BAY_W
    nav_x = [bay_x1 + P.BAY_CLEAR + P.MX_BODY / 2]
    nav_x.append(nav_x[0] + P.NAV_PITCH)
    W = nav_x[1] + P.EDGE_MARGIN
    # Y: the nav block is the tallest group; its corner keys must clear the corner bosses
    nav_span = (len(P.NAV_ROWS) - 1) * P.NAV_PITCH
    nav_margin = max(P.EDGE_MARGIN, P.BOSS_INSET + P.BOSS_D / 2 + P.CORNER_CLEAR + P.MX_BODY / 2)
    knob_span = (P.KNOB_ROWS - 1) * P.KNOB_PITCH
    D = max(nav_span + 2 * nav_margin, knob_span + 2 * P.EDGE_MARGIN, P.BAY_L + 2 * P.WALL)
    notes.append(f"depth set by the nav keys' corner clearance: row centres {nav_margin:.1f} from the front and back edges")
    ox = -W / 2
    fine = (fine_x + ox, 0.0)
    knobs = {}
    for r in range(P.KNOB_ROWS):            # row 0 is the back row (knobs 1-4)
        y = knob_span / 2 - r * P.KNOB_PITCH
        for c in range(P.KNOB_COLS):
            knobs[r * P.KNOB_COLS + c + 1] = (kx[c] + ox, y)
    nav = {}
    for r, name in enumerate(P.NAV_ROWS):   # from the back
        y = nav_span / 2 - r * P.NAV_PITCH
        nav[f"{name}<"] = (nav_x[0] + ox, y)
        nav[f"{name}>"] = (nav_x[1] + ox, y)
    y_back = D / 2 - P.WALL
    bay = (bay_x0 + ox, bay_x1 + ox, y_back - P.BAY_L, y_back)
    pico = ((bay[0] + bay[1]) / 2, y_back - P.PICO_WALL_GAP - P.PICO_L / 2)
    return Layout(W, D, fine, knobs, nav, bay, pico, notes)


L = layout()
KEYS = {"FINE": L.fine, **L.nav}


# ---- helpers -------------------------------------------------------------------------------
def block(lx: float, ly: float, lz: float, cx: float, cy: float, z0: float) -> Part:
    """Box centred on (cx, cy) in plan, from z0 up."""
    return Pos(cx, cy, z0 + lz / 2) * Box(lx, ly, lz)


def zcyl(d: float, h: float, cx: float, cy: float, z0: float) -> Part:
    return Pos(cx, cy, z0 + h / 2) * Cylinder(d / 2, h)


def hex_nut(af: float, h: float, hole_d: float, cx: float, cy: float, z0: float) -> Part:
    ac = af / (3 ** 0.5 / 2)    # corner-to-corner of the true hex
    return (Pos(cx, cy, z0) * extrude(RegularPolygon(ac / 2, 6), h)).cut(zcyl(hole_d, h + 2, cx, cy, z0 - 1))


def to_bed(p: Part, flip: bool = False) -> Part:
    """The part as it goes on the bed: flipped upside down if asked, centred, resting on z = 0."""
    if flip:
        p = Rot(180, 0, 0) * p
    bb = p.bounding_box()
    return Pos(-(bb.min.X + bb.max.X) / 2, -(bb.min.Y + bb.max.Y) / 2, -bb.min.Z) * p


@dataclass
class Placed:
    part: Part
    kind: str           # colour and grouping in the renders
    group: str          # "panel" (hangs from or sits on the panel), "base", "zone" (keep-out)
    lift: int = 0       # exploded view: 0 with its group, 1 a further step up (knobs, keycaps)


def labels() -> list[tuple[str, float, float, str]]:
    """(text, x, y, anchor) of the engraved labels; anchor "c" centres the text on (x, y), "r"
    ends it there."""
    if not P.LABELS:
        return []
    half = P.KEYCAP_W / 2 + P.LABEL_GAP + P.LABEL_H / 2
    out = [("FINE", L.fine[0], L.fine[1] + half, "c")]
    for name in P.NAV_ROWS:
        x, y = L.nav[f"{name}<"]
        out.append((name, x - P.KEYCAP_W / 2 - P.LABEL_GAP, y, "r"))
    back = P.NAV_ROWS[0]
    for arrow in "<>":
        x, y = L.nav[f"{back}{arrow}"]
        out.append((arrow, x, y + half, "c"))
    return out


def label_solids(depth: float, z0: float) -> list[Part]:
    """The labels as solids `depth` tall from z0, capitals LABEL_H tall."""
    cap = Text("E", font_size=10.0, font_path=P.LABEL_FONT).bounding_box().size.Y / 10.0   # cap height per font size
    size = P.LABEL_H / cap
    out = []
    for text, x, y, anchor in labels():
        align = (Align.CENTER if anchor == "c" else Align.MAX, Align.CENTER)
        out.append(Pos(x, y, z0) * extrude(Text(text, font_size=size, font_path=P.LABEL_FONT, align=align), depth))
    return out


# ---- the parts -------------------------------------------------------------------------------
ENC_BODY_Z0 = Z_UNDER - P.ENC_BODY_H
ENC_BUSH_TOP = Z_UNDER + P.ENC_BUSHING_L
ENC_TIP = Z_UNDER + P.ENC_SHAFT_TIP
KNOB_Z0 = Z_TOP + P.KNOB_LIFT
KNOB_HOLLOW_TOP = ENC_BUSH_TOP + P.KNOB_NUT_GAP
KNOB_BORE_TOP = ENC_TIP + P.KNOB_BORE_EXTRA
KNOB_TOP = KNOB_BORE_TOP + P.KNOB_CAP
KNOB_H = KNOB_TOP - KNOB_Z0

MX_BODY_Z0 = Z_TOP - P.MX_BELOW_PLATE
MX_TOP_Z = Z_TOP + P.MX_ABOVE_PLATE
CAP_TOP = Z_TOP + P.KEYCAP_TOP

PICO_Z0 = Z_FLOOR + P.PICO_STANDOFF_H   # board's chip side (now facing down)
PICO_Z1 = PICO_Z0 + P.PICO_T            # plain side, headers on it, pins up
JUMPER_Z0 = PICO_Z1 + P.HEADER_PLASTIC_H
JUMPER_Z1 = JUMPER_Z0 + P.JUMPER_H
WIRES_TOP = JUMPER_Z1 + P.WIRE_BEND
USB_Z = PICO_Z0 - P.USB_RECEPT_H / 2    # receptacle centre, under the board


def encoder(x: float, y: float) -> Part:
    body = block(P.ENC_BODY_X, P.ENC_BODY_Y, P.ENC_BODY_H, x, y, ENC_BODY_Z0)
    bushing = zcyl(P.ENC_BUSHING_D, P.ENC_BUSHING_L, x, y, Z_UNDER)
    shaft = zcyl(P.ENC_SHAFT_D, P.ENC_SHAFT_TIP - P.ENC_BUSHING_L, x, y, ENC_BUSH_TOP)
    # the flat: the shaft keeps ENC_SHAFT_FLAT measured from its round (-Y) side, over ENC_FLAT_L from the tip
    flat = block(P.ENC_SHAFT_D, P.ENC_SHAFT_D, P.ENC_FLAT_L + 1, x, y + P.ENC_SHAFT_FLAT, ENC_TIP - P.ENC_FLAT_L)
    return body.fuse(bushing).fuse(shaft.cut(flat))


def make_knob() -> Part:
    """The knob in its own frame: axis +Z, skirt (underside) at z = 0, the D flat toward +Y."""
    r = P.KNOB_D / 2
    k = Pos(0, 0, KNOB_H / 2) * Cylinder(r, KNOB_H)
    k = chamfer(k.edges().filter_by(GeomType.CIRCLE).sort_by(Axis.Z)[-1], P.KNOB_CHAMFER)
    for i in range(P.KNOB_FLUTES):
        a = 2 * math.pi * i / P.KNOB_FLUTES
        rc = r + P.KNOB_FLUTE_D / 2 - P.KNOB_FLUTE_DEPTH
        k = k.cut(zcyl(P.KNOB_FLUTE_D, KNOB_H + 2, rc * math.cos(a), rc * math.sin(a), -1))
    hollow_top = KNOB_HOLLOW_TOP - KNOB_Z0
    bore_top = KNOB_BORE_TOP - KNOB_Z0
    k = k.cut(zcyl(P.KNOB_HOLLOW_D, hollow_top + 1, 0, 0, -1))
    bore_d = P.ENC_SHAFT_D + 2 * P.FIT_PRESS
    bore = zcyl(bore_d, bore_top - hollow_top + 0.5, 0, 0, hollow_top - 0.5)
    flat_y = P.ENC_SHAFT_FLAT - P.ENC_SHAFT_D / 2 + P.FIT_PRESS     # the bore's flat face
    bore = bore.cut(block(bore_d + 2, bore_d, P.KNOB_FLAT_L, 0, flat_y + bore_d / 2, bore_top - P.KNOB_FLAT_L))
    return k.cut(bore)


KNOB = make_knob()


def switch(x: float, y: float) -> Part:
    body = block(P.MX_BODY, P.MX_BODY, P.MX_BELOW_PLATE, x, y, MX_BODY_Z0)
    return body.fuse(block(P.MX_TOP_W, P.MX_TOP_W, P.MX_ABOVE_PLATE, x, y, Z_TOP))


def keycap(x: float, y: float) -> Part:
    outer = block(P.KEYCAP_W, P.KEYCAP_W, P.KEYCAP_H, x, y, CAP_TOP - P.KEYCAP_H)
    inner = P.KEYCAP_W - 2 * P.KEYCAP_WALL
    return outer.cut(block(inner, inner, P.KEYCAP_H, x, y, CAP_TOP - P.KEYCAP_H - P.KEYCAP_WALL))


def pico_holes() -> list[tuple[float, float]]:
    px, py = L.pico
    return [(px + sx * P.PICO_HOLE_X / 2, py + sy * P.PICO_HOLE_Y / 2) for sx in (-1, 1) for sy in (-1, 1)]


def standoff_d(hy: float) -> float:
    """The two standoffs at the USB end are thinner: the receptacle sits between them."""
    return P.PICO_STANDOFF_D_USB if hy > L.pico[1] else P.PICO_STANDOFF_D


def bootsel_holes() -> list[tuple[float, float]]:
    px, py = L.pico
    y = py + P.PICO_L / 2 - P.BOOTSEL_FROM_USB
    return [(px - P.BOOTSEL_OFF_AXIS, y), (px + P.BOOTSEL_OFF_AXIS, y)]


def build_parts() -> dict[str, Placed]:
    parts: dict[str, Placed] = {}
    for n, (x, y) in L.knobs.items():
        parts[f"enc{n}"] = Placed(encoder(x, y), "encoder", "panel")
        parts[f"nut{n}"] = Placed(hex_nut(P.ENC_NUT_AF, P.ENC_NUT_H, P.ENC_BUSHING_D, x, y, Z_TOP), "nut", "panel")
        parts[f"knob{n}"] = Placed(Pos(x, y, KNOB_Z0) * KNOB, "knob", "panel", lift=1)
        parts[f"enc{n}_wires"] = Placed(block(P.ENC_WIRE_ZONE, P.ENC_WIRE_ZONE, P.ENC_BELOW - P.ENC_BODY_H, x, y, Z_UNDER - P.ENC_BELOW),
                                        "zone", "zone")
    for name, (x, y) in KEYS.items():
        parts[f"sw_{name}"] = Placed(switch(x, y), "switch", "panel")
        parts[f"cap_{name}"] = Placed(keycap(x, y), "keycap", "panel", lift=1)
        parts[f"sw_{name}_pins"] = Placed(block(P.MX_BODY, P.MX_BODY, P.MX_BELOW - (Z_PLATE_UNDER - MX_BODY_Z0), x, y, Z_PLATE_UNDER - P.MX_BELOW),
                                          "zone", "zone")
    px, py = L.pico
    board = block(P.PICO_W, P.PICO_L, P.PICO_T, px, py, PICO_Z0)
    for hx, hy in pico_holes():
        board = board.cut(zcyl(P.PICO_HOLE_D, P.PICO_T + 2, hx, hy, PICO_Z0 - 1))
    parts["pico"] = Placed(board, "pico", "base")
    parts["pico_chips"] = Placed(block(P.PICO_CHIPS_W, P.PICO_CHIPS_L, P.PICO_CHIPS_H, px, py, PICO_Z0 - P.PICO_CHIPS_H), "chips", "base")
    usb_y = py + P.PICO_L / 2 + P.USB_OVERHANG - P.USB_RECEPT_L / 2
    parts["pico_usb"] = Placed(block(P.USB_RECEPT_W, P.USB_RECEPT_L, P.USB_RECEPT_H, px, usb_y, PICO_Z0 - P.USB_RECEPT_H), "usb", "base")
    for side, sx in (("L", -1), ("R", 1)):
        hx = px + sx * P.HEADER_ROW_X / 2
        parts[f"header_{side}"] = Placed(block(P.HEADER_W, P.HEADER_L, P.HEADER_PLASTIC_H, hx, py, PICO_Z1), "header", "base")
        parts[f"jumpers_{side}"] = Placed(block(P.HEADER_W, P.HEADER_L, P.JUMPER_H, hx, py, JUMPER_Z0), "jumper", "base")
    x0, x1, y0, y1 = L.bay
    parts["bay"] = Placed(Pos((x0 + x1) / 2, (y0 + y1) / 2, (Z_FLOOR + Z_UNDER) / 2) * Box(x1 - x0, y1 - y0, Z_UNDER - Z_FLOOR),
                          "bay", "zone")
    return parts


def screw_axes() -> list[tuple[float, float]]:
    ax, ay = L.W / 2 - P.BOSS_INSET, L.D / 2 - P.BOSS_INSET
    return [(sx * ax, sy * ay) for sx in (-1, 1) for sy in (-1, 1)]


def rib_xs() -> list[float]:
    """Cross ribs: between FINE's switch and knob column 1, and between the knob columns."""
    cols = sorted({x for x, _ in L.knobs.values()})
    first = ((L.fine[0] + P.MX_BODY / 2) + (cols[0] - P.ENC_WIRE_ZONE / 2)) / 2
    return [first] + [(a + b) / 2 for a, b in zip(cols, cols[1:])]


def ribs() -> list[Part]:
    if not P.RIBS:
        return []
    z0 = Z_UNDER - P.RIB_H
    half_y = L.D / 2 - P.WALL - P.FIT_SLIDE
    out = [block(P.RIB_T, 2 * half_y, P.RIB_H, x, 0, z0) for x in rib_xs()]
    x_end = L.bay[0] - P.BAY_CLEAR      # the long rib between the rows stops short of the bay
    x_start = rib_xs()[0]
    out.append(block(x_end - x_start, P.RIB_T, P.RIB_H, (x_start + x_end) / 2, 0, z0))
    return out


def build_shells() -> dict[str, Part]:
    W, D = L.W, L.D
    # ---- base: prints open side up, floor on the bed
    base = block(W, D, Z_UNDER, 0, 0, 0)
    base = base.cut(block(W - 2 * P.WALL, D - 2 * P.WALL, Z_UNDER, 0, 0, Z_FLOOR))
    for x, y in screw_axes():
        base = base.fuse(zcyl(P.BOSS_D, P.INSIDE_H, x, y, Z_FLOOR))
    for hx, hy in pico_holes():
        base = base.fuse(zcyl(standoff_d(hy), P.PICO_STANDOFF_H, hx, hy, Z_FLOOR))
        if P.PICO_FIXING == "pins":
            base = base.fuse(zcyl(P.PICO_PIN_D, P.PICO_PIN_H, hx, hy, PICO_Z0))
        else:
            base = base.cut(zcyl(P.PICO_M2_PILOT_D, P.PICO_M2_PILOT_DEPTH + 1, hx, hy, PICO_Z0 - P.PICO_M2_PILOT_DEPTH))
    for x, y in screw_axes():
        base = base.cut(zcyl(P.SCREW_PILOT_D, P.SCREW_PILOT_DEPTH + 1, x, y, Z_UNDER - P.SCREW_PILOT_DEPTH))
    base = base.cut(Pos(L.pico[0], D / 2 - P.WALL / 2, USB_Z) * Box(P.USB_CUT_W, P.WALL + 2, P.USB_CUT_H))
    for x, y in bootsel_holes():
        base = base.cut(zcyl(P.BOOTSEL_HOLE_D, P.FLOOR + 2, x, y, -1))
    if P.FEET_RECESS:
        fx, fy = W / 2 - P.FEET_INSET, D / 2 - P.FEET_INSET
        for sx in (-1, 1):
            for sy in (-1, 1):
                base = base.cut(zcyl(P.FEET_D, P.FEET_DEPTH + 1, sx * fx, sy * fy, -1))
    base.label = "base"

    # ---- panel: prints top face down; everything below is on its underside, facing up
    panel = block(W, D, P.PANEL_T, 0, 0, Z_UNDER)
    for r in ribs():
        panel = panel.fuse(r)
    for x, y in L.knobs.values():
        panel = panel.cut(zcyl(P.ENC_HOLE_D, P.PANEL_T + 2, x, y, Z_UNDER - 1))
        if P.ENC_TAB_HOLE:
            a = math.radians(P.ENC_TAB_ANGLE)
            sx, sy = P.ENC_TAB_SLOT
            slot = block(sx, sy, P.PANEL_T - P.ENC_TAB_SKIN + 1, 0, 0, Z_UNDER - 1)
            panel = panel.cut(Pos(x + P.ENC_TAB_R * math.cos(a), y + P.ENC_TAB_R * math.sin(a), 0) * Rot(0, 0, P.ENC_TAB_ANGLE - 90) * slot)
    for x, y in KEYS.values():
        panel = panel.cut(block(P.MX_CUT, P.MX_CUT, P.PANEL_T + 2, x, y, Z_UNDER - 1))
        panel = panel.cut(block(P.MX_POCKET, P.MX_POCKET, Z_PLATE_UNDER - Z_UNDER + 1, x, y, Z_UNDER - 1))
    for x, y in screw_axes():
        panel = panel.cut(zcyl(P.SCREW_CLEAR_D, P.PANEL_T + 2, x, y, Z_UNDER - 1))
        if P.SCREW_COUNTERSINK:
            h = (P.SCREW_HEAD_D - P.SCREW_CLEAR_D) / 2      # 90 degree countersink
            panel = panel.cut(Pos(x, y, Z_TOP - h) * Cone(P.SCREW_CLEAR_D / 2, P.SCREW_HEAD_D / 2, h, align=(Align.CENTER, Align.CENTER, Align.MIN)))
    # letter by letter: cutting a whole word at once fails where two letters touch (TRACK)
    for t in label_solids(P.LABEL_DEPTH + 1, Z_TOP - P.LABEL_DEPTH):
        for glyph in t.solids():
            before = panel.volume
            panel = panel.cut(glyph)
            removed = before - panel.volume
            expect = glyph.volume * P.LABEL_DEPTH / (P.LABEL_DEPTH + 1)
            assert abs(removed - expect) < 0.05 * expect + 0.01, f"label cut went wrong: removed {removed:.2f}, expected {expect:.2f}"
    panel.label = "panel"
    return {"base": base, "panel": panel}


# ---- checks (spec "check()") -----------------------------------------------------------------
def overlap(a: Part, b: Part) -> float:
    ba, bb = a.bounding_box(), b.bounding_box()
    if (ba.min.X >= bb.max.X or bb.min.X >= ba.max.X or ba.min.Y >= bb.max.Y or bb.min.Y >= ba.max.Y
            or ba.min.Z >= bb.max.Z or bb.min.Z >= ba.max.Z):
        return 0.0
    r = a.intersect(b)
    if r is None:
        return 0.0
    return sum(s.volume for s in r) if isinstance(r, list) else r.volume


def check(parts: dict[str, Placed], shells: dict[str, Part]) -> bool:
    ok = True

    def line(good: bool, msg: str) -> None:
        nonlocal ok
        ok &= good
        print(f"{'PASS' if good else 'FAIL'} {msg}")

    names = [n for n in parts if n != "bay"]
    hits = [(a, b, v) for i, a in enumerate(names) for b in names[i + 1:] if (v := overlap(parts[a].part, parts[b].part)) > 1e-3]
    for a, b, v in hits:
        line(False, f"{a} intersects {b} ({v:.1f} mm3)")
    if not hits:
        line(True, f"no two parts intersect ({len(names)} parts and keep-out zones; knobs with their real bore)")

    hits = [(n, sn, v) for n in names for sn, s in shells.items() if (v := overlap(parts[n].part, s)) > 1e-3]
    for n, sn, v in hits:
        line(False, f"{n} intersects the {sn} ({v:.1f} mm3)")
    if not hits:
        line(True, "every part and wire zone clears the walls, bosses, standoffs, glue pins, panel and ribs")

    bay = parts["bay"].part
    hits = [(n, v) for n in names if parts[n].group in ("panel", "zone") and (v := overlap(parts[n].part, bay)) > 1e-3]
    v = overlap(shells["panel"], bay)
    if v > 1e-3:
        hits.append(("the panel (a rib?)", v))
    for n, v in hits:
        line(False, f"{n} hangs into the Pico bay ({v:.1f} mm3)")
    if not hits:
        x0, x1, y0, y1 = L.bay
        line(True, f"Pico bay {x1 - x0:.1f} wide x {y1 - y0:.1f} long is free of anything from the panel, ribs included")
    bb = bay.bounding_box()
    out = [n for n, p in parts.items() if p.group == "base" and not (
        bb.min.X <= p.part.bounding_box().min.X and p.part.bounding_box().max.X <= bb.max.X
        and bb.min.Y <= p.part.bounding_box().min.Y)]   # (the USB receptacle may pass the back end, into its cutout)
    line(not out, f"the Pico and its jumpers lie inside the bay{': ' + ', '.join(out) if out else ''}")
    line(WIRES_TOP <= Z_UNDER, f"jumper tops {JUMPER_Z1:.1f} + {P.WIRE_BEND:.0f} wire bend = {WIRES_TOP:.1f} <= panel underside {Z_UNDER:.1f}")

    # plan view: caps and keycaps clear each other; labels clear caps, keycaps, nuts and screw heads
    tall = 200.0
    tops = {f"knob{n}": zcyl(P.KNOB_D, tall, x, y, -100) for n, (x, y) in L.knobs.items()}
    tops |= {f"cap_{k}": block(P.KEYCAP_W, P.KEYCAP_W, tall, x, y, -100) for k, (x, y) in KEYS.items()}
    tn = list(tops)
    hits = [(a, b) for i, a in enumerate(tn) for b in tn[i + 1:] if overlap(tops[a], tops[b]) > 1e-3]
    for a, b in hits:
        line(False, f"{a} and {b} overlap in plan view")
    if not hits:
        line(True, f"knob caps and keycaps clear each other in plan view (knob gap {P.KNOB_PITCH - P.KNOB_D:.1f}, keycap gap {P.NAV_PITCH - P.KEYCAP_W:.2f})")
    heads = {f"screw head {i}": zcyl(P.SCREW_HEAD_D, tall, x, y, -100) for i, (x, y) in enumerate(screw_axes())}
    head_gap = min(max(abs(sy) - P.SCREW_HEAD_D / 2 - (abs(ky) + P.KEYCAP_W / 2), abs(sx) - P.SCREW_HEAD_D / 2 - (abs(kx) + P.KEYCAP_W / 2))
                   for sx, sy in screw_axes() for kx, ky in KEYS.values() if sx * kx > 0 and sy * ky > 0 and abs(ky) > 1)
    hits = [n for n, h in heads.items() for t in tops.values() if overlap(h, t) > 1e-3]
    line(not hits, f"screw heads (Ø{P.SCREW_HEAD_D}) clear the keycaps and knobs in plan view; nearest keycap {head_gap:.2f} away")
    texts = label_solids(1.0, 0.0)
    hits = [(t, n) for t, s in zip(labels(), texts) for n, o in (tops | heads).items() if overlap(s, o) > 1e-3]
    for (t, *_), n in hits:
        line(False, f"label {t!r} runs under {n}")
    if not hits:
        line(True, f"{len(texts)} labels clear the caps, keycaps and screw heads")

    fine_gap = (L.knobs[1][0] - P.KNOB_D / 2) - (L.fine[0] + P.KEYCAP_W / 2)
    nav_gap = (L.nav["PAGE<"][0] - P.KEYCAP_W / 2) - (L.knobs[4][0] + P.KNOB_D / 2)
    line(min(fine_gap, nav_gap) >= P.GROUP_GAP - 1e-6, f"group gaps: FINE to knobs {fine_gap:.1f}, knobs to nav keys {nav_gap:.1f} (>= {P.GROUP_GAP})")
    centres = [*L.knobs.values(), *KEYS.values()]
    m = min(min(L.W / 2 - abs(x), L.D / 2 - abs(y)) for x, y in centres)
    line(m >= P.EDGE_MARGIN - 1e-6, f"hole centres at least {m:.1f} from the panel edge (>= {P.EDGE_MARGIN})")

    above = P.ENC_BUSHING_L - P.PANEL_T
    line(above >= P.ENC_NUT_H, f"bushing stands {above:.1f} above the panel for the {P.ENC_NUT_H} nut and washer (CHECK the bushing)")
    line(P.KNOB_FLAT_L < P.ENC_FLAT_L, f"knob bore is D-shaped for {P.KNOB_FLAT_L}, shorter than the shaft's {P.ENC_FLAT_L} flat")
    line(P.KNOB_HOLLOW_D > P.ENC_NUT_AC + 1.0, f"knob hollow Ø{P.KNOB_HOLLOW_D} clears the nut's corners ({P.ENC_NUT_AC}) by {(P.KNOB_HOLLOW_D - P.ENC_NUT_AC) / 2:.2f} a side")
    bite = P.SCREW_LEN - P.PANEL_T
    line(4.0 <= bite <= P.SCREW_PILOT_DEPTH - 1.0, f"M3 x {P.SCREW_LEN:.0f} bites {bite:.0f} into the boss; pilot {P.SCREW_PILOT_DEPTH:.0f} deep takes up to M3 x {P.SCREW_PILOT_DEPTH + P.PANEL_T - 1:.0f}")
    wall = (P.BOSS_D - P.SCREW_PILOT_D) / 2
    line(wall >= 2.0, f"boss wall {wall:.2f} around the Ø{P.SCREW_PILOT_D} pilot")
    pin_play = (P.PICO_HOLE_D - P.PICO_PIN_D) / 2
    line(P.PICO_PIN_H > P.PICO_T and pin_play > 0, f"Pico glue pins Ø{P.PICO_PIN_D} in Ø{P.PICO_HOLE_D} holes, {P.PICO_PIN_H - P.PICO_T:.1f} proud of the board")
    usb_bot, usb_top = USB_Z - P.USB_CUT_H / 2, USB_Z + P.USB_CUT_H / 2
    line(usb_bot >= Z_FLOOR - 1e-6 and usb_top <= Z_UNDER, f"USB cutout z {usb_bot:.1f}..{usb_top:.1f}, above the floor top {Z_FLOOR}")

    line(L.W <= P.MAX_FOOTPRINT[0] and L.D <= P.MAX_FOOTPRINT[1], f"footprint {L.W:.1f} x {L.D:.1f} under {P.MAX_FOOTPRINT[0]:.0f} x {P.MAX_FOOTPRINT[1]:.0f}")
    for n, s in printables(shells).items():
        bb = s.bounding_box()
        sz = bb.size
        fits = sz.X <= P.BED[0] and sz.Y <= P.BED[1] and sz.Z <= P.BED[2]
        line(fits and abs(bb.min.Z) < 1e-6, f"{n}.stl {sz.X:.1f} x {sz.Y:.1f} x {sz.Z:.1f} as it lies on the bed, fits {P.BED[0]:.0f} x {P.BED[1]:.0f}")
    return ok


def printables(shells: dict[str, Part]) -> dict[str, Part]:
    """Each printable part in print orientation (README "Printing")."""
    return {
        "base": to_bed(shells["base"]),                 # open side up
        "panel": to_bed(shells["panel"], flip=True),     # top face down
        "knob": to_bed(KNOB, flip=True),                 # top down: no bridges, no supports
    }


def report(parts: dict[str, Placed], shells: dict[str, Part]) -> None:
    print(f"box {L.W:.1f} x {L.D:.1f} x {CASE_H:.1f} (W x D x H); inside {L.W - 2 * P.WALL:.1f} x {L.D - 2 * P.WALL:.1f} x {P.INSIDE_H}")
    for n in L.notes:
        print(" ", n)
    print(f"  FINE at x {L.fine[0]:.1f}; knob columns x {', '.join(f'{L.knobs[c][0]:.1f}' for c in range(1, 5))}; rows y ±{P.KNOB_PITCH / 2}")
    print(f"  bay x {L.bay[0]:.1f}..{L.bay[1]:.1f}, y {L.bay[2]:.1f}..{L.bay[3]:.1f}; nav columns x {L.nav['PAGE<'][0]:.1f}, {L.nav['PAGE>'][0]:.1f}")
    print(f"  knob Ø{P.KNOB_D} x {KNOB_H:.1f}, top {KNOB_TOP - Z_TOP:.1f} above the panel; hollow Ø{P.KNOB_HOLLOW_D} x {KNOB_HOLLOW_TOP - KNOB_Z0:.1f}, "
          f"bore Ø{P.ENC_SHAFT_D + 2 * P.FIT_PRESS:.1f} x {KNOB_BORE_TOP - KNOB_HOLLOW_TOP:.1f} (D-shaped for the last {P.KNOB_FLAT_L})")
    print(f"  Pico board z {PICO_Z0:.1f}..{PICO_Z1:.1f} (chips down), jumpers to {JUMPER_Z1:.1f}, wires to {WIRES_TOP:.1f}; USB centre z {USB_Z:.1f}")
    print(f"  ribs {P.RIB_H:.0f} deep under the panel at x {', '.join(f'{x:.1f}' for x in rib_xs())} and along y 0" if P.RIBS else "  no ribs")
    for n, s in shells.items():
        print(f"  {n}: {s.volume / 1000:.1f} cm3, ~{s.volume / 1000 * 1.24:.0f} g if solid PLA")


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "out")
    os.makedirs(out, exist_ok=True)
    parts = build_parts()
    shells = build_shells()
    report(parts, shells)
    print()
    good = check(parts, shells)
    export_step(Compound(children=[p.part for p in parts.values() if p.group != "zone"] + list(shells.values())),
                os.path.join(out, "case.step"))
    for n, s in printables(shells).items():
        export_stl(s, os.path.join(out, f"{n}.stl"))
    print(f"\nwrote {out}/case.step, base.stl, panel.stl (top face down), knob.stl (print 8)")
    if not good:
        print("check() failed: fix before rendering")
        sys.exit(1)
    sys.path.insert(0, os.path.join(here, "tools"))
    import render_layout  # noqa: E402

    render_layout.render_all(parts, shells, L, out, ROUND)

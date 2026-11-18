"""Every dimension the test-case model uses, in mm, with where it comes from: the case's design brief ("spec"), an earlier printed case
by pywkt ("pywkt"), a datasheet, or
CHECK (from memory or a guess; measure before the print that needs it).
model.py has no bare numbers.

Frame: Z up, X across the front (left to right as you sit at it),
Y from the front edge to the back. Origin at the centre of the footprint on the table, so
the front face is at y = -CASE_D/2 and the floor's underside at z = 0.

Only the inputs live here. The box size comes out of the layout (model.py computes it and
prints it); it is never typed in.
"""

# ---- print and fit (as pywkt) -----------------------------------------------------------
LAYER = 0.2
FIT_SLIDE = 0.3     # parts that slide in
FIT_PRESS = 0.1     # press fits
PART_GAP = 1.0      # least air check() wants between parts that are not meant to touch

# ---- box (spec "Box") ----------------------------------------------------------------------
WALL = 2.0          # side walls, as pywkt
FLOOR = 2.0
PANEL_T = 2.0       # the lid; 1.5 in the key pockets (MX_PLATE_T)
INSIDE_H = 36.0     # floor top to panel underside, set by the Pico bay (BAY_H below)
BOSS_D = 8.0        # 4 corner bosses, full inside height, merged into the corner walls
BOSS_INSET = 5.0    # boss axis from both outer faces: the boss overlaps each wall by 1,
                    # and the Ø3.4 panel hole keeps 3.3 of plastic to the panel's edge
SCREW_PILOT_D = 2.6     # M3 thread-forming screw straight into the plastic (pywkt: 2.6 worked;
                        # the fit test tries 2.4 / 2.6 / 2.8 / 3.0)
SCREW_PILOT_DEPTH = 12.0
SCREW_CLEAR_D = 3.4     # panel holes, plain for pan heads
SCREW_COUNTERSINK = False   # True for countersunk heads (round 2)
SCREW_HEAD_D = 5.5      # M3 pan head (any M3 on hand, 8 to 12 long)
SCREW_LEN = 8.0         # the shortest that works: 2 through the panel, 6 into the boss

# ---- layout rules (spec "Panel layout") -------------------------------------------------
EDGE_MARGIN = 12.0      # least distance from a hole centre to the panel's outer edge
GROUP_GAP = 12.0        # least air between groups, measured between caps and keycaps
CORNER_CLEAR = 2.0      # air between a switch body and a corner boss (2 from round 2: keeps the
                        # screw heads 1.25 clear of the corner keycaps in plan, not 0.25)
BAY_CLEAR = 2.0         # air between the bay and the nearest body hanging from the panel

# ---- EC11 encoder, bare (spec "EC11 encoder") -------------------------------------------
ENC_HOLE_D = 8.0        # panel hole for the M7 bushing (pywkt: 7.5 printed too small)
ENC_BODY_X = 11.83      # metal body (pywkt sheet 11.65 x 11.83; nominal 12 x 12)
ENC_BODY_Y = 11.65
ENC_BODY_H = 7.0        # body face to the bottom of the body (EC11 nominal, pywkt)
ENC_BUSHING_D = 7.0     # M7 x 0.75 thread
ENC_BUSHING_L = 5.0     # body face to the end of the thread. CHECK: as of 2026-11-10, these are
                        # the "short shaft" EC11s (not pywkt's 20 mm ones); 5 is the short end of
                        # what is sold, so the nut check is honest. pywkt had ~6.5.
ENC_SHAFT_D = 6.0       # D shaft. CHECK (pywkt: 5.94, 4.47 across the flat)
ENC_SHAFT_FLAT = 4.5    # across the flat. CHECK
ENC_FLAT_L = 7.0        # length of the D flat, from the tip. CHECK
ENC_SHAFT_TIP = 15.0    # body face to the shaft tip. CHECK: the "15 mm" short-shaft EC11 (pywkt's
                        # were 6.5 + 12.4 = 18.9, the "20 mm" kind)
ENC_NUT_AF = 10.0       # nut across flats (pywkt)
ENC_NUT_AC = 11.5       # across corners (pywkt)
ENC_NUT_H = 2.5         # nut + washer (pywkt)
ENC_BELOW = 22.0        # keep clear below the panel: body, legs and soldered wires (spec)
ENC_WIRE_ZONE = 13.0    # square the legs and wire ends fill under the body (body + solder blobs)
ENC_TAB_HOLE = False    # anti-rotation tab: snapped off (spec default). True: a blind slot
ENC_TAB_SLOT = (2.5, 1.5)   # in the underside, CHECK
ENC_TAB_R = 7.5             # from the shaft axis, CHECK
ENC_TAB_ANGLE = 90.0        # where the slot goes round the hole, degrees from +X (90 = toward the back)
ENC_TAB_SKIN = 1.0          # panel left over the slot

# ---- printed knob (printed rather than bought, 2026-11-10) -------------------------------
KNOB_D = 19.0           # <= 20 (spec); 6 mm between caps at the 25 pitch
KNOB_LIFT = 1.0         # the knob's skirt above the panel
KNOB_HOLLOW_D = 13.0    # clears the nut (11.5 across corners)
KNOB_NUT_GAP = 0.5      # air above the thread end inside the hollow
KNOB_BORE_EXTRA = 0.3   # bore deeper than the shaft reaches
KNOB_CAP = 2.0          # closed top; the bore is blind
KNOB_FLAT_L = 6.0       # the bore is D-shaped only this far down from its blind end, round below,
                        # so it never needs the shaft's flat to be longer than this (CHECK < ENC_FLAT_L)
KNOB_CHAMFER = 0.8      # on the top edge, which prints on the bed (no elephant's foot)
KNOB_FLUTES = 18        # grip grooves round the side; 0 for a plain cylinder
KNOB_FLUTE_D = 2.0
KNOB_FLUTE_DEPTH = 0.6

# ---- knob grid (spec) -------------------------------------------------------------------
KNOB_COLS = 4
KNOB_ROWS = 2
KNOB_PITCH = 28.0       # both ways. The spec said 25; widened to 28 after round 1 (2026-11-10) (9 mm between Ø19 caps instead of 6)
KNOBS_WITH_SWITCH = (1, 2, 3)   # hold to reset (pico-only-wiring.md)

# ---- MX key switch (spec "MX key switch") -----------------------------------------------
MX_BODY = 14.0          # switch housing below the plate, square (Cherry MX drawing)
MX_CUT = 14.1           # square cutout in the plate. Nominal 14.0; 14.1 because there is no fit
                        # test (2026-11-10) and printed holes come out a little small.
                        # Too loose: 14.0. Too tight: 14.2, or file the four sides.
MX_PLATE_T = 1.5        # plate at the cutout, which the clips need
MX_POCKET = 16.0        # square pocket out of the panel underside around each cutout
MX_BELOW_PLATE = 5.0    # housing below the plate's top face (Cherry MX drawing)
MX_ABOVE_PLATE = 6.6    # housing above the plate's top face (11.6 tall overall)
MX_TOP_W = 15.6         # upper housing, square
MX_BELOW = 20.0         # keep clear below the plate: body, pins and soldered wires (spec)
KEYCAP_W = 18.0         # 1u, on hand (spec ~18)
KEYCAP_H = 8.0          # CHECK, only for the pictures
KEYCAP_TOP = 12.0       # keycap top above the plate's top face, CHECK (pictures only)
KEYCAP_WALL = 1.0       # keycaps are hollow; the upper housing sits inside
NAV_PITCH = 19.05       # standard key spacing (spec)
NAV_ROWS = ("PAGE", "DEVICE", "TRACK")   # from the back (spec)

# ---- Raspberry Pi Pico (spec "Raspberry Pi Pico"; Pico datasheet) ------------------------
PICO_L = 51.0
PICO_W = 21.0
PICO_T = 1.0
PICO_HOLE_D = 2.1
PICO_HOLE_X = 11.4      # across
PICO_HOLE_Y = 47.0      # along
PICO_CHIPS_H = 2.0      # parts on the chip side (RP2040, flash, BOOTSEL), now facing the floor. CHECK
PICO_CHIPS_L = 38.0     # the zone they fill, inside the standoffs
PICO_CHIPS_W = 15.0
PICO_STANDOFF_H = 6.0   # spec says 5; 6 lifts the USB cutout's bottom edge clear of the floor
                        # top (model.py prints both edges), so the floor is not notched
PICO_STANDOFF_D = 4.5   # <= 4.5: the end header pins are close (spec)
PICO_STANDOFF_D_USB = 3.6   # the two at the USB end: the receptacle (7.5 wide) sits between
                            # holes 11.4 apart, leaving 1.95 a side. CHECK on the board
PICO_PIN_D = 1.8        # through the board's hole, then glue (PICO_FIXING = "pins")
PICO_PIN_H = 2.0
PICO_FIXING = "pins"    # or "m2": a pilot hole for an M2 self-tapper
PICO_M2_PILOT_D = 1.6
PICO_M2_PILOT_DEPTH = 5.0
PICO_WALL_GAP = 0.5     # board edge to the inside of the back wall
USB_RECEPT_W = 7.5      # micro-B receptacle shell (typical part drawing; CHECK)
USB_RECEPT_H = 2.6
USB_RECEPT_L = 5.5
USB_OVERHANG = 1.0      # past the board edge (spec)
USB_CUT_W = 12.0        # back-wall cutout for the plug's moulding (spec)
USB_CUT_H = 9.0
HEADER_ROW_X = 17.78    # the two pin rows, 0.7" apart
HEADER_W = 2.54
HEADER_L = 50.8         # 20 pins
HEADER_PLASTIC_H = 2.5  # the black strip, on the plain side (spec assumption, CHECK)
JUMPER_H = 14.0         # female jumper housing, CHECK (spec)
WIRE_BEND = 10.0        # room for the wires to bend over (spec)
BOOTSEL_HOLE_D = 3.0    # in the floor, for a paperclip (recovery only; tools/bootsel.sh is the normal way)
BOOTSEL_FROM_USB = 12.0     # CHECK on the board
BOOTSEL_OFF_AXIS = 4.0      # CHECK. Which side it lands on once the board is flipped is not
                            # settled, so the floor gets a hole on both sides; one is the button

# ---- Pico bay (spec "Bay clearance") ----------------------------------------------------
BAY_W = 30.0            # least free width (board + jumpers fanning out)
BAY_L = 58.0            # least free length, from the back wall forward

# ---- labels (spec, optional, on by default; round 2 engraves them) -----------------------
LABELS = True
LABEL_DEPTH = 0.6
LABEL_H = 4.0           # capital height
LABEL_GAP = 2.0         # air between a label and its keycap
LABEL_FONT = "/usr/share/fonts/noto/NotoSans-Bold.ttf"

# ---- panel stiffening (round 2) ---------------------------------------------------------
# A 2 mm plate spanning 68 mm flexes under a knob press, so ribs run across its underside,
# between the knob columns and between the two rows, stopping short of the Pico bay. The
# cross ribs end FIT_SLIDE from the walls, so they also locate the panel front to back.
RIBS = True
RIB_T = 2.0
RIB_H = 5.0             # below the panel; the wires run under them

# ---- feet (spec: optional) ---------------------------------------------------------------
FEET_RECESS = False     # True: 4 shallow recesses in the base's underside for stick-on feet
FEET_D = 8.5            # Ø8 feet + 0.5
FEET_DEPTH = 1.0
FEET_INSET = 12.0       # recess centre from the outer faces

# ---- printer (spec) --------------------------------------------------------------------
BED = (250.0, 210.0, 200.0)     # Prusa MK2/MK3
MAX_FOOTPRINT = (240.0, 200.0)  # spec: keep the box under this

"""Renders the model as pictures (adapted from an earlier case by pywkt).

A small painter's-algorithm renderer on matplotlib: every solid is tessellated, split into
small triangles, projected orthographically, depth-sorted and drawn with Lambert shading.
That shows holes and overhangs honestly, which matplotlib's own 3D axes cannot. Plus a 2D
plan with dimensions (plan()). Called by model.py; standalone:

    .venv/bin/python tools/render_layout.py
"""
from __future__ import annotations

import os
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from matplotlib.collections import PolyCollection  # noqa: E402
from matplotlib.colors import to_rgb  # noqa: E402
from matplotlib.patches import Circle, PathPatch, Rectangle, RegularPolygon  # noqa: E402
from matplotlib.textpath import TextPath  # noqa: E402

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import params as P  # noqa: E402

KIND_COLOUR = {
    "encoder": "#9ca3af", "nut": "#6b7280", "knob": "#1e3a8a",
    "switch": "#dc2626", "keycap": "#e5e7eb",
    "pico": "#16a34a", "chips": "#14532d", "usb": "#a3a3a3", "header": "#111827", "jumper": "#f59e0b",
    "zone": "#facc15", "bay": "#22d3ee",
    "screen": "#1d4ed8", "glass": "#1f2937", "active": "#38bdf8", "flex": "#d97706",
    "base": "#e7e5e4", "panel": "#d6d3d1",
}
KIND_ALPHA = {"zone": 0.18, "bay": 0.12}
SHELL_ALPHA = 0.28
MAX_EDGE = 8.0      # triangles are split until no edge is longer than this (depth sorting)
LIGHT = np.array([-0.4, -0.7, 0.6])
LIGHT = LIGHT / np.linalg.norm(LIGHT)


# ---- geometry --------------------------------------------------------------------------------
_cache: dict[int, np.ndarray] = {}


def tris(part, offset=(0.0, 0.0, 0.0)) -> np.ndarray:
    key = id(part)
    if key not in _cache:
        v, t = part.tessellate(0.15)
        v = np.array([tuple(p) for p in v])
        _cache[key] = subdivide(v[np.array(t)])
    return _cache[key] + np.array(offset)


def subdivide(t: np.ndarray) -> np.ndarray:
    """Split triangles (n, 3, 3) until every edge is <= MAX_EDGE."""
    out = []
    work = t
    for _ in range(6):
        e = np.linalg.norm(work - np.roll(work, -1, axis=1), axis=2).max(1)
        small, big = work[e <= MAX_EDGE], work[e > MAX_EDGE]
        out.append(small)
        if len(big) == 0:
            break
        a, b, c = big[:, 0], big[:, 1], big[:, 2]
        ab, bc, ca = (a + b) / 2, (b + c) / 2, (c + a) / 2
        work = np.concatenate([
            np.stack([a, ab, ca], 1), np.stack([ab, b, bc], 1),
            np.stack([ca, bc, c], 1), np.stack([ab, bc, ca], 1)])
    else:
        out.append(work)
    return np.concatenate(out)


def view_basis(elev: float, azim: float):
    """Right, up and towards-viewer unit vectors for a view direction given like matplotlib."""
    e, a = np.radians(elev), np.radians(azim)
    d = np.array([np.cos(e) * np.cos(a), np.cos(e) * np.sin(a), np.sin(e)])   # scene -> viewer
    zup = np.array([0.0, 0.0, 1.0])
    r = np.cross(zup, d)
    if np.linalg.norm(r) < 1e-9:
        r = np.array([1.0, 0.0, 0.0])
    r /= np.linalg.norm(r)
    u = np.cross(d, r)
    return r, u, d


ORTHO = {
    "top  (looking down; front edge at the bottom)": (90.0, -90.0),
    "front  (looking at the front face; X right, Z up)": (0.0, -90.0),
    "right side  (front is left, Z up)": (0.0, 0.0),
}


def draw(ax, items, elev, azim):
    """items: list of (tris, colour, alpha). Depth-sorted, shaded, drawn as one collection."""
    r, u, d = view_basis(elev, azim)
    polys, cols, depth, widths = [], [], [], []
    for t, colour, alpha in items:
        if len(t) == 0:
            continue
        n = np.cross(t[:, 1] - t[:, 0], t[:, 2] - t[:, 0])
        nn = np.linalg.norm(n, axis=1, keepdims=True)
        n = n / np.where(nn == 0, 1, nn)
        shade = 0.55 + 0.45 * np.abs(n @ LIGHT)
        base = np.array(to_rgb(colour))
        rgba = np.empty((len(t), 4))
        rgba[:, :3] = np.clip(base[None, :] * shade[:, None], 0, 1)
        rgba[:, 3] = alpha
        polys.append(np.stack([t @ r, t @ u], axis=2))
        cols.append(rgba)
        depth.append((t @ d).mean(1))
        widths.append(np.full(len(t), 0.2 if alpha >= 1 else 0.0))   # translucent edges would streak
    polys = np.concatenate(polys)
    cols = np.concatenate(cols)
    depth = np.concatenate(depth)
    order = np.argsort(depth)
    widths = np.concatenate(widths)[order]
    pc = PolyCollection(polys[order], facecolors=cols[order], edgecolors=cols[order], linewidths=widths)
    ax.add_collection(pc)
    ax.set_aspect("equal")
    ax.autoscale()
    ax.margins(0.04)
    ax.set_axis_off()
    return r, u


def items_for(parts, shells, offsets=None, shell_alpha=SHELL_ALPHA, zones=True):
    offsets = offsets or {}
    items = []
    for sn, sh in shells.items():
        items.append((tris(sh, offsets.get(sn, (0, 0, 0))), KIND_COLOUR[sn], shell_alpha))
    for n, pl in parts.items():
        if pl.group == "zone" and not zones:
            continue
        items.append((tris(pl.part, offsets.get(n, (0, 0, 0))), KIND_COLOUR[pl.kind], KIND_ALPHA.get(pl.kind, 1.0)))
    return items


def tag(ax, text, p, r, u, fs=7):
    p = np.array(p)
    ax.annotate(text, (p @ r, p @ u), ha="center", va="center", fontsize=fs,
                bbox=dict(boxstyle="round,pad=0.15", fc="white", ec="none", alpha=0.75))


def scale_bar(ax, r, u, origin, length=50.0):
    o = np.array(origin)
    p0, p1 = o @ r, (o + np.array([length, 0, 0])) @ r
    y = o @ u
    ax.plot([p0, p1], [y, y], "k-", lw=2)
    ax.annotate(f"{length:.0f} mm", ((p0 + p1) / 2, y), ha="center", va="top", fontsize=8, xytext=(0, -4), textcoords="offset points")


# ---- 2D plan with dimensions -----------------------------------------------------------------
def dim(ax, a, b, off, text, vertical=False):
    """A dimension line from a to b (data coords), offset `off` across it, with text."""
    if vertical:
        (x, y0), y1 = a, b[1]
        xs = x + off
        ax.annotate("", (xs, y0), (xs, y1), arrowprops=dict(arrowstyle="<->", lw=0.7, shrinkA=0, shrinkB=0))
        ax.text(xs + (1.5 if off >= 0 else -1.5), (y0 + y1) / 2, text, rotation=90, ha="left" if off >= 0 else "right", va="center", fontsize=7)
    else:
        (x0, y), x1 = a, b[0]
        ys = y + off
        ax.annotate("", (x0, ys), (x1, ys), arrowprops=dict(arrowstyle="<->", lw=0.7, shrinkA=0, shrinkB=0))
        ax.text((x0 + x1) / 2, ys + (1.0 if off >= 0 else -1.0), text, ha="center", va="bottom" if off >= 0 else "top", fontsize=7)


def engraved(ax, text, x, y, anchor):
    tp = TextPath((0, 0), text, size=P.LABEL_H / 0.72, prop=dict(family="DejaVu Sans", weight="bold"))
    bb = tp.get_extents()
    dx = x - (bb.x0 + bb.width / 2 if anchor == "c" else bb.x1)
    dy = y - P.LABEL_H / 2
    tp = tp.transformed(matplotlib.transforms.Affine2D().translate(dx, dy))
    ax.add_patch(PathPatch(tp, fc="#57534e", ec="none"))


def plan(L, model, out: str, round_name: str) -> None:
    W, D = L.W, L.D
    fig, ax = plt.subplots(figsize=(17, 8.2))
    ax.add_patch(Rectangle((-W / 2, -D / 2), W, D, fc="#f5f5f4", ec="k", lw=1.2))
    ax.add_patch(Rectangle((-W / 2 + P.WALL, -D / 2 + P.WALL), W - 2 * P.WALL, D - 2 * P.WALL, fc="none", ec="#a8a29e", lw=0.7, ls="--"))
    x0, x1, y0, y1 = L.bay
    ax.add_patch(Rectangle((x0, y0), x1 - x0, y1 - y0, fc=KIND_COLOUR["bay"], alpha=0.18, ec="#0891b2", lw=0.8, hatch="//"))
    px, py = L.pico
    ax.add_patch(Rectangle((px - P.PICO_W / 2, py - P.PICO_L / 2), P.PICO_W, P.PICO_L, fc=KIND_COLOUR["pico"], alpha=0.35, ec="#166534", lw=0.8))
    for sx in (-1, 1):
        ax.add_patch(Rectangle((px + sx * P.HEADER_ROW_X / 2 - P.HEADER_W / 2, py - P.HEADER_L / 2), P.HEADER_W, P.HEADER_L, fc=KIND_COLOUR["jumper"], alpha=0.7, ec="none"))
    for hx, hy in model.pico_holes():
        ax.add_patch(Circle((hx, hy), model.standoff_d(hy) / 2, fc="#78716c", ec="none"))
    ax.add_patch(Rectangle((px - P.USB_CUT_W / 2, D / 2 - P.WALL), P.USB_CUT_W, P.WALL, fc="white", ec="#dc2626", lw=1))
    ax.annotate("USB out the back", (px, D / 2), (px, D / 2 + 7), ha="center", fontsize=7, color="#dc2626",
                arrowprops=dict(arrowstyle="<-", color="#dc2626", lw=0.8))
    # the screen: frame, PCB and header under the panel (dashed), glass, window and picture
    g = model.screen_geometry()
    fx0, fy0 = g["frame_x0"], g["frame_y0"]
    ax.add_patch(Rectangle((fx0, fy0), g["frame_x1"] - fx0, g["frame_y1"] - fy0, fc="none", ec="#78716c", lw=0.7, ls="--"))
    ax.add_patch(Rectangle((g["pcb_x0"], g["cy"] - P.SCREEN_PCB_W / 2), P.SCREEN_PCB_L, P.SCREEN_PCB_W, fc=KIND_COLOUR["screen"], alpha=0.12,
                           ec=KIND_COLOUR["screen"], lw=0.7, ls=":"))
    ax.add_patch(Rectangle((g["glass_x0"], g["cy"] - P.SCREEN_GLASS_W / 2), P.SCREEN_GLASS_L, P.SCREEN_GLASS_W, fc="none", ec="#1f2937", lw=0.6, ls=":"))
    ax.add_patch(Rectangle((g["header_x"] - P.HEADER_W / 2, g["cy"] - model.SCR_ROW / 2), P.HEADER_W, model.SCR_ROW, fc=KIND_COLOUR["jumper"], alpha=0.7, ec="none"))
    wt = model.SCR_WINDOW + 2 * P.SCREEN_CHAMFER
    ax.add_patch(Rectangle((g["win_cx"] - wt / 2, g["cy"] - wt / 2), wt, wt, fc="#e7e5e4", ec="k", lw=0.8))
    ax.add_patch(Rectangle((g["win_cx"] - model.SCR_WINDOW / 2, g["cy"] - model.SCR_WINDOW / 2), model.SCR_WINDOW, model.SCR_WINDOW, fc="#1f2937", ec="none"))
    a = P.SCREEN_ACTIVE
    ax.add_patch(Rectangle((g["win_cx"] - a / 2, g["cy"] - a / 2), a, a, fc=KIND_COLOUR["active"], alpha=0.55, ec="none"))
    ax.text(g["win_cx"], g["cy"] + 3, "screen\n240 x 240", ha="center", va="center", fontsize=7, color="white")
    ax.text(g["header_x"] + 0.5, g["cy"] - model.SCR_ROW / 2 - 1.5, "header ▸", ha="center", va="top", fontsize=5.5, color="#b45309")
    for sx, sy in model.screw_axes():
        ax.add_patch(Circle((sx, sy), P.BOSS_D / 2, fc="#d6d3d1", ec="#78716c", lw=0.6))
        ax.add_patch(Circle((sx, sy), P.SCREW_CLEAR_D / 2, fc="white", ec="k", lw=0.6))
    for n, (x, y) in L.knobs.items():
        ax.add_patch(Rectangle((x - P.ENC_WIRE_ZONE / 2, y - P.ENC_WIRE_ZONE / 2), P.ENC_WIRE_ZONE, P.ENC_WIRE_ZONE, fc="none", ec="#ca8a04", lw=0.5, ls=":"))
        ax.add_patch(Circle((x, y), P.KNOB_D / 2, fc="#bfdbfe", ec=KIND_COLOUR["knob"], lw=1))
        ax.add_patch(RegularPolygon((x, y), 6, radius=P.ENC_NUT_AC / 2, fc="none", ec="#6b7280", lw=0.5))
        ax.add_patch(Circle((x, y), P.ENC_HOLE_D / 2, fc="white", ec="k", lw=0.6))
        sw = " ⏎" if n in P.KNOBS_WITH_SWITCH else ""
        ax.text(x, y - P.KNOB_D / 2 - 0.8, f"knob {n}{sw}", ha="center", va="top", fontsize=6.5, color=KIND_COLOUR["knob"])
    for name, (x, y) in {"FINE": L.fine, **L.nav}.items():
        ax.add_patch(Rectangle((x - P.KEYCAP_W / 2, y - P.KEYCAP_W / 2), P.KEYCAP_W, P.KEYCAP_W, fc="#fafafa", ec="#525252", lw=0.9))
        ax.add_patch(Rectangle((x - P.MX_CUT / 2, y - P.MX_CUT / 2), P.MX_CUT, P.MX_CUT, fc="none", ec="#dc2626", lw=0.6))
        ax.text(x, y, name.replace("<", " ←").replace(">", " →"), ha="center", va="center", fontsize=5.5, color="#737373")
    for text, x, y, anchor in model.labels():
        engraved(ax, text, x, y, anchor)
    # dimensions
    dim(ax, (-W / 2, -D / 2), (W / 2, -D / 2), -9, f"{W:.1f}")
    dim(ax, (-W / 2, -D / 2), (-W / 2, D / 2), -6, f"{D:.1f}", vertical=True)
    k1, k2, k5 = L.knobs[1], L.knobs[2], L.knobs[5]
    dim(ax, k1, k2, P.KNOB_D / 2 + 5, f"{P.KNOB_PITCH:.0f}")
    dim(ax, (k1[0] + P.KNOB_PITCH / 2, k5[1]), (0, k1[1]), 0, f"{P.KNOB_PITCH:.0f}", vertical=True)   # between columns 1 and 2
    a, b = L.nav["TRACK<"], L.nav["TRACK>"]
    dim(ax, a, b, -P.KEYCAP_W / 2 - 3.5, f"{P.NAV_PITCH}")
    dim(ax, L.nav["DEVICE>"], L.nav["PAGE>"], P.KEYCAP_W / 2 + 3, f"{P.NAV_PITCH}", vertical=True)
    dim(ax, (-W / 2, L.fine[1]), (L.fine[0] - P.KEYCAP_W / 2, 0), 0, f"{L.fine[0] + W / 2:.1f} to FINE's centre")
    dim(ax, (L.fine[0] + P.KEYCAP_W / 2, k5[1] - P.KNOB_D / 2 - 4), (k5[0] - P.KNOB_D / 2, 0), 0, f"{k5[0] - P.KNOB_D / 2 - L.fine[0] - P.KEYCAP_W / 2:.0f}")
    dim(ax, (g["win_cx"] - wt / 2, g["cy"] + wt / 2), (g["win_cx"] + wt / 2, 0), 2.5, f"window {wt:.1f} (top face), {model.SCR_WINDOW:.1f} at the glass")
    dim(ax, (x0, y0 - 3), (x1, 0), 0, f"Pico bay {x1 - x0:.0f} x {y1 - y0:.0f}")
    dim(ax, (b[0], -D / 2 + 3.5), (W / 2, 0), 0, f"{W / 2 - b[0]:.0f}")
    ax.set_xlim(-W / 2 - 14, W / 2 + 6)
    ax.set_ylim(-D / 2 - 16, D / 2 + 11)
    ax.set_aspect("equal")
    ax.set_axis_off()
    ax.set_title(f"knobbox test case {round_name}: panel from above, front edge at the bottom; {W:.1f} x {D:.1f} x {model.CASE_H:.0f} mm. "
                 f"Knob caps Ø{P.KNOB_D:.0f}, keycaps {P.KEYCAP_W:.0f}, MX cutouts red, wire zones dotted"
                 f"{', ⏎ = has a push switch' if P.KNOBS_WITH_SWITCH else ''}. "
                 f"Grey text is engraved ({P.LABEL_H:.0f} mm).\n"
                 f"Cyan = Pico bay: nothing hangs from the panel there. Green = the Pico, chips down and header pins up, orange = jumper rows. "
                 f"Screen: window (dark, chamfered edge grey), picture (light blue); under the panel, dotted: its PCB and glass, dashed: the frame.", fontsize=8.5)
    fig.tight_layout()
    p = os.path.join(out, f"{round_name}_plan.png")
    fig.savefig(p, dpi=130)
    plt.close(fig)
    print("wrote", p)


# ---- the printable parts on their own ---------------------------------------------------------
def parts_sheet(shells, model, out: str, round_name: str) -> None:
    from build123d import Box, Pos  # noqa: PLC0415

    fig, axes = plt.subplots(2, 2, figsize=(20, 13), gridspec_kw=dict(height_ratios=[1, 1]))
    draw(axes[0, 0], [(tris(shells["base"]), KIND_COLOUR["base"], 1.0)], 48, -62)
    axes[0, 0].set_title("base from the front right, above: corner bosses with pilot holes, Pico standoffs with glue pins, "
                         "USB cutout in the back wall", fontsize=9)
    draw(axes[0, 1], [(tris(shells["panel"]), KIND_COLOUR["panel"], 1.0)], -55, -62)
    axes[0, 1].set_title("panel from below (this side faces up on the bed): key pockets, ribs, encoder and screw holes, "
                         "the screen's frame and glass pocket", fontsize=9)
    draw(axes[1, 0], [(tris(shells["panel"]), KIND_COLOUR["panel"], 1.0)], 62, -80)
    axes[1, 0].set_title("panel from above: engraved labels and the screen window (this side goes face down on the bed)", fontsize=9)
    k = model.KNOB
    half = k.intersect(Pos(-50, 0, 0) * Box(100, 100, 100))
    half = half if not isinstance(half, list) else half[0]
    items = [(tris(half), KIND_COLOUR["knob"], 1.0), (tris(k, (0, 30, 0)), KIND_COLOUR["knob"], 1.0)]
    draw(axes[1, 1], items, -25, 20)
    axes[1, 1].set_title(f"knob, Ø{P.KNOB_D:.0f} x {model.KNOB_H:.1f}: cut in half (left) and from below (right). "
                         "Hollow over the nut, then the bore, D-shaped near its blind end", fontsize=9)
    fig.suptitle(f"knobbox test case {round_name}: the printable parts", fontsize=11)
    fig.tight_layout()
    p = os.path.join(out, f"{round_name}_parts.png")
    fig.savefig(p, dpi=100)
    plt.close(fig)
    print("wrote", p)


# ---- the pictures ------------------------------------------------------------------------------
def render_all(parts, shells, L, out: str, round_name: str) -> None:
    import model  # noqa: PLC0415

    size = f"{L.W:.1f} x {L.D:.1f} x {model.CASE_H:.0f} mm (W x D x H)"
    plan(L, model, out, round_name)
    parts_sheet(shells, model, out, round_name)

    # 1. three orthographic views, shells translucent
    fig, axes = plt.subplots(3, 1, figsize=(15, 17), gridspec_kw=dict(height_ratios=[1.0, 0.7, 0.7]))
    for i, (ax, (title, (el, az))) in enumerate(zip(axes, ORTHO.items())):
        r, u = draw(ax, items_for(parts, shells), el, az)
        if i == 0:
            for n, (x, y) in L.knobs.items():
                tag(ax, str(n), (x, y, model.KNOB_TOP), r, u, 8)
        if i == 1:
            scale_bar(ax, r, u, (-L.W / 2, -L.D / 2, -6))
        ax.set_title(title, fontsize=10)
    fig.suptitle(f"knobbox test case {round_name}: {size}. Base and panel translucent; yellow = wire keep-out under the "
                 f"encoders and switches, cyan = Pico bay, orange = jumper housings.", fontsize=10)
    fig.tight_layout()
    p = os.path.join(out, f"{round_name}_views.png")
    fig.savefig(p, dpi=100)
    plt.close(fig)
    print("wrote", p)

    # 2. isometric from the front right, shells translucent; and outside, opaque
    fig, axes = plt.subplots(1, 2, figsize=(20, 8))
    draw(axes[0], items_for(parts, shells), 28, -60)
    axes[0].set_title(f"{round_name}: inside, from the front right; base and panel translucent", fontsize=10)
    r, u = draw(axes[1], items_for(parts, shells, shell_alpha=1.0, zones=False), 32, -115)
    axes[1].set_title(f"{round_name}: outside, from the front left, as it sits on the desk", fontsize=10)
    fig.suptitle(size, fontsize=10)
    fig.tight_layout()
    p = os.path.join(out, f"{round_name}_iso.png")
    fig.savefig(p, dpi=100)
    plt.close(fig)
    print("wrote", p)

    # 3. exploded: the panel with everything on it lifted, knobs and keycaps a further step
    up, up2 = 60.0, 30.0
    offsets = {}
    for n, pl in parts.items():
        on_panel = pl.group == "panel" or (pl.group == "zone" and n != "bay")
        offsets[n] = (0, 0, up + up2 * pl.lift) if on_panel else (0, 0, 0)
    offsets["panel"] = (0, 0, up)
    fig = plt.figure(figsize=(14, 11))
    ax = fig.add_subplot(111)
    draw(ax, items_for(parts, shells, offsets, shell_alpha=0.55), 24, -60)
    ax.set_title(f"{round_name}: exploded. The panel and everything hanging from it lifted {up:.0f}, knobs and keycaps a further {up2:.0f}; "
                 f"the base keeps the Pico on its standoffs.", fontsize=9)
    fig.tight_layout()
    p = os.path.join(out, f"{round_name}_exploded.png")
    fig.savefig(p, dpi=100)
    plt.close(fig)
    print("wrote", p)


if __name__ == "__main__":
    import model

    render_all(model.build_parts(), model.build_shells(), model.L,
               os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "out"), model.ROUND)

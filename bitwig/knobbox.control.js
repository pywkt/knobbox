// knobbox — Bitwig Studio controller script.
//
// The box only reports what its controls do (see logic/src/midi.rs); all
// behaviour is here, so changing it means editing this file, not reflashing.
//
// Devices (the starting mode):
//   Knobs 1–8    the selected device's current Remote Controls page
//   Knobs 9–16   the page after it (does nothing on the last page)
//   Page ←/→     Remote Controls page
//   Device ←/→   device
//   Track ←/→    track
//
// Mixer:
//   Knobs 1–8    volume, pan or a send of 8 tracks; knobs 9–16 the next one
//   Page ←/→     volume → pan → send 1 → send 2 …
//   Device ←/→   the 8 tracks before / after
//   Track ←/→    track (the 8 shown follow it)
//
// Either mode:
//   Fine         hold while turning for small steps
//   Fine + Track ←/→     switch between devices and the mixer
//   Fine + Device ←/→    lock the knobs to this device (and its track), or
//                        unlock; while locked, neither selecting things in
//                        Bitwig nor the box's Track and Device buttons move
//                        them (Page ←/→ still works, and so does the
//                        mixer's Device ←/→)
//   Fine, Fine (double-tap)  reset the last knob you turned to its default
//   Hold a knob  for half a second: reset it (needs the knob switches wired)
//   Turning fast takes bigger steps (Preferences: Acceleration)
//
// A popup names the track, device and page whenever they change, and the
// box's screen shows them with each knob's parameter, value and modulation,
// the transport and tempo, and levels (SysEx back to the box; see
// logic/src/screen.rs).

loadAPI(25);

host.defineController("DIY", "knobbox", "0.3", "45a01e73-9a8c-4567-b0c4-40cc263388de", "DIY");
host.defineMidiPorts(1, 1);
// The ALSA port name comes from the USB product string "knobbox".
host.addDeviceNameBasedDiscoveryPair(["knobbox MIDI 1"], ["knobbox MIDI 1"]);

// Must match logic/src/midi.rs.
const CHANNEL = 0;
const ENCODER_CC = 102;
const SWITCH_CC = 70;
const BUTTON_CC = 52;

const KNOBS = 16;
const ROW = 8;
const BUTTONS = 8;
const BUTTON = {
    PAGE_PREV: 0,
    PAGE_NEXT: 1,
    DEVICE_PREV: 2,
    DEVICE_NEXT: 3,
    TRACK_PREV: 4,
    TRACK_NEXT: 5,
    FINE: 6,
};

const MODE = { DEVICE: "device", MIXER: "mixer" };
// Sends the mixer can reach (effect tracks).
const SENDS = 8;

// Screen messages: F0 7D 6B 62 <command> <data> F7. Must match
// logic/src/sysex.rs and logic/src/screen.rs.
const SYSEX_HEADER = [0xF0, 0x7D, 0x6B, 0x62];
const HELLO = "f07d6b6210f7"; // the box (re)connected: send everything
const SCREEN = {
    TRACK: 0x20,
    COLOR: 0x21,
    DEVICE: 0x22,
    PAGE: 0x23,
    KNOB_NAME: 0x24,
    KNOB_VALUE: 0x25,
    STATUS: 0x26,
    KNOB_STYLE: 0x27,
    KNOB_MOD: 0x28,
    METERS: 0x29,
    CONFIG: 0x2A,
};
const STATUS_PLAYING = 1;
const STATUS_RECORDING = 2;
const STATUS_LOCKED = 4;
const STYLE_COLOR = 1;
const STYLE_SELECTED = 2;
const METER_LEVELS = 127; // levels 0–126
const NO_METER = 0x7F;
const MOD_MAX = 127;
const SHOWN = 8; // knobs on the screen's grid
const NAME_LEN = 16;
const VALUE_LEN = 12;
const TITLE_LEN = 24;
const VALUE_MAX = 0x3FFF; // 14 bits

const RESET_HOLD_MS = 500;
// Fine counts as a tap if released within this, and two taps this close
// together reset the last knob turned.
const TAP_MS = 300;
const DOUBLE_TAP_MS = 400;
// The relative matcher's notion of "one full turn". Knob speed is then set
// with each knob's sensitivity, so it can follow the preferences live.
const MATCHER_FULL_TURN = 128;
// Acceleration: clicks a second below which a knob moves normally, and at
// which it reaches the most it multiplies by.
const ACCEL_SLOW = 8;
const ACCEL_FAST = 40;
const ACCEL_MAX = { "Off": 1, "Gentle": 3, "Strong": 6 };
// A pause this long starts a turn again at normal speed.
const ACCEL_RESET_MS = 150;

var midiOut;
var cursorTrack, cursorDevice, transport;
var rows = []; // [current page, next page]
var trackBank, effectBank;
var knobs = [];
var deviceParams = []; // knob → its Remote Control in either row
var turnTargets = [];
var hasNextPage = false;

var mode = MODE.DEVICE;
var mixerPage = 0; // 0 volume, 1 pan, 2… sends

var clicksForFullRange = 64;
var fineDivisor = 10;
var accelMax = ACCEL_MAX.Gentle;
var zoomTenths = 10;

var fineHeld = false;
var fineDownAt = 0;
var fineUsed = false; // a combo or a turn happened while Fine was held
var lastFineTap = 0;

var lastTurned = -1;
var lastTurnAt = [];
var turnRate = []; // clicks a second, smoothed

var switchDown = [];
var pressCount = [];

var ready = false;
var announcePending = false;

// The last screen message for each thing shown, and which still need sending.
var screenLast = {};
var screenPending = {};

var trackMeter = 0;
var mixerMeters = [];

function init() {
    var midiIn = host.getMidiInPort(0);
    midiIn.setMidiCallback(onMidi);
    midiIn.setSysexCallback(onSysex);
    midiOut = host.getMidiOutPort(0);

    var prefs = host.getPreferences();
    prefs.getNumberSetting("Clicks for full range", "Knobs", 8, 512, 1, "clicks", 64)
        .addRawValueObserver(function (v) { clicksForFullRange = v; applySensitivity(); });
    prefs.getNumberSetting("Fine adjust divides by", "Knobs", 2, 50, 1, "×", 10)
        .addRawValueObserver(function (v) { fineDivisor = v; applySensitivity(); });
    prefs.getEnumSetting("Acceleration", "Knobs", ["Off", "Gentle", "Strong"], "Gentle")
        .addValueObserver(function (v) { accelMax = ACCEL_MAX[v] || 1; });
    prefs.getNumberSetting("Big value while turning (0 = off)", "Screen", 0, 5, 0.1, "s", 1)
        .addRawValueObserver(function (v) {
            zoomTenths = Math.round(v * 10);
            screenSet("config", [SCREEN.CONFIG, Math.min(zoomTenths, 0x7F)]);
        });

    cursorTrack = host.createCursorTrack("knobbox-track", "knobbox", 0, 0, true);
    cursorDevice = cursorTrack.createCursorDevice(); // follows the selection in Bitwig
    rows = [
        cursorDevice.createCursorRemoteControlsPage(ROW), // the page shown in Bitwig
        cursorDevice.createCursorRemoteControlsPage("knobbox-next", ROW, ""),
    ];
    trackBank = host.createMainTrackBank(ROW, SENDS, 0);
    trackBank.followCursorTrack(cursorTrack);
    effectBank = host.createEffectTrackBank(SENDS, 0);
    transport = host.createTransport();

    var surface = host.createHardwareSurface();
    for (var i = 0; i < KNOBS; i++) {
        var knob = surface.createRelativeHardwareKnob("knob" + (i + 1));
        knob.setAdjustValueMatcher(
            midiIn.createRelative2sComplementCCValueMatcher(CHANNEL, ENCODER_CC + i, MATCHER_FULL_TURN));
        knobs.push(knob);
        turnTargets.push(host.createRelativeHardwareControlAdjustmentTarget(onTurn.bind(null, i)));

        var param = rows[rowOf(i)].getParameter(i % ROW);
        watchParam(param, i);
        deviceParams.push(param);

        switchDown.push(false);
        pressCount.push(0);
        lastTurnAt.push(0);
        turnRate.push(0);
    }
    for (var t = 0; t < ROW; t++) {
        watchTrack(t);
    }
    for (var s = 0; s < SENDS; s++) {
        var fx = effectBank.getItemAt(s);
        fx.name().addValueObserver(onMixerPagesChanged);
        fx.exists().addValueObserver(onMixerPagesChanged);
    }
    trackBank.scrollPosition().addValueObserver(screenHeader);

    rows[0].selectedPageIndex().addValueObserver(syncNextRow);
    rows[0].pageCount().addValueObserver(syncNextRow);
    rows[0].pageNames().addValueObserver(screenHeader);
    cursorTrack.name().addValueObserver(scheduleAnnounce);
    cursorDevice.name().addValueObserver(scheduleAnnounce);
    cursorDevice.exists().addValueObserver(scheduleAnnounce);

    cursorTrack.name().addValueObserver(function (name) {
        screenSet("track", [SCREEN.TRACK].concat(ascii(name, TITLE_LEN)));
    });
    cursorTrack.color().addValueObserver(function (r, g, b) {
        screenSet("color", [SCREEN.COLOR, to7bit(r), to7bit(g), to7bit(b)]);
    });
    cursorTrack.position().addValueObserver(screenAllKnobs);
    cursorTrack.addVuMeterObserver(METER_LEVELS, -1, true, function (v) {
        trackMeter = v;
        screenMeters();
    });
    cursorDevice.name().addValueObserver(screenHeader);
    cursorDevice.exists().addValueObserver(screenHeader);

    cursorTrack.isPinned().markInterested();
    cursorDevice.isPinned().addValueObserver(screenStatus);
    transport.isPlaying().addValueObserver(screenStatus);
    transport.isArrangerRecordEnabled().addValueObserver(screenStatus);
    transport.tempo().value().addRawValueObserver(screenStatus);

    // Last, once every value read here is being watched.
    rebind();
    applySensitivity();

    // Don't announce the burst of values that arrives at startup.
    host.scheduleTask(function () { ready = true; }, 1000);
    host.println("knobbox ready");
}

// Observe what the screen shows of a parameter, for knob `i` (and, for the
// mixer's parameters, its partner in the other row).
function watchParam(param, i) {
    var update = function () { screenKnobsFor(i); };
    param.name().addValueObserver(update);
    param.exists().addValueObserver(update);
    param.value().addValueObserver(update);
    param.displayedValue().addValueObserver(update);
    param.modulatedValue().addValueObserver(update);
}

function watchTrack(t) {
    var track = trackBank.getItemAt(t);
    track.name().markInterested();
    track.exists().markInterested();
    track.position().markInterested();
    var update = function () { screenKnobsFor(t); };
    track.name().addValueObserver(update);
    track.exists().addValueObserver(update);
    track.position().addValueObserver(update);
    track.color().addValueObserver(update);
    watchParam(track.volume(), t);
    watchParam(track.pan(), t);
    for (var s = 0; s < SENDS; s++) {
        watchParam(track.sendBank().getItemAt(s), t);
    }
    mixerMeters.push(0);
    track.addVuMeterObserver(METER_LEVELS, -1, true, function (v) {
        mixerMeters[t] = v;
        screenMeters();
    });
}

// Bitwig calls this after values change: send the screen what changed.
function flush() {
    for (var key in screenPending) {
        midiOut.sendSysex(hex(SYSEX_HEADER.concat(screenLast[key], [0xF7])));
    }
    screenPending = {};
}

function onSysex(data) {
    if (data.toLowerCase() === HELLO) {
        for (var key in screenLast) {
            screenPending[key] = true;
        }
        host.requestFlush();
    }
}

function exit() {}

// ---- what each knob controls ----

function rowOf(knob) {
    return knob < ROW ? 0 : 1;
}

// Volume, pan, then one page per effect track.
function mixerPageCount() {
    var n = 0;
    while (n < SENDS && effectBank.getItemAt(n).exists().get()) {
        n++;
    }
    return 2 + n;
}

function mixerPageName(page) {
    if (page === 0) {
        return "Volume";
    }
    if (page === 1) {
        return "Pan";
    }
    return "Send " + effectBank.getItemAt(page - 2).name().get();
}

function mixerParam(track, page) {
    if (page === 0) {
        return track.volume();
    }
    if (page === 1) {
        return track.pan();
    }
    return track.sendBank().getItemAt(page - 2);
}

// The parameter knob `i` controls now, or null.
function target(i) {
    var row = rowOf(i);
    if (mode === MODE.MIXER) {
        var page = mixerPage + row;
        if (page >= mixerPageCount()) {
            return null;
        }
        return mixerParam(trackBank.getItemAt(i % ROW), page);
    }
    if (row === 1 && !hasNextPage) {
        return null;
    }
    return deviceParams[i];
}

// The mixer track knob `i` controls, if it's on one.
function mixerTrack(i) {
    if (mode !== MODE.MIXER || !target(i)) {
        return null;
    }
    var track = trackBank.getItemAt(i % ROW);
    return track.exists().get() ? track : null;
}

// Each knob drives its parameter directly (so Bitwig shows it as mapped)
// and also reports each turn to onTurn, for acceleration and reset.
function rebind() {
    for (var i = 0; i < KNOBS; i++) {
        knobs[i].clearBindings();
        var p = target(i);
        if (p) {
            knobs[i].addBinding(p);
            knobs[i].addBinding(turnTargets[i]);
        }
    }
    screenAllKnobs();
    screenHeader();
}

function applySensitivity() {
    if (knobs.length === 0) {
        return;
    }
    var s = MATCHER_FULL_TURN / clicksForFullRange;
    if (fineHeld) {
        s /= fineDivisor;
    }
    for (var i = 0; i < knobs.length; i++) {
        knobs[i].setSensitivity(s);
    }
}

// Knob `i` turned by `amount` (already moved by its binding). Spinning it
// fast adds more on top.
function onTurn(i, amount) {
    lastTurned = i;
    if (fineHeld) {
        fineUsed = true;
        return; // no acceleration in fine adjust
    }
    var now = Date.now();
    var dt = now - lastTurnAt[i];
    lastTurnAt[i] = now;
    var clicks = Math.abs(amount) * clicksForFullRange;
    if (dt > ACCEL_RESET_MS) {
        turnRate[i] = 0;
        return;
    }
    // Smoothed, so one quick pair of clicks doesn't jump to full speed.
    turnRate[i] = 0.7 * turnRate[i] + 0.3 * clicks * 1000 / Math.max(dt, 1);
    var t = (turnRate[i] - ACCEL_SLOW) / (ACCEL_FAST - ACCEL_SLOW);
    var mult = 1 + (accelMax - 1) * Math.max(0, Math.min(1, t));
    var p = target(i);
    if (mult > 1 && p) {
        p.value().inc(amount * (mult - 1));
    }
}

// Keep row 2 one page after row 1; nothing there on the last page.
function syncNextRow() {
    var index = rows[0].selectedPageIndex().get();
    var count = rows[0].pageCount().get();
    var hasNext = index >= 0 && index + 1 < count;
    if (hasNext) {
        rows[1].selectedPageIndex().set(index + 1);
    }
    if (hasNext !== hasNextPage) {
        hasNextPage = hasNext;
        if (mode === MODE.DEVICE) {
            rebind();
        }
    }
    screenHeader();
    scheduleAnnounce();
}

function onMixerPagesChanged() {
    var count = mixerPageCount();
    if (mixerPage >= count) {
        mixerPage = count - 1;
    }
    if (mode === MODE.MIXER) {
        rebind();
    }
    screenHeader();
}

// ---- buttons and knob switches ----

// Encoder turns are handled by the hardware knobs above; this gets the
// switches and buttons.
function onMidi(status, cc, value) {
    if (status !== (0xB0 | CHANNEL)) {
        return;
    }
    var pressed = value > 0;
    if (cc >= SWITCH_CC && cc < SWITCH_CC + KNOBS) {
        onSwitch(cc - SWITCH_CC, pressed);
    } else if (cc >= BUTTON_CC && cc < BUTTON_CC + BUTTONS) {
        onButton(cc - BUTTON_CC, pressed);
    }
}

function onSwitch(i, pressed) {
    switchDown[i] = pressed;
    if (!pressed) {
        return;
    }
    var thisPress = ++pressCount[i];
    host.scheduleTask(function () {
        if (switchDown[i] && pressCount[i] === thisPress) {
            resetKnob(i);
        }
    }, RESET_HOLD_MS);
}

function resetKnob(i) {
    var p = target(i);
    if (!p || !p.exists().get()) {
        host.showPopupNotification("Nothing to reset on knob " + (i + 1));
        return;
    }
    p.reset();
    var track = mixerTrack(i);
    var name = track ? track.name().get() + " " + mixerPageName(mixerPage + rowOf(i)) : p.name().get();
    host.showPopupNotification("Reset " + name);
}

function onFine(pressed) {
    var now = Date.now();
    fineHeld = pressed;
    applySensitivity();
    if (pressed) {
        fineDownAt = now;
        fineUsed = false;
        if (now - lastFineTap < DOUBLE_TAP_MS) {
            lastFineTap = 0;
            fineUsed = true; // this press was the second tap, not a third's start
            if (lastTurned < 0) {
                host.showPopupNotification("Turn a knob, then double-tap Fine to reset it");
            } else {
                resetKnob(lastTurned);
            }
        }
    } else if (!fineUsed && now - fineDownAt < TAP_MS) {
        lastFineTap = now;
    }
}

function onButton(i, pressed) {
    if (i === BUTTON.FINE) {
        onFine(pressed);
        return;
    }
    if (!pressed) {
        return;
    }
    if (fineHeld) {
        fineUsed = true;
        switch (i) {
            case BUTTON.TRACK_PREV:
            case BUTTON.TRACK_NEXT:
                toggleMode();
                return;
            case BUTTON.DEVICE_PREV:
            case BUTTON.DEVICE_NEXT:
                toggleLock();
                return;
        }
    }
    if (isLocked() && movesCursor(i)) {
        host.showPopupNotification("Locked to " + cursorTrack.name().get()
            + ": Fine + Device ←/→ unlocks");
        return;
    }
    if (mode === MODE.MIXER) {
        switch (i) {
            case BUTTON.PAGE_PREV: setMixerPage(mixerPage - 1); break;
            case BUTTON.PAGE_NEXT: setMixerPage(mixerPage + 1); break;
            case BUTTON.DEVICE_PREV: trackBank.scrollPageBackwards(); break;
            case BUTTON.DEVICE_NEXT: trackBank.scrollPageForwards(); break;
            case BUTTON.TRACK_PREV: cursorTrack.selectPrevious(); break;
            case BUTTON.TRACK_NEXT: cursorTrack.selectNext(); break;
        }
        scheduleAnnounce();
        return;
    }
    switch (i) {
        case BUTTON.PAGE_PREV: rows[0].selectPreviousPage(false); break;
        case BUTTON.PAGE_NEXT: rows[0].selectNextPage(false); break;
        case BUTTON.DEVICE_PREV: cursorDevice.selectPrevious(); break;
        case BUTTON.DEVICE_NEXT: cursorDevice.selectNext(); break;
        case BUTTON.TRACK_PREV: cursorTrack.selectPrevious(); break;
        case BUTTON.TRACK_NEXT: cursorTrack.selectNext(); break;
    }
}

function isLocked() {
    return cursorDevice.isPinned().get();
}

// Whether a button would move the track or device the knobs are locked to.
// The mixer's Device ←/→ only scrolls which 8 tracks show, so it's allowed.
function movesCursor(i) {
    switch (i) {
        case BUTTON.TRACK_PREV:
        case BUTTON.TRACK_NEXT:
            return true;
        case BUTTON.DEVICE_PREV:
        case BUTTON.DEVICE_NEXT:
            return mode === MODE.DEVICE;
    }
    return false;
}

function setMixerPage(page) {
    var count = mixerPageCount();
    page = Math.max(0, Math.min(count - 1, page));
    if (page !== mixerPage) {
        mixerPage = page;
        rebind();
    }
}

function toggleMode() {
    mode = mode === MODE.DEVICE ? MODE.MIXER : MODE.DEVICE;
    lastTurned = -1;
    rebind();
    announce();
}

// Lock: pin the track and device so selecting elsewhere in Bitwig leaves the
// knobs where they are; onButton also ignores the box's Track and Device
// buttons while locked.
function toggleLock() {
    var lock = !isLocked();
    cursorTrack.isPinned().set(lock);
    cursorDevice.isPinned().set(lock);
    var device = cursorDevice.exists().get() ? cursorDevice.name().get() : "no device";
    host.showPopupNotification(lock
        ? "Locked to " + cursorTrack.name().get() + " › " + device
        : "Unlocked: following the selection");
}

// ---- popups ----

// Several values change together (track → device → page), so wait a moment
// and show one popup.
function scheduleAnnounce() {
    if (!ready || announcePending) {
        return;
    }
    announcePending = true;
    host.scheduleTask(function () {
        announcePending = false;
        announce();
    }, 100);
}

function announce() {
    if (mode === MODE.MIXER) {
        host.showPopupNotification("Mixer › " + mixerPageName(mixerPage) + " (" + (mixerPage + 1) + "/"
            + mixerPageCount() + ") › " + mixerRange());
        return;
    }
    var text = cursorTrack.name().get();
    if (!cursorDevice.exists().get()) {
        host.showPopupNotification(text + " › no device");
        return;
    }
    text += " › " + cursorDevice.name().get();
    var names = rows[0].pageNames().get();
    var index = rows[0].selectedPageIndex().get();
    var count = rows[0].pageCount().get();
    if (count > 0 && index >= 0 && index < names.length) {
        text += " › " + names[index] + " (" + (index + 1) + "/" + count + ")";
        if (hasNextPage && index + 1 < names.length) {
            text += " + " + names[index + 1];
        }
    } else {
        text += " › no remote controls";
    }
    host.showPopupNotification(text);
}

function mixerRange() {
    var first = trackBank.scrollPosition().get() + 1;
    return "tracks " + first + "-" + (first + ROW - 1);
}

// ---- the box's screen ----

function screenSet(key, bytes) {
    var last = screenLast[key];
    if (last && last.length === bytes.length && last.every(function (b, i) { return b === bytes[i]; })) {
        return;
    }
    screenLast[key] = bytes;
    screenPending[key] = true;
}

// The device line and page: the device and its Remote Controls page, or the
// mixer's tracks and parameter.
function screenHeader() {
    if (!rows.length) {
        return; // observers can fire before init has finished
    }
    var device, index, count, name;
    if (mode === MODE.MIXER) {
        device = "Mixer: " + mixerRange();
        index = mixerPage;
        count = mixerPageCount();
        name = mixerPageName(mixerPage);
    } else {
        device = cursorDevice.exists().get() ? cursorDevice.name().get() : "";
        index = rows[0].selectedPageIndex().get();
        count = rows[0].pageCount().get();
        var names = rows[0].pageNames().get();
        var valid = count > 0 && index >= 0 && index < names.length;
        name = valid ? names[index] : "";
        if (!valid) {
            index = -1;
        }
    }
    screenSet("device", [SCREEN.DEVICE].concat(ascii(device, TITLE_LEN)));
    screenSet("page", [SCREEN.PAGE, index >= 0 ? Math.min(index, 0x7E) : 0x7F, Math.min(count, 0x7F)]
        .concat(ascii(name, NAME_LEN)));
}

function screenStatus() {
    if (!transport) {
        return;
    }
    var flags = (transport.isPlaying().get() ? STATUS_PLAYING : 0)
        | (transport.isArrangerRecordEnabled().get() ? STATUS_RECORDING : 0)
        | (isLocked() ? STATUS_LOCKED : 0);
    var tempo = Math.max(0, Math.min(VALUE_MAX, Math.round(transport.tempo().value().getRaw() * 10)));
    screenSet("status", [SCREEN.STATUS, flags, tempo >> 7, tempo & 0x7F]);
}

function screenMeters() {
    var bytes = [SCREEN.METERS, clampLevel(trackMeter)];
    for (var i = 0; i < SHOWN; i++) {
        var track = mixerTrack(i);
        bytes.push(track ? clampLevel(mixerMeters[i % ROW]) : NO_METER);
    }
    screenSet("meters", bytes);
}

function clampLevel(v) {
    return Math.max(0, Math.min(METER_LEVELS - 1, v));
}

// A value observer for knob or track `i` fired: refresh the knobs it can
// show on (the other row's knob too, which shows the same track).
function screenKnobsFor(i) {
    screenKnob(i % ROW);
    screenKnob(i % ROW + ROW);
}

function screenAllKnobs() {
    for (var i = 0; i < KNOBS; i++) {
        screenKnob(i);
    }
    screenMeters();
}

function screenKnob(i) {
    if (knobs.length < KNOBS) {
        return; // observers can fire before init has finished
    }
    var p = target(i);
    var exists = p && p.exists().get();
    var track = mixerTrack(i);
    var name = "";
    if (track) {
        name = track.name().get();
    } else if (exists && mode === MODE.DEVICE) {
        name = p.name().get();
    }
    screenSet("name" + i, [SCREEN.KNOB_NAME, i].concat(ascii(name, NAME_LEN)));

    var value = exists ? p.value().get() : 0;
    var v = Math.round(Math.max(0, Math.min(1, value)) * VALUE_MAX);
    screenSet("value" + i, [SCREEN.KNOB_VALUE, i, v >> 7, v & 0x7F]
        .concat(ascii(exists ? p.displayedValue().get() : "", VALUE_LEN)));

    var style = [SCREEN.KNOB_STYLE, i, 0];
    if (track) {
        var c = track.color();
        var selected = track.position().get() === cursorTrack.position().get();
        style = [SCREEN.KNOB_STYLE, i, STYLE_COLOR | (selected ? STYLE_SELECTED : 0),
            to7bit(c.red()), to7bit(c.green()), to7bit(c.blue())];
    }
    screenSet("style" + i, style);

    // Only when modulation has moved it visibly from where it's set.
    var mod = [SCREEN.KNOB_MOD, i];
    if (exists) {
        var m = Math.max(0, Math.min(1, p.modulatedValue().get()));
        if (Math.abs(m - value) * MOD_MAX >= 1) {
            mod.push(Math.round(m * MOD_MAX));
        }
    }
    screenSet("mod" + i, mod);
}

// Printable ASCII only (the box's fonts), cut to `max` characters.
var ASCII_SWAPS = { "µ": "u", "°": "o", "–": "-", "—": "-", "−": "-", "×": "x", "♯": "#", "♭": "b", "∞": "inf" };

function ascii(text, max) {
    var out = [];
    text = text || "";
    for (var i = 0; i < text.length && out.length < max; i++) {
        var c = text.charAt(i);
        var swap = ASCII_SWAPS[c] || c;
        for (var j = 0; j < swap.length && out.length < max; j++) {
            var code = swap.charCodeAt(j);
            out.push(code >= 0x20 && code <= 0x7E ? code : 0x3F); // '?'
        }
    }
    return out;
}

function to7bit(x) {
    return Math.max(0, Math.min(127, Math.round(x * 127)));
}

function hex(bytes) {
    return bytes.map(function (b) { return (b < 16 ? "0" : "") + b.toString(16); }).join("");
}

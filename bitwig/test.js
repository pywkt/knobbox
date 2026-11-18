// Checks bitwig/knobbox.control.js without Bitwig: `node bitwig/test.js`.
// Every API object is a recording fake, values can be set (firing observers), and
// the test drives knob turns and buttons, then checks bindings, popups, resets and
// the SysEx sent to the box.
const fs = require('fs');
const assert = require('assert');
const src = fs.readFileSync(process.argv[2] || require('path').join(__dirname, 'knobbox.control.js'), 'utf8');

let now = 1000;
Date.now = () => now;
const log = [];       // ['inc', path, x] / ['reset', path]
const popups = [];
const nav = [];       // '<path>.selectNext' etc.
const sent = [];
const tasks = [];
let midiCb = null, sysexCb = null;

function defaultFor(path) {
    if (/exists\(\)$|isPinned\(\)$|isPlaying\(\)$|RecordEnabled\(\)$/.test(path)) return false;
    if (/name\(\)$|displayedValue\(\)$/.test(path)) return '';
    if (/pageNames\(\)$/.test(path)) return [];
    if (/selectedPageIndex\(\)$/.test(path)) return -1;
    return 0;
}

function mock(path, extra = {}) {
    const children = {};
    const state = { path, value: undefined, observers: [], vu: null, bindings: [] };
    return new Proxy(function () {}, {
        get(_, prop) {
            if (typeof prop === 'symbol') return undefined;
            if (Object.prototype.hasOwnProperty.call(extra, prop)) return extra[prop];
            switch (prop) {
                case '__state': return state;
                case 'toString': return () => path;
                case 'addValueObserver': case 'addRawValueObserver':
                    return (cb) => { state.observers.push(cb); };
                case 'markInterested': return () => {};
                case 'get': case 'getRaw':
                    return () => (state.value === undefined ? defaultFor(path) : state.value);
                case 'set': return (v) => { state.value = v; state.observers.forEach(cb => cb(v)); };
                case 'inc': return (x) => log.push(['inc', path, x]);
                case 'reset': return () => log.push(['reset', path]);
                case 'red': case 'green': case 'blue': return () => (state.rgb ? state.rgb[prop] : 0.5);
                case 'addVuMeterObserver': return (range, ch, peak, cb) => { state.vu = cb; };
                case 'addBinding': return (t) => { state.bindings.push(t); };
                case 'clearBindings': return () => { state.bindings = []; };
                case 'setBinding': return (t) => { state.bindings = [t]; };
                case 'selectNext': case 'selectPrevious':
                case 'selectNextPage': case 'selectPreviousPage':
                case 'scrollPageForwards': case 'scrollPageBackwards':
                    return () => nav.push(path + '.' + prop);
            }
            return (...args) => {
                const key = String(prop) + '(' + args.filter(a => typeof a !== 'function' && typeof a !== 'object').join(',') + ')';
                return children[key] || (children[key] = mock(path + '.' + key));
            };
        },
    });
}

global.loadAPI = () => {};
global.host = mock('host', {
    getMidiInPort: () => mock('in', {
        setMidiCallback: (cb) => { midiCb = cb; },
        setSysexCallback: (cb) => { sysexCb = cb; },
    }),
    getMidiOutPort: () => ({ sendSysex: (h) => sent.push(h) }),
    scheduleTask: (fn, ms) => tasks.push({ fn, at: now + ms }),
    showPopupNotification: (t) => popups.push(t),
    createRelativeHardwareControlAdjustmentTarget: (fn) => ({ __turn: fn, toString: () => 'turnTarget' }),
    println: () => {},
    // Bitwig reports each setting's value once at startup.
    getPreferences: () => ({
        getNumberSetting: (...a) => ({ addRawValueObserver: (cb) => tasks.push({ fn: () => cb(a[6]), at: 0 }) }),
        getEnumSetting: (...a) => ({ addValueObserver: (cb) => tasks.push({ fn: () => cb(a[3]), at: 0 }) }),
    }),
    requestFlush: () => {},
});

eval(src + `
;global.S = {
  init, flush, onMidi: (s, c, v) => onMidi(s, c, v), onSysex: (d) => onSysex(d),
  get knobs() { return knobs; }, get deviceParams() { return deviceParams; },
  get trackBank() { return trackBank; }, get effectBank() { return effectBank; },
  get cursorTrack() { return cursorTrack; }, get cursorDevice() { return cursorDevice; },
  get transport() { return transport; }, get rows() { return rows; }, get mode() { return mode; },
  BUTTON_CC, BUTTON,
};`);

function advance(ms) {
    now += ms;
    for (const t of tasks.splice(0)) {
        if (t.at <= now) t.fn(); else tasks.push(t);
    }
}
function take() { S.flush(); return sent.splice(0); }
function hexStr(s) { return Buffer.from(s, 'ascii').toString('hex'); }
function msg(cmd, ...data) {
    const bytes = [0xf0, 0x7d, 0x6b, 0x62, cmd].concat(data, [0xf7]);
    return bytes.map(b => (b < 16 ? '0' : '') + b.toString(16)).join('');
}
function hasMsg(list, prefixCmd, text) {
    return list.some(h => h.startsWith('f07d6b62' + prefixCmd) && (!text || h.includes(hexStr(text))));
}
const press = (b) => S.onMidi(0xB0, S.BUTTON_CC + b, 127);
const release = (b) => S.onMidi(0xB0, S.BUTTON_CC + b, 0);
const bindingPaths = (k) => S.knobs[k].__state.bindings.map(String);

// ---- start ----
S.init();
advance(1000); // ready
let out = take();
assert(hasMsg(out, '2a', null), 'sends the zoom config');
assert.deepStrictEqual(bindingPaths(0), [String(S.deviceParams[0]), 'turnTarget'], 'knob 1: its remote control + turn reports');
assert.deepStrictEqual(bindingPaths(8), [], 'row 2 unbound with no next page');

// A device with a page of remote controls.
S.cursorTrack.name().set('Bass');
S.cursorDevice.exists().set(true);
S.cursorDevice.name().set('Polysynth');
S.rows[0].pageNames().set(['Filter', 'Env']);
S.rows[0].pageCount().set(2);
S.rows[0].selectedPageIndex().set(0);
const p0 = S.deviceParams[0];
p0.exists().set(true);
p0.name().set('Cutoff');
p0.value().set(0.5);
p0.displayedValue().set('1.0 kHz');
out = take();
assert(hasMsg(out, '22', 'Polysynth'), 'device name');
assert(hasMsg(out, '23', 'Filter'), 'page');
assert(out.includes(msg(0x24, 0, ...Buffer.from('Cutoff'))), 'knob 1 name');
assert(hasMsg(out, '2500', '1.0 kHz'), 'knob 1 value');
assert.deepStrictEqual(bindingPaths(8), [String(S.deviceParams[8]), 'turnTarget'], 'row 2 bound once there is a next page');

// Modulation shows only when it moves the value.
p0.modulatedValue().set(0.5);
out = take();
assert(!out.some(h => h.startsWith('f07d6b622800') && h.length > 14), 'no modulation when equal');
p0.modulatedValue().set(0.75);
out = take();
assert(out.includes(msg(0x28, 0, 95)), 'modulated to 0.75 → 95');

// ---- acceleration ----
const turn = S.knobs[0].__state.bindings[1].__turn;
log.length = 0;
for (let i = 0; i < 5; i++) { advance(200); turn(1 / 64); }
assert.strictEqual(log.length, 0, 'slow turns: no extra');
for (let i = 0; i < 10; i++) { advance(10); turn(1 / 64); } // 100 clicks/s
const incs = log.filter(l => l[0] === 'inc');
assert(incs.length > 0 && incs.every(l => l[1] === String(p0.value()) && l[2] > 0), 'fast turns add on top, same direction');
const last = incs[incs.length - 1][2];
assert(Math.abs(last - 2 / 64) < 1e-9, `gentle: up to 3x, so +2 clicks extra (got ${last * 64})`);
advance(500); log.length = 0; turn(-1 / 64);
assert.strictEqual(log.length, 0, 'after a pause, back to normal speed');

// ---- Fine: double-tap resets the last knob turned ----
log.length = 0;
press(S.BUTTON.FINE); advance(100); release(S.BUTTON.FINE);
advance(150);
press(S.BUTTON.FINE);
assert.deepStrictEqual(log, [['reset', String(p0)]], 'double-tap resets knob 1');
assert(popups[popups.length - 1] === 'Reset Cutoff');
release(S.BUTTON.FINE);
advance(100); press(S.BUTTON.FINE); release(S.BUTTON.FINE);
assert.strictEqual(log.length, 1, 'a third tap right after is not another double-tap');
// Holding Fine while turning, then tapping, is not a tap.
advance(1000); log.length = 0;
press(S.BUTTON.FINE); turn(1 / 640); advance(50); release(S.BUTTON.FINE);
advance(100); press(S.BUTTON.FINE); release(S.BUTTON.FINE);
assert.strictEqual(log.length, 0, 'fine-adjusting does not count as a tap');

// ---- lock ----
advance(1000);
press(S.BUTTON.FINE); press(S.BUTTON.DEVICE_NEXT); release(S.BUTTON.FINE);
assert.strictEqual(S.cursorDevice.isPinned().get(), true);
assert.strictEqual(S.cursorTrack.isPinned().get(), true);
assert(/^Locked to Bass › Polysynth/.test(popups[popups.length - 1]));
S.transport.isPlaying().set(true);
S.transport.tempo().value().set(128.5);
out = take();
assert(out.includes(msg(0x26, 1 | 4, 1285 >> 7, 1285 & 0x7f)), 'status: playing, locked, 128.5 BPM');
// While locked, the box's Track and Device buttons do nothing; Page still works.
nav.length = 0;
for (const b of ['TRACK_PREV', 'TRACK_NEXT', 'DEVICE_PREV', 'DEVICE_NEXT']) {
    press(S.BUTTON[b]); release(S.BUTTON[b]);
}
assert.deepStrictEqual(nav, [], 'locked: track and device buttons blocked');
assert(/^Locked to Bass: Fine \+ Device/.test(popups[popups.length - 1]));
press(S.BUTTON.PAGE_NEXT); release(S.BUTTON.PAGE_NEXT);
assert.strictEqual(nav.length, 1, 'locked: page still works');
advance(1000);
press(S.BUTTON.FINE); press(S.BUTTON.DEVICE_PREV); release(S.BUTTON.FINE);
assert.strictEqual(S.cursorDevice.isPinned().get(), false, 'unlocks');

// ---- mixer ----
const fx0 = S.effectBank.getItemAt(0);
fx0.exists().set(true); fx0.name().set('Reverb');
for (let t = 0; t < 3; t++) {
    const tr = S.trackBank.getItemAt(t);
    tr.exists().set(true);
    tr.name().set(['Drums', 'Bass', 'Keys'][t]);
    tr.position().set(t);
    tr.volume().exists().set(true);
    tr.volume().value().set(0.7);
    tr.volume().displayedValue().set('-1.0 dB');
    tr.sendBank().getItemAt(0).exists().set(true);
}
S.trackBank.getItemAt(1).color().__state.rgb = { red: 1, green: 0, blue: 0 };
S.cursorTrack.position().set(1);
take();
advance(1000);
press(S.BUTTON.FINE); press(S.BUTTON.TRACK_NEXT); release(S.BUTTON.FINE);
assert.strictEqual(S.mode, 'mixer');
const vol0 = S.trackBank.getItemAt(0).volume();
assert.deepStrictEqual(bindingPaths(0), [String(vol0), 'turnTarget'], 'knob 1 → track 1 volume');
const pan0 = S.trackBank.getItemAt(0).pan();
assert.deepStrictEqual(bindingPaths(8), [String(pan0), 'turnTarget'], 'knob 9 → track 1 pan');
out = take();
assert(hasMsg(out, '22', 'Mixer: tracks 1-8'), 'mixer header');
assert(hasMsg(out, '23', 'Volume'), 'mixer page name');
assert(out.includes(msg(0x24, 1, ...Buffer.from('Bass'))), 'knob 2 named after track 2');
assert(out.includes(msg(0x27, 1, 1 | 2, 127, 0, 0)), 'knob 2: red, selected');
assert(out.includes(msg(0x27, 0, 1, 64, 64, 64)), 'knob 1: coloured, not selected');
assert(!out.some(h => h.startsWith('f07d6b622404') && h.length > 14), 'knob 5: no track, stays unnamed');
// Meters: the selected track's, and one per existing track.
S.cursorTrack.__state.vu(100);
S.trackBank.getItemAt(0).__state.vu(60);
out = take();
const meters = out.filter(h => h.startsWith('f07d6b6229')).pop();
assert.strictEqual(meters, msg(0x29, 100, 60, 0, 0, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f), 'meters');
// Pages: volume → pan → send Reverb, and no further.
press(S.BUTTON.PAGE_NEXT); press(S.BUTTON.PAGE_NEXT); press(S.BUTTON.PAGE_NEXT);
assert.deepStrictEqual(bindingPaths(0), [String(S.trackBank.getItemAt(0).sendBank().getItemAt(0)), 'turnTarget'], 'page 3 = send 1');
assert.deepStrictEqual(bindingPaths(8), [], 'no page after the last send');
out = take();
assert(hasMsg(out, '23', 'Send Reverb'));
// Reset in the mixer names the track and parameter.
log.length = 0;
const sendTurn = S.knobs[1].__state.bindings[1].__turn;
advance(1000); sendTurn(1 / 64);
press(S.BUTTON.FINE); release(S.BUTTON.FINE); advance(100); press(S.BUTTON.FINE); release(S.BUTTON.FINE);
assert.deepStrictEqual(log, [['reset', String(S.trackBank.getItemAt(1).sendBank().getItemAt(0))]]);
assert.strictEqual(popups[popups.length - 1], 'Reset Bass Send Reverb');
// Locked in the mixer: Track is blocked, Device still scrolls the 8 tracks.
advance(1000);
press(S.BUTTON.FINE); press(S.BUTTON.DEVICE_NEXT); release(S.BUTTON.FINE);
assert.strictEqual(S.cursorDevice.isPinned().get(), true, 'locks from the mixer');
nav.length = 0;
press(S.BUTTON.TRACK_NEXT); press(S.BUTTON.DEVICE_NEXT);
assert.deepStrictEqual(nav, [String(S.trackBank) + '.scrollPageForwards'], 'mixer locked: only the bank scrolls');
advance(1000);
press(S.BUTTON.FINE); press(S.BUTTON.DEVICE_PREV); release(S.BUTTON.FINE);
assert.strictEqual(S.cursorDevice.isPinned().get(), false);
// Back to devices.
advance(1000);
press(S.BUTTON.FINE); press(S.BUTTON.TRACK_PREV); release(S.BUTTON.FINE);
assert.strictEqual(S.mode, 'device');
assert.deepStrictEqual(bindingPaths(0), [String(p0), 'turnTarget']);
out = take();
assert(out.includes(msg(0x27, 1, 0)), 'device mode: knob styles cleared');
assert(out.some(h => h.startsWith('f07d6b6229') && h.endsWith('7f7f7f7f7f7f7f7ff7')), 'device mode: no knob meters');

// ---- hello resends everything ----
S.onSysex('F07D6B6210F7');
out = take();
assert(out.length > 40, `hello resends everything (${out.length} messages)`);
assert(out.every(h => h.length <= 128), 'every message fits the box (64 bytes)');

console.log('all script checks passed');

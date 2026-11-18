#!/bin/sh
# Reboot a running knobbox into BOOTSEL mode, so `cargo run --release` in
# firmware/ can flash it without unplugging. The box then shows up as the
# RPI-RP2 drive and its LED blinks.
set -eu
port=$(amidi -l | awk '/knobbox/ { print $2; exit }')
if [ -z "$port" ]; then
    echo "knobbox not found in 'amidi -l'. Is it plugged in?" >&2
    exit 1
fi
amidi -p "$port" -S 'F0 7D 6B 62 00 F7'
echo "Sent reboot to $port. Wait for the RPI-RP2 drive to appear."

#!/usr/bin/env bash
# Opt-in USB-export integration test against the XDJ-AZ 1.30 AtEmu model.
# This is intentionally absent from package.json and CI: it needs the private
# AtEmu checkout, imported firmware, QEMU, and host audio sockets.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
ATEMU_ROOT="${ATEMU_DIR:-$HOME/code/alphatheta-emu-private}"
if [[ -x "$ATEMU_ROOT/atemu/bin/atemu" ]]; then
  ATEMU_ROOT="$ATEMU_ROOT/atemu"
fi
ATEMU="$ATEMU_ROOT/bin/atemu"
[[ -x "$ATEMU" ]] || { echo "XDJ-AZ test: no AtEmu at $ATEMU (set ATEMU_DIR)" >&2; exit 2; }
CABINET="${ATEMU_XDJ_AZ_CABINET:-$HOME/code/alphatheta-docs/devices/_other/OneLibrary/cabinet.img}"
[[ -f "$CABINET" ]] || { echo "XDJ-AZ test: no opened cabinet image at $CABINET (set ATEMU_XDJ_AZ_CABINET)" >&2; exit 2; }

WORK="$(mktemp -d -t rbxport-xdj-az.XXXXXX)"
IMAGE="$WORK/usb.img"
MOUNT="$WORK/mnt"
STICK="$MOUNT"
BOOTH="$WORK/atemu-compose.yaml"
OUT="${RBL_XDJ_AZ_OUT:-$REPO/verification/xdj-az/$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$STICK" "$OUT"

cleanup() {
  local status=$?
  set +e
  "$ATEMU" logs az >"$OUT/atemu.log" 2>&1
  "$ATEMU" down -f "$BOOTH" >"$OUT/atemu-down.log" 2>&1
  hdiutil detach "$MOUNT" >/dev/null 2>&1
  if [[ "${RBL_XDJ_AZ_KEEP:-0}" != "1" ]]; then
    python3 -c 'import shutil,sys; shutil.rmtree(sys.argv[1], ignore_errors=True)' "$WORK"
  else
    echo "kept fixture at $WORK"
  fi
  echo "XDJ-AZ evidence: $OUT"
  exit "$status"
}
trap cleanup EXIT

hdiutil create -size 256m -fs 'MS-DOS FAT32' -volname RBXAZ -layout MBRSPUD "$IMAGE.dmg" >/dev/null
mv "$IMAGE.dmg" "$IMAGE"
mkdir -p "$MOUNT"
hdiutil attach -mountpoint "$MOUNT" "$IMAGE" >/dev/null
mkdir -p "$STICK"
(cd "$REPO" && cargo run -q -p rbl-export --example xdj_az_fixture -- "$STICK") \
  | tee "$OUT/fixture.json"
hdiutil detach "$MOUNT" >/dev/null

cat >"$BOOTH" <<EOF
version: 1
name: rbxport-xdj-az-usb
media:
  rbxport: { path: "$IMAGE" }
devices:
  az:
    model: xdj-az
    hardware: rk3399
    number: 1
    cabinet: "$CABINET"
    slots: { usb: rbxport }
EOF

"$ATEMU" up --api --headless --json -f "$BOOTH" | tee "$OUT/atemu-up.json"

# The app API is live before EP147. Wait until the shim has emitted a sustained
# run of input frames and reports mounted media; an API response alone lets the
# test press keys during boot and then grade silence. EP147 does not currently
# expose a decoded TX frame, so tx_seq is not a readiness signal for this model.
RFBDUMP="$ATEMU_ROOT/build/cdj3k-rfbdump_artefacts/Release/cdj3k-rfbdump"
[[ -x "$RFBDUMP" ]] || { echo "XDJ-AZ test: missing $RFBDUMP; build AtEmu first" >&2; exit 2; }
ready=0
for _ in $(seq 1 120); do
  rfb="$($ATEMU port az rfb)"
  if "$ATEMU" exec az state >"$OUT/state.json" 2>/dev/null \
    && grep -Eq '^STATE .*usb=1 .*rx_emitted=([1-9][0-9]{3,})( |$)' "$OUT/state.json" \
    && "$RFBDUMP" 127.0.0.1 "$rfb" "$OUT/0-ready.png" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 2
done
[[ "$ready" == "1" ]] || { echo "XDJ-AZ firmware did not become ready with USB mounted" >&2; exit 1; }
# The first painted frame precedes EP147's media scan. Give DeviceSQL time to
# finish constructing its browser before replaying the insertion. EP147 can
# consume the power-on udev event before the SOURCE view exists.
sleep 25
"$ATEMU" exec az control "USB 0" >/dev/null
sleep 3
"$ATEMU" exec az control "USB 1" >/dev/null
sleep 8

press() {
  local port=$1 byte=$2 mask=$3
  "$ATEMU" exec az control "AZ $port $byte $mask" >/dev/null
  sleep 0.25
  "$ATEMU" exec az control "AZ $port $byte 0" >/dev/null
}

shot() {
  local name=$1 rfb
  rfb="$($ATEMU port az rfb)"
  "$RFBDUMP" 127.0.0.1 "$rfb" "$OUT/$name.png" >/dev/null
}

touch() {
  local x=$1 y=$2
  "$ATEMU" exec az control "TOUCH $x $y" >/dev/null
  sleep 0.25
  "$ATEMU" exec az control "TOUCH 0 0" >/dev/null
}

# [OBS] EP147 1.30 input fields from AtEmu's XDJ-AZ model contract.
# [ASSUME] With one pre-mounted USB export, SOURCE initially highlights that
# source; PLAYLIST then highlights the fixture's sole playlist and track. The
# audio assertion below is the acceptance gate, so a changed UI/default focus
# fails rather than being recorded as a successful mount/load.
press 3 32 128       # SOURCE
sleep 2
shot 1-source
touch 45 120         # USB 1 source tile
sleep 3
shot 2-selected-source
press 3 32 16        # PLAYLIST
sleep 2
shot 3-playlist
press 3 31 1         # open sole playlist
sleep 2
shot 4-open-playlist
press 3 31 1         # select sole track
press 3 33 8         # LOAD 1
sleep 4
shot 5-loaded

CONTROL_PORT="$($ATEMU port az control)"
AUDIO_PORT="$($ATEMU port az audio)"
ATEMU_XDJ_AZ_LIVE=1 CDJ_CONTROL_PORT="$CONTROL_PORT" CDJ_AUDIO_PORT="$AUDIO_PORT" \
  python3 -m pytest -q "$ATEMU_ROOT/tests/test_xdj_az_playback.py" \
  --junitxml="$OUT/results.xml" 2>&1 | tee "$OUT/pytest.log"

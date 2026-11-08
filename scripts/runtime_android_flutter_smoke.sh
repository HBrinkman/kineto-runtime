#!/usr/bin/env bash
# Plays an exported .kvk in the Flutter example on an Android emulator, and checks
# it from outside the app: pixels from the system screenshot, taps through the
# system input service, results from logcat.
#
# Emulator only. It refuses a physical device rather than installing on one.
#
# Usage: ANDROID_SERIAL=emulator-5554 scripts/runtime_android_flutter_smoke.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLE="${ROOT_DIR}/packages/kineto_runtime/example"
PACKAGE="com.example.kineto_runtime_example"
SERIAL="${ANDROID_SERIAL:-}"
ADB=(adb -s "${SERIAL}")

if [[ "${SERIAL}" != emulator-* ]]; then
  echo "error: set ANDROID_SERIAL to an emulator serial (got '${SERIAL}')" >&2
  exit 1
fi
if [[ "$("${ADB[@]}" shell getprop ro.boot.qemu | tr -d '\r')" != "1" ]]; then
  echo "error: ${SERIAL} does not report itself as an emulator" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

(cd "${EXAMPLE}" && flutter build apk --debug \
  --dart-define=KINETO_ASSET=assets/parity/sounds_demo.kvk >/dev/null)
"${ADB[@]}" install -r "${EXAMPLE}/build/app/outputs/flutter-apk/app-debug.apk" >/dev/null

launch() {
  "${ADB[@]}" shell am start -S -W -n "${PACKAGE}/.MainActivity" >/dev/null
}

shot() {
  "${ADB[@]}" exec-out screencap > "${WORK}/shot.raw"
}

# Prints "count cx cy left" for the fixture's shape colour on the raw screenshot.
measure() {
  python3 -I - "${WORK}/shot.raw" <<'PY'
import struct, sys
data = open(sys.argv[1], 'rb').read()
width, height = struct.unpack('<II', data[:8])
pixels = data[16:]
count = sx = sy = 0
left = width
for y in range(0, height, 4):
    row = y * width * 4
    for x in range(0, width, 4):
        r, g, b = pixels[row + x * 4:row + x * 4 + 3]
        if abs(r - 108) < 12 and abs(g - 99) < 12 and abs(b - 255) < 12:
            count += 1
            sx += x
            sy += y
            left = min(left, x)
if count == 0:
    print('0 0 0 0')
else:
    print(count, sx // count, sy // count, left)
PY
}

wait_for_shape() {
  for _ in $(seq 1 30); do
    shot
    read -r COUNT CX CY LEFT <<<"$(measure)"
    if (( COUNT > 50 )); then return 0; fi
    sleep 1
  done
  echo "FAIL: the shape never reached the screen" >&2
  return 1
}

# Asserts logcat holds this line, then forgets it.
expect_log() {
  for _ in $(seq 1 10); do
    if "${ADB[@]}" logcat -d -s flutter:I | grep -qF "$1"; then
      echo "ok: $1"
      "${ADB[@]}" logcat -c
      return 0
    fi
    sleep 0.5
  done
  echo "FAIL: logcat never showed: $1" >&2
  "${ADB[@]}" logcat -d -s flutter:I | tail -10 >&2
  return 1
}

"${ADB[@]}" logcat -c
launch
wait_for_shape
echo "ok: drawn, ${COUNT} sampled pixels around (${CX}, ${CY})"

"${ADB[@]}" shell input tap "${CX}" "${CY}"
expect_log "kineto_smoke pointer=down fired=1 sounds=[event-click]"

"${ADB[@]}" shell input tap $(( LEFT - 40 )) "${CY}"
expect_log "kineto_smoke pointer=down fired=0"

"${ADB[@]}" shell input keyevent KEYCODE_HOME
sleep 2
"${ADB[@]}" shell am start -n "${PACKAGE}/.MainActivity" >/dev/null
wait_for_shape
echo "ok: drawn again after background and resume"

"${ADB[@]}" shell input tap "${CX}" "${CY}"
expect_log "kineto_smoke pointer=down fired=1"
echo "ok: touch still lands after resume"

if "${ADB[@]}" logcat -d -b crash | grep -q "${PACKAGE}"; then
  echo "FAIL: the crash buffer names ${PACKAGE}" >&2
  exit 1
fi
echo "PASS: Flutter smoke on ${SERIAL}. Emulator evidence, not device evidence."

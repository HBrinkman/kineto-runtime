#!/usr/bin/env bash
# Guards the issue #54 runtime link: every Kineto engine C ABI symbol the
# Dart FFI bindings look up via DynamicLibrary.process() must be present in
# the built iOS app binary. dart:ffi references are invisible to the static
# linker, so -dead_strip silently removes unreferenced symbols — the app
# then builds fine but the Export Runtime Logic sheet reports "engine not
# linked" on device. The Flutter plugin's Package.swift and CocoaPods fallback
# podspec pin each symbol as a link root; this script verifies the result after
# a build.
#
# Usage:
#   flutter build ios --release --no-codesign
#   tool/check_runtime_symbols.sh [path/to/Runner]
#
# Keep the symbol list in sync with engine/kineto_engine/src/ffi/c.rs, the
# plugin Package.swift, and the fallback podspec.

set -euo pipefail

BINARY="${1:-build/ios/iphoneos/Runner.app/Runner}"

if [[ ! -f "$BINARY" ]]; then
  echo "error: app binary not found at $BINARY" >&2
  echo "hint: run 'flutter build ios --release --no-codesign' first" >&2
  exit 2
fi

SYMBOLS=(
  _kineto_active_audio_clips
  _kineto_advance
  _kineto_animations
  _kineto_audio_assets
  _kineto_audio_events
  _kineto_destroy
  _kineto_override_scalar
  _kineto_retain
  _kineto_get_active_state
  _kineto_get_active_transition
  _kineto_get_transition_progress
  _kineto_hit_test_document_point
  _kineto_last_error_message
  _kineto_load
  _kineto_machine_inputs
  _kineto_pause
  _kineto_play
  _kineto_document_point
  _kineto_background_color
  _kineto_pointer_event
  _kineto_pointer_moved
  _kineto_render
  _kineto_select_animation
  _kineto_set_bool
  _kineto_set_number
  _kineto_set_trigger
  _kineto_state_machines
  _kineto_surface_create_from_ca_metal_layer
  _kineto_surface_destroy
  _kineto_surface_last_error_message
  _kineto_surface_resize
  _kineto_take_fired_audio_events
)

defined="$(nm -gU "$BINARY" 2>/dev/null || true)"
missing=()
for symbol in "${SYMBOLS[@]}"; do
  if ! grep -q " ${symbol}\$" <<<"$defined"; then
    missing+=("$symbol")
  fi
done

if (( ${#missing[@]} > 0 )); then
  echo "FAIL: ${#missing[@]} engine symbol(s) missing from $BINARY:" >&2
  printf '  %s\n' "${missing[@]}" >&2
  echo "The runtime FFI will fail at startup — check the link-root lists in" >&2
  echo "packages/kineto_runtime/ios/kineto_runtime/Package.swift and the podspec." >&2
  exit 1
fi

echo "OK: all ${#SYMBOLS[@]} Kineto engine symbols present in $BINARY"
